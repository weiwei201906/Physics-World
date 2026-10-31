# 物理答题游戏 · 全栈架构实践文档

---

## 一、命名规范

### 1.1 服务命名（kebab-case）

`gateway`、`auth-service`、`user-api`、`user-api-sse`、`recommendation-service`、`battle-service`、`question-bank`、`attempt-consumer`、`battle-result-consumer`、`profile-bootstrap-consumer`。

### 1.2 各层命名约定

| 层 | 风格 | 示例 |
|---|---|---|
| GraphQL 类型 / gRPC service / method | PascalCase | `QuizSession`、`QuestionBank`、`GetQuestion` |
| GraphQL 字段 / mutation | camelCase | `userId`、`submitAttempt` |
| GraphQL 枚举 | UPPER_SNAKE_CASE | `PRACTICE`、`DAILY` |
| REST 路径 / K8s 资源 | kebab-case | `/auth/refresh-token`、`user-api` |
| Redis key / PG schema / table / column / Kafka topic / Config namespace | snake_case | `battle:room:{id}`、`user_profiles`、`attempts`、`user_api` |
| Config key | 点分小写 | `auth.jwt.access_ttl_sec` |

### 1.3 协议缩写

| 缩写 | 全称 | 使用范围 |
|---|---|---|
| FlareRPC | Flare 框架原生 RPC（同机 IPC-RPC / 跨机 QUIC） | 仅 `question-bank` 内部模块间 |
| gRPC | 标准 gRPC over HTTP/2 | Rust ↔ C++ 对外调用 |
| SSE | Server-Sent Events | 推送通道 |
| WS | WebSocket | 对战通道 |

---

## 二、项目概述与设计原则

本系统是面向学校的物理答题游戏，涵盖单人练习、章节刷题、实时对战、个性化推荐与排行榜。前端采用 Vue 3 + TypeScript，后端由 Rust 与 C++ 混合编写，题库与媒体资源托管在腾讯云 COS，整体部署于 TKE。

### 设计原则

1. **按伸缩特性、故障域、变更频率拆服务**，不按领域实体拆。
2. **同步链路只保留必要调用**。跨服务同步调用仅限 `user-api / recommendation-service / battle-service → question-bank`（gRPC）和 `user-api → auth-service`（低频）。
3. **写路径异步化，读路径同步化**。
4. **状态权威唯一**。对战状态权威在 Redis；身份权威在 `auth-service` 的 PG；题库权威在 COS；画像权威在 `recommendation-service` 的 PG；配置权威在 `config` schema 的 PG。
5. **认证只签发与撤销，`gateway` 独占每请求验签**。`gateway` 验签通过后注入身份 header；下游信任这些 header，NetworkPolicy 禁止绕过 `gateway` 直连 Pod。
6. **扩缩容信号按服务画像选，禁止统一用 CPU**。
7. **配置外置，代码不硬编码**。**所有阈值、超时、TTL、批量大小、并发上限、重试次数、比例、窗口、限流速率、评分系数、SLO 目标等运营可调参数一律从配置读取**，代码中不出现字面量数值。配置分四层管理，schema 动态注册，支持热更新与降级。本文档后续章节在描述行为时，**只引用配置 key，不重复写默认值**——完整默认值见第七章配置清单。
8. **接口优先 GraphQL**。业务查询与变更统一走 GraphQL；仅认证、公开配置拉取、SSE 保留 REST。
9. **媒体直连 CDN**，不经 `gateway` 或后端服务。
10. **登录态无感续签**。用户一次登录后，系统在后台自动完成 access token 刷新，用户在整个 refresh token 有效期内不需要重新输入密码；仅在 refresh token 失效、撤销或主动登出时才需要重新登录。

---

## 三、总体架构

```
┌────────────────────────────────────────────────────────────┐
│ 前端  Vue 3 + TS + Vite + Pinia + Axios                     │
│ 媒体 <img> ────────→ CDN ──→ COS（题目图片/公式渲染图）       │
└────────────────────────────────────────────────────────────┘
         │ /auth  /graphql(×2)  /ws/battle  /events  /config
         ▼
┌────────────────────────────────────────────────────────────┐
│ gateway（Ingress-Nginx + OpenResty Lua，跨 AZ 部署）         │
│ access_by_lua: JWT 验签 + Redis 黑名单 + 限流                │
│ 注入 X-User-Id / X-User-Roles / X-Real-IP / X-Request-Id    │
│ 清除客户端传入的同名 header（含 IP 相关）                     │
│ WS/SSE 场景：从 query 提取 token，日志脱敏 token 参数         │
└──┬─────┬─────┬─────┬─────┬──────┬──────┬──────────────────┘
   ▼     ▼     ▼     ▼     ▼      ▼      ▼
 auth  user  user  recomm battle attempt battle-qbank
       -api  -api- -end   -     -cons  -res   (C++)
             sse          serv   umer  cons
   │     │     │     │     │      │      │      │
   ▼     ▼     ▼     ▼     ▼      ▼      ▼      ▼
┌────────────────────────────────────────────────────────────┐
│ PG（auth / app / battle / question / outbox / config）      │
│ + pgvector（读副本）                                        │
│ Redis 实例 1：对战/会话（noeviction）                        │
│ Redis 实例 2：排行榜/画像/限流/黑名单                        │
│ Redis 实例 3：QBank L2/配置/幂等标记                         │
│ Kafka（attempts / battle_result / user_registered）        │
│ COS + CDN                                                   │
└────────────────────────────────────────────────────────────┘
```

---

## 四、服务清单与职责边界

| 服务 | 语言/框架 | 对外协议 | 认证 | 核心职责 | 明确不做 |
|---|---|---|---|---|---|
| `gateway` | Nginx（`deploy/gateway/default.conf.template`） | HTTP/WS/SSE | JWT 验签（auth_request） | 入口、路径分流、身份注入 | 业务逻辑、媒体代理、GraphQL 缓存 |
| `auth-service` | Rust + Axum + SQLx | REST | — | 注册/登录/刷新/登出/JWKS/撤销 | 每请求验证、profile 存储 |
| `user-api` | Rust + Axum + async-graphql | GraphQL | 信任 gateway header | 用户、会话、提交、排行榜、admin | 异步重计算、attempts 写入、SSE |
| `user-api-sse` | Rust + Axum | SSE | 信任 gateway header | `/events` 端点 + 订阅 `notification:broadcast` + `auth:revoked` | GraphQL、写操作 |
| `recommendation-service` | Rust + Axum + async-graphql | GraphQL | 信任 gateway header | 推荐读、画像 flusher、画像摘要 | 答题提交、排行榜写 |
| `battle-service` | Rust + Axum WS + Tokio | WebSocket | 握手读 gateway header + 新 token 帧验签 + 查黑名单 + 撤销订阅 | 匹配、对战状态机、广播、结算、对账 | 本地持有房间状态、PG 读写 |
| `question-bank` | C++ + Flare + 标准 gRPC C++ 库 | 对外 gRPC / 对内 FlareRPC | mTLS | 题库读、L1/L2/COS 三层缓存 | 碰 PG、检索、组卷、返回媒体二进制 |
| `attempt-consumer` | Rust + rdkafka | — | — | attempts 落库 + 画像 + 排行榜（Lua 原子） | 重算答题结果、处理对战 |
| `battle-result-consumer` | Rust + rdkafka | — | — | 对战战绩落库 + 排行榜（Lua 原子） | 重算胜负、处理单题提交 |
| `profile-bootstrap-consumer` | Rust + rdkafka | — | — | `user_registered` → profile 创建（唯一写者） | 其他事件 |

**实现状态**：鉴权、用户、推荐、对战、网关与三个 Consumer 已有可运行代码；`gateway` 当前为
Nginx 模板（未使用 OpenResty Lua，限流与 JWT 黑名单检查仍在计划中）；`battle-service` 的 JWT
验签与 Redis 撤销订阅仍为 TODO；`question-bank` 为离线 C++ 骨架，Flare/Blade 依赖尚未 vendoring。

**拆分理由**：`user-api-sse` 独立（SSE 长连接、HPA 信号不同）；三个 Consumer 独立（topic 分区数不同，单进程无法独立扩缩）；`recommendation-service` 独立（flusher 后台批量任务与 GraphQL 请求争 PG）；`battle-service` 独立（WS 长连接、有状态、伸缩信号是连接数）。

---

## 五、无感续签（核心机制）

无感续签是整套认证体系的体验基础。用户一次登录后，系统在后台自动完成 access token 刷新，**用户在整个 refresh token 有效期内不需要重新输入密码**。这套机制贯穿 HTTP 请求、WS 长连接、SSE 长连接、页面加载恢复、多 Tab 并发、异常降级六类场景。

### 5.1 Token 双令牌模型

| 令牌 | 格式 | 存储位置 | 有效期来源 | 用途 |
|---|---|---|---|---|
| Access Token | JWT RS256（含 `sub`/`jti`/`iat`/`exp`/`roles`） | **前端内存**（Pinia，不落 localStorage） | `auth.jwt.access_ttl_sec` | 每个业务请求带在 `Authorization: Bearer` |
| Refresh Token | 不透明随机串（服务端存 sha256 哈希） | **httpOnly + Secure + SameSite=Strict Cookie** | `auth.jwt.refresh_ttl_days` | 仅用于换取新 access token；一次性使用 + 轮换 |

**存储策略的核心取舍**：

- **Access Token 存内存**：XSS 攻击者无法从 `document.cookie` 或 `localStorage` 读取，刷新页面即丢失——但刷新页面时有 5.4 的静默恢复机制兜底。
- **Refresh Token 存 httpOnly Cookie**：JavaScript 完全无法读取，XSS 无法窃取。`Secure` 保证仅 HTTPS 传输，`SameSite=Strict` 防止 CSRF。
- **不把 access token 存 Cookie**：Cookie 会随每个请求自动发送，扩大攻击面。
- **不把 refresh token 存 localStorage**：XSS 可直接读取，等于把长期凭证交给攻击者。

### 5.2 刷新接口契约

`auth-service` 的 `POST /auth/refresh-token`：

- **请求**：无 body，Refresh Token 从 httpOnly Cookie 自动携带。
- **响应成功**（200）：新 access token（body）+ 新 refresh token（Set-Cookie 轮换）+ 新 access token 的 `exp`。
- **响应失败**（401）：Cookie 无效、过期、已轮换、已撤销——前端收到 401 后进入 5.7 的降级路径。
- **轮换语义**：每次刷新都作废旧 refresh token、签发新 refresh token。旧的 refresh token 在宽限期（`auth.refresh_grace_sec`）内仍可用，宽限期外重复使用视为盗用并撤销该用户所有 refresh token。
- **限流**：按 `X-Real-IP` + `X-User-Id`（从旧 refresh token 解析）双维度限流，阈值 `auth.refresh_rate_limit_per_min`。

**为什么用独立 REST 端点而不是 GraphQL**：刷新失败需要区分「HTTP 401（凭证无效，需跳登录）」和「GraphQL error（业务错误）」；GraphQL over HTTP 通常返回 200 + errors，前端无法用统一拦截器判断。

### 5.3 前端刷新调度（single-flight）

**请求拦截器**：每个业务请求带 `Authorization: Bearer <access_token>`（从 Pinia 内存读取）。

**响应拦截器（401 触发）** ：

1. 收到 HTTP 401 → 检查是否是刷新请求本身——是则直接进入 5.7 降级。
2. 检查是否已有刷新在飞：
   - **已有**：把当前请求推入等待队列，订阅刷新结果。
   - **没有**：标记「刷新中」，发起刷新请求，成功后取出队列中的所有请求重放（带新 access token），失败则队列中所有请求返回错误并进入 5.7。
3. 业务层 GraphQL error（HTTP 200 + errors）**不触发刷新**，直接展示错误。

**主动刷新定时器**：在 access token 的 `exp` 前 `auth.frontend.pre_refresh_sec` 秒主动触发一次刷新（不等 401），减少 401 发生频率。

**为什么需要 single-flight**：一次页面渲染可能并发发起多个 GraphQL 请求，若都过期则同时 401；若都触发刷新，refresh token 轮换会互相踢掉。single-flight 保证只有一个刷新在飞。

### 5.4 页面加载静默恢复

用户刷新页面或重新打开标签页时，内存中的 access token 丢失，但 httpOnly Cookie 中的 refresh token 还在。前端启动流程：

1. 应用挂载前，先调 `POST /auth/refresh-token`。
2. **成功**：拿到新 access token 存入内存，用户**直接进入已登录状态**。
3. **失败（401）**：视为未登录，跳登录页。

加载期间显示轻量骨架屏，刷新请求超时设 `auth.frontend.silent_restore_timeout_sec`，超时视为失败跳登录。

### 5.5 多 Tab 协同

同一浏览器打开多个标签页时，refresh token Cookie 是共享的，但 access token 在各自内存中独立。

**解决方案**：

- **每个 Tab 独立维护自己的 access token**（内存）。
- **刷新通过 `BroadcastChannel` 广播**：Tab A 刷新成功后广播 `{type: "token_refreshed", exp: <新过期时间>}`。
- **其他 Tab 收到广播不立即刷新**，只更新本地「已知 token 过期时间」；等到自己的 access token 真过期或主动定时器触发时才刷新，此时用共享 Cookie 中的最新 refresh token，不会冲突。
- **宽限期作为兜底**：`auth.refresh_grace_sec` 覆盖「Tab A 刷新成功但广播还没到 Tab B、Tab B 恰好同时触发刷新」的极小概率窗口。

### 5.6 长连接续签（WS / SSE）

**WS（`battle-service`）** ：

- **握手时**：`gateway` 从 URL query 提取 token 验签，注入 `X-User-Id` 到 upstream；`battle-service` 从 header 建立连接身份，记录 `token_exp`。
- **连接期间**：在 `exp` 前 `battle.token_refresh_ahead_sec` 秒，`battle-service` 通过 WS 消息发送 `{type: "token_refresh_required"}`。
- **客户端收到提示**：调用 `POST /auth/refresh-token` 获取新 access token，通过 WS 消息帧 `{type: "token_update", token: "<新access>"}` 发回。
- **`battle-service` 收到新 token 帧**：**自行验签 + 查 Redis 黑名单 `revoked:{jti}`**（`gateway` 之外唯一合理的黑名单查询点）。验签通过更新连接的 `token_exp` 和 `jti`。
- **token 过期后未刷新**：拒绝该连接上的提交答案，返回错误要求刷新。过期后 `battle.token_expired_grace_sec` 内未刷新则强制断开连接，客户端重连到任意 Pod 从 Redis 拉状态恢复。
- **撤销传播**：订阅 `auth:revoked` 频道主动断开；**pub/sub 丢失兜底**为每 `battle.revoked_version_poll_sec` 秒轮询 `auth:revoked_version` 计数器，版本号变化则批量重新校验所有连接的 `jti`。

**SSE（`user-api-sse`）** ：

- **连接建立时**：`gateway` 验签注入 `X-User-Id`，`user-api-sse` 从 header 建立连接身份。
- **连接期间**：SSE 是单向长连接，**不需要续签**——连接已建立，access token 过期不影响推送。
- **重连时**：前端确保先刷新 access token 再重连；`gateway` 从 query 提取 token 验签，日志脱敏。
- **撤销**：订阅 `auth:revoked`，收到后断开该用户所有本地 SSE 连接。

### 5.7 刷新失败的降级路径

| 触发条件 | 降级动作 |
|---|---|
| 刷新请求返回 401（refresh token 无效/过期/已撤销） | 清内存 access token + 清 Pinia 用户态 + 跳登录页 |
| 刷新请求超时（`auth.frontend.silent_restore_timeout_sec`） | 视为失败，同上 |
| 刷新成功但业务请求重放后仍 401 | 说明新 token 立即失效（账号被封禁），同上 |
| 主动登出 | 调 `POST /auth/logout` → 服务端撤销 refresh token + 加 access token 的 `jti` 到黑名单 → 清前端状态 → 跳登录 |
| 收到 SSE/WS 的 `revoked` 事件 | 清前端状态 → 跳登录（不等待下次刷新） |

**降级的一致性要求**：无论从哪条路径降级，前端都要**原子地**清除 access token、用户态、battle/sse 连接、轮询定时器。

### 5.8 时钟偏移容忍

- **`gateway` 验签**：允许 `auth.jwt.leeway_sec` 容忍——`exp` 后的容忍期内 token 仍被认为有效。
- **`battle-service` 的刷新提示**：基于 `exp - now < battle.token_refresh_ahead_sec` 判断。
- **前端主动刷新定时器**：基于服务端返回的 `exp` 绝对时间戳计算，而非本地时钟。
- **前端收到 401 的优先级**：即使本地认为 token 还有效，**服务端返回的 401 是权威**，立即触发刷新。

### 5.9 刷新机制的可观测性

| 指标 | 说明 | 告警阈值来源 |
|---|---|---|
| `auth_refresh_total{result}` | 刷新成功/失败/限流计数 | 失败率 > `alerts.auth_refresh_failure_ratio` |
| `auth_refresh_rate_limited_total` | 限流拒绝计数 | 非零告警 |
| `auth_refresh_rotation_conflict_total` | 宽限期外旧 token 重复使用计数 | 非零告警 |
| `auth_refresh_grace_used_total` | 宽限期内旧 token 使用计数 | 持续非零说明多 Tab 并发频繁 |
| `frontend_token_refresh_success_total` / `frontend_token_refresh_failed_total` | 前端上报 | 失败率 > `alerts.frontend_refresh_failure_ratio` |
| `frontend_silent_restore_total{result}` | 页面加载静默恢复 | 失败率突增 |
| `battle_token_refresh_hint_sent_total` | WS 刷新提示发送计数 | — |
| `battle_token_expired_rejected_total` | token 过期后拒绝操作计数 | 突增 |

**审计**：每次 refresh token 轮换、撤销、刷新失败都写 `login_audit`（含 IP、UA、结果、宽限期使用标记）。

### 5.10 无感续签的边界与不变式

1. **Access Token 只在内存**，绝不落 localStorage；刷新页面依靠 5.4 静默恢复。
2. **Refresh Token 只在 httpOnly Cookie**，JavaScript 不可读；同一时刻只有一个有效 refresh token，宽限期覆盖多 Tab 冲突。
3. **刷新端点只返回 200 / 401**；前端 Axios 只对 401 触发刷新。
4. **single-flight 保证只有一个刷新在飞**，其余请求排队等待重放。
5. **WS 新 token 帧验签必须查黑名单**；SSE 连接期间不续签、重连时带最新 token。
6. **刷新失败统一降级到登录页**，前端原子清除所有登录相关状态。
7. **服务端时钟是权威**，前端 `exp` 基于服务端返回值计算。

---

## 六、各服务详细设计

**统一约定**：本章所有服务的行为参数（超时、TTL、批量、并发、阈值、比例等）均从配置读取，正文只引用配置 key。完整默认值见第七章配置清单。

### 6.1 `gateway`

**信任边界**：

- **`gateway` 独占 JWT 验签和黑名单查询**。验签通过后注入 `X-User-Id` / `X-User-Roles` / `X-Real-IP`（取自 TCP 连接真实源 IP，覆盖客户端传入值）/ `X-Request-Id` / `X-Trace-Id`。
- **清除客户端传入的所有同名 header**，包括 `X-Forwarded-For`、`X-Real-IP`、`X-User-*`、`X-Request-Id`、`X-Trace-Id`。
- **NetworkPolicy 强制**：所有对外服务的入站流量只允许来自 `gateway` Pod。
- **唯一下游验签例外**：`battle-service` 在 WS 连接期间收到客户端新 token 帧时自行验签 + 查黑名单。

**路由与 Ingress 拆分**：

| 路径 | 上游 | 关键配置来源 |
|---|---|---|
| `/auth/*` | `auth-service` | `gateway.timeout.auth_sec`；登录/注册/刷新限流 |
| `/graphql` | `user-api` | `gateway.body_limit.graphql_bytes`；禁止 URL 缓存 |
| `/rec/graphql` | `recommendation-service` | `gateway.timeout.rec_sec` |
| `/events` | `user-api-sse` | 独立 Ingress：`gateway.ws_sse.proxy_buffering_off` + `gateway.ws_sse.read_timeout_sec` |
| `/ws/battle` | `battle-service` | 独立 Ingress：`gateway.ws_sse.read_timeout_sec` + `gateway.ws_sse.proxy_buffering_off` |
| `/config` | `user-api` | `gateway.rate_limit.config_user_per_min` / `gateway.rate_limit.config_ip_per_min` |
| `/media/*` | 不存在 | 媒体由前端直连 CDN |

**WS/SSE 场景的 token 提取**：浏览器 WS API / EventSource 不能设自定义 header，token 在 URL query 中传递。`gateway` 从 query 提取 token 验签 → 注入 `X-User-Id` 到 upstream → **访问日志必须脱敏 query 中的 token 参数**（`gateway.log.mask_query_token`）。

**GraphQL 鉴权失败语义**：`gateway` 验签失败返回 **HTTP 401**；业务层鉴权失败返回 **GraphQL error**（HTTP 200 + errors）。

**`/config` 限流**：已登录按 `X-User-Id` 限流（`gateway.rate_limit.config_user_per_min`）；未登录按 `X-Real-IP` 限流（`gateway.rate_limit.config_ip_per_min`）。**限流为单 Pod 生效**（shared dict 边界），全局配额 = 副本数 × 单 Pod 阈值。响应带 `gateway.cache.config_max_age_sec`。

**Lua 原子筛选**：JWT 本地验签（JWKS 公钥缓存 shared dict，TTL `gateway.jwks.cache_ttl_sec`）、黑名单 cosocket 查 Redis、IP 级令牌桶限流、header 清理与注入。

**配置外置**：Lua 阈值从 `config:gateway` 快照读取，启动加载到 shared dict，每 `gateway.config.refresh_interval_sec` 轮询或收到变更通知立即刷新。

### 6.2 `auth-service`

**接口**（REST，OAuth2 风格）：`/auth/register`、`/auth/login`、`/auth/refresh-token`、`/auth/logout`、`/.well-known/jwks.json`、内部 gRPC `GetUserBasic`。

**Token 设计**：Access Token 为 JWT RS256（有效期 `auth.jwt.access_ttl_sec`）；Refresh Token 为不透明随机串（存 PG，一次性使用 + 轮换，存 sha256 哈希，有效期 `auth.jwt.refresh_ttl_days` 天）。撤销写 Redis 黑名单 + 发 pub/sub 通知。密钥轮换采用双公钥策略（`kid` 标识），轮换周期 `auth.jwt.rotate_days`。

**注册流程**：写用户记录（argon2id，参数 `auth.argon2.*`）→ 发 `user_registered` 事件 → 同步返回 token（同时 Set-Cookie 写入 refresh token）。`profile-bootstrap-consumer` 异步创建 profile（唯一写者）。

**安全**：登录响应时间恒定化、email + IP 双维度限流（`auth.login_rate_limit.*`）、登录审计、refresh 端点单独限流（`auth.refresh_rate_limit_per_min`）、宽限期轮换（`auth.refresh_grace_sec`）、密码最小长度（`auth.password.min_length`）。

**扩缩容**：HPA 按请求速率。NetworkPolicy 只允许 `gateway` 和内部 gRPC。

### 6.3 `user-api`

**GraphQL 端点**：`/graphql`，覆盖用户、会话、提交、排行榜、admin。

**submitAttempt 同步路径**：

1. 从 `X-User-Id` 取用户身份。
2. 校验 session / question_id 基本合法性。
3. 生成 `attempt_id`（UUID）。
4. 同一 PG 事务：更新会话状态 + 写 outbox 记录。
5. 事务提交后立即返回 `{ accepted: true, attemptId }`。

**前端一致性策略**：提交后前端依赖 mutation 返回值做乐观 UI，不立即查询 attempts。允许 `user_api.submit.consistency_window_sec` 的落库延迟；超过该窗口的查询走补偿。

**排行榜模块**（双 key 方案）：日榜 `leaderboard:daily:{date}`、周榜 `leaderboard:weekly:{week}`、总榜 `leaderboard:all`。每日/每周 00:00 创建新 key 并发布通知；旧 key 保留 `user_api.leaderboard.old_key_retention_hours` 小时。Top N 本地缓存每 Pod 缓存 Top `user_api.leaderboard.top_cache_size`，TTL `user_api.leaderboard.top_cache_ttl_sec`。

**admin 子模块**（全部走 GraphQL）：题库管理（写 COS + 版本元数据软删 + 缓存失效 + 审计）、配置管理（写入、schema 注册、历史查询、回滚）。

**Admin 权限检查**：`X-User-Roles` 含 `admin`，且 **`X-Real-IP`** 在 `user_api.admin.allowed_cidrs` 内。

**profile 懒创建**：`profile-bootstrap-consumer` 是 `app.user_profiles` 的唯一写者。`user-api` 在 `me` 查询找不到 profile 时，返回默认 profile 并发送补偿事件到 `user_registered` topic。

**扩缩容**：请求速率 + PG 连接池排队数。不看 CPU。

### 6.4 `user-api-sse`

**职责**：仅提供 `/events` SSE 端点。

- 从 `X-User-Id` 取用户身份。
- 订阅 Redis pub/sub：`notification:broadcast`（业务推送）+ `auth:revoked`（撤销时断开该用户所有本地 SSE 连接）。
- 连接期间**不需要续签**（SSE 单向，token 过期不影响已建立的推送）。
- 重连时前端确保先刷新 access token（`gateway` 从 query 提取验签，日志脱敏）。
- 断线后前端主动调 GraphQL 拉最新状态，SSE 不补发。
- 不持有业务数据，不查 PG。

**扩缩容**：SSE 连接数 / 每 Pod 目标连接数（`user_api_sse.max_connections_per_pod`，KEDA custom metrics）。

### 6.5 `recommendation-service`

**GraphQL 端点**：`/rec/graphql`。

**推荐读路径**：SLO `recommendation.slo.read_p99_ms`。读 PG 画像向量 → 读 Redis 增量 → 内存合并 → L2 归一化 → pgvector HNSW 查询（参数 `recommendation.pgvector.m` / `recommendation.pgvector.ef_construction`）→ QBank 批量取题。PG 每 Pod 连接池上限 `recommendation.pg.pool_size_per_pod`；向量查询路由到 PG 读副本。

**Flusher 三层触发**：空闲批量（`recommendation.flusher.idle.*`）/ 定时兜底（`recommendation.flusher.timer.*`）/ 阈值紧急（`recommendation.flusher.emergency.*`）。单用户流程：Lua 原子取走增量 → 读 PG 向量 → 累加 → L2 归一化 → 更新 PG。用户级锁（`recommendation.flusher.lock_ttl_sec`）防并发；先清标记再写 PG（丢一批增量可接受）。并发上限 `recommendation.flusher.max_concurrency`。

**推荐读永远「读时合并」**：不假设 flusher 已跑完。

**画像维度**：由 `recommendation.recommend.chapter_dims` 决定；答错增量 `recommendation.recommend.delta.wrong`，答对衰减 `recommendation.recommend.delta.correct`。

**扩缩容**：请求速率 + 向量查询 P99。扩副本不减轻 PG 压力，扩缩需结合 PG 读副本容量评估。

### 6.6 `battle-service`

**Redis 键空间**（对战/会话使用独立 Redis 实例，`noeviction`）：

| Key 模式 | 类型 | 内容 |
|---|---|---|
| `battle:room:{id}` | Hash | 状态、模式、当前题序号、时间戳 |
| `battle:room:{id}:players` | Hash | 玩家得分、连接状态、最后活跃时间 |
| `battle:room:{id}:questions` | List | 题目 id 列表 |
| `battle:room:{id}:content` | Hash | 题目 id → 完整题目 JSON |
| `battle:room:{id}:answers` | Hash | `{题序号}:{玩家}` → 答案 + 提交时间 |
| `battle:user:{uid}` | String | 当前房间 id（SET NX 防重复进房） |
| `battle:queue:{mode}` | List | 匹配队列 |
| `battle:broadcast` | Pub/Sub | 跨 Pod 广播频道 |
| `battle:finished` | ZSET | member=battle_id, score=结算时间戳 |

**状态机**：`waiting → matching → playing → settling → finished`。每个状态转换用 Lua 脚本原子执行。

**对局参数**：每局题数 `battle.match.questions_per_room`、每题时限 `battle.match.per_question_sec`、整局时限 `battle.match.total_sec`、房间 TTL `battle.match.room_ttl_sec`、匹配超时 `battle.match.queue_timeout_sec`、单 Pod 房间上限 `battle.match.max_rooms_per_pod`。

**评分规则**：答对 `battle.score.correct`、答错 `battle.score.wrong`、速度加成上限 `battle.score.speed_bonus_max`。

**认证与续签**：握手读 `gateway` 注入的 header；连接期间 token 刷新见 5.6（提前量 `battle.token_refresh_ahead_sec`）；新 token 帧验签 + 查黑名单；订阅 `auth:revoked` + 每 `battle.revoked_version_poll_sec` 轮询 `auth:revoked_version` 兜底；token 过期后拒绝提交，`battle.token_expired_grace_sec` 内未刷新强制断开。

**房间生命周期**：结算后房间键保留 `battle.match.room_ttl_sec`（重连窗口），之后主动删除。结算时同时 ZADD `battle:finished`。

**对账任务**：Redis 锁选主（`SET battle:reconcile:lock {pod_id} NX EX battle.reconcile.lock_ttl_sec`）；每 `battle.reconcile.interval_sec` 从 `battle:finished` ZSET 按时间窗口（`ZRANGEBYSCORE ... LIMIT 0 battle.reconcile.batch_size`）批量取；PG 批量查询差集；重发成功后 `ZREM`；超过 `battle.reconcile.stale_threshold_sec` 未对账的记录告警，不自动恢复。

**开局拉题**：配对成功后调 `question-bank` 取候选和完整题目写入房间内容。失败则取消匹配。

**结算**：写 finished 状态 + ZADD `battle:finished` → 发 Kafka 结算事件。

**扩缩容**：WS 连接数 / 每 Pod 目标连接数（`battle.ws.max_connections_per_pod`，KEDA）。心跳间隔 `battle.ws.ping_interval_sec`。

### 6.7 `question-bank`

**双协议栈边界**：对外标准 gRPC + mTLS；对内 FlareRPC（同机 IPC-RPC / 跨机 QUIC）。

**对外 gRPC 实现**：标准 gRPC C++ 异步 API 暴露接口，handler 投递任务到 Flare Fiber 调度器执行。桥接队列有界（`qbank.grpc.fiber_bridge_queue_size`），最大并发 Fiber `qbank.grpc.max_concurrent_fibers`。

**内部模块**：Query Service（对外 gRPC + L1 LRU）、Cache Proxy（L2 Redis）、COS Fetcher（COS 回源 + singleflight）。初期同进程，可拆。抽象层（`CacheBackend` / `FetcherBackend` trait）在初期就位。

**三层缓存**：

- **L1 进程内 LRU**：容量 `qbank.l1.capacity_mb`，订阅 `qbank:invalidate` 清 L1。
- **L2 Redis**（独立 Redis 实例 3）：单题 TTL `qbank.l2.question_ttl_sec`、章节 TTL `qbank.l2.chapter_ttl_sec`、元数据 TTL `qbank.l2.meta_ttl_sec`，批量读用 MGET / pipeline。
- **L3 COS**：GetObject + per-key singleflight（并发上限 `qbank.cos.singleflight_max`），超时 `qbank.cos.timeout_ms`，失败返回 stale 数据（最长服务时间 `qbank.cos.stale_serve_max_sec`）。

**元数据来源与批量回源**：元数据权威源是 COS 对象的 metadata。批量 `GetQuestionMeta` 先 `MGET qbank:meta:{id}` 批量查 L2 → 未命中的批量并发 COS HEAD（并发上限 `qbank.cos.head_concurrency`）→ 回填 L2 → 返回。**admin 写入顺序**：先写 COS metadata，成功后再写 PG `question_versions`；COS 成功但 PG 失败时触发补偿任务（每 `qbank.cos.compensation_interval_sec` 扫描 COS 中 `updated_at` 晚于 PG 的对象，补写 PG）。

**缓存一致性**：管理侧更新 → 写 COS → 删 Redis 键 → 发失效 pub/sub → 各 Pod 清 L1。

**扩缩容**：HPA 信号 = Redis 命令延迟 P99 + L1 命中率。副本上限 `qbank.hpa.max_replicas`。新 Pod 从 `qbank:hot:*` 预热 Top `qbank.l1.warmup_top_n` 题目。L1 命中率低于 `qbank.alerts.l1_hit_ratio` 告警。

**配置读取**：从 Redis 读 `config:qbank`（每 `qbank.config.refresh_interval_sec` 轮询或订阅变更）。

### 6.8 Consumers（三个独立 Deployment）

**`attempt-consumer`**（`attempts` topic，key = `user_id`）：三步操作，每步累加 + 标记进同一段 Lua 原子执行：

| 步骤 | Lua 脚本逻辑 | 标记 TTL 来源 |
|---|---|---|
| 1. PG 落库 | `INSERT ... ON CONFLICT (attempt_id) DO NOTHING` | 主键本身幂等 |
| 2. 画像增量 | `if not EXISTS attempt:redis:image:{id} then HINCRBY ...; ZADD ...; SET ... end` | `consumers.idempotency_ttl_sec` |
| 3. 排行榜 | `if not EXISTS attempt:redis:rank:{id} then ZINCRBY ...; SET ... end` | `consumers.idempotency_ttl_sec` |

标记检查、累加、写标记在同一 Lua 脚本中原子执行。

**`battle-result-consumer`**（`battle_result` topic，key = `battle_id`）：落 `battles` + `battle_participants`（`ON CONFLICT DO NOTHING`）；排行榜 Lua 原子执行 `if not EXISTS battle:processed:{battle_id} then ZINCRBY ...; SET ... end`；不重算胜负。

**`profile-bootstrap-consumer`**（`user_registered` topic，key = `user_id`）：创建 `user_profiles` 记录（`ON CONFLICT DO NOTHING`）。**唯一写者**；`user-api` 的懒创建通过发送补偿事件到此 topic 实现。

**DLQ**：重试 `consumers.max_retries` 次后 produce 到 DLQ + 提交 offset。重试计数 TTL `consumers.retry_ttl_sec`。DLQ 独立告警。

**扩缩容**：各自 KEDA Kafka scaler，`lagThreshold` = `consumers.keda.lag_threshold`，副本上限 = 各自 topic 分区数。

**Kafka 消费参数**：poll 批量 `consumers.batch_size`、offset 提交间隔 `consumers.commit_interval_ms`、offset 重置策略 `consumers.offset_reset_policy`。

---

## 七、配置管理

本章是**全部可调参数的权威清单**。代码中不出现字面量数值，各服务启动时通过静态清单声明依赖的 namespace/key，缺失时用默认值兜底并告警。

### 7.1 配置分层

| 层级 | 内容 | 存储 | 加载方式 | 热更新 |
|---|---|---|---|---|
| L1 环境配置 | DB/Redis/Kafka 地址、端口、日志级别、证书路径 | K8s ConfigMap | 启动时环境变量 | 部分支持 |
| L2 密钥 | JWT 私钥、DB 密码、COS 凭证 | ESO + 云 KMS | 启动时注入，定期刷新 | 支持 |
| L3 动态业务配置 | 本章全部参数 | PG `config` schema + Redis | 启动读 Redis + pub/sub 热更新 | 支持 |
| L4 前端配置 | API 地址、CDN 域名、功能开关、UI 文案 | 构建时 + 运行时 `/config` | 启动拉取 + 轮询 | 支持 |

### 7.2 L1 环境配置（K8s ConfigMap）

每服务一个 ConfigMap，只放非敏感、启动时必须、极少变更的内容：

| Key 模式 | 说明 |
|---|---|
| `LOG_LEVEL` | 日志级别 |
| `PG_HOST` / `PG_PORT` / `PG_DATABASE` | PG 连接 |
| `REDIS_HOST_1/2/3` / `REDIS_PORT` | 三个 Redis 实例地址 |
| `KAFKA_BROKERS` | Kafka broker 地址 |
| `QBANK_GRPC_ADDR` / `AUTH_GRPC_ADDR` | 内部 gRPC 地址 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTel 收集器地址 |
| `CONFIG_REFRESH_INTERVAL_SEC` | 配置轮询兜底间隔 |

### 7.3 L2 密钥（ESO + KMS）

JWT 私钥、DB/Redis/Kafka 凭证从 KMS 经 ESO 注入。COS 凭证用 TKE 服务角色，不走 Secret。

### 7.4 L3 动态业务配置

**数据模型**：`config.configs`（namespace + key 唯一）、`config.config_audit`、`config.schemas`。

**Redis 缓存**（独立 Redis 实例 3）：每 namespace 一个 JSON 快照 `config:{namespace}` + 一个变更通知频道 `config:changed:{namespace}`。

**快照结构**：包含 `based_on_pg_max_updated_at`（快照基于的 PG configs 最大 `updated_at`）、`written_at`、`values`。对账比较 `based_on_pg_max_updated_at`。

**写入流程**：校验 schema → PG 事务写 configs + audit → 写 Redis 快照（重试 `config.write_retry_times` 次）→ 发 pub/sub → 返回。

**读取流程**：启动 GET Redis 快照，为空则从 PG 回填；订阅变更频道更新本地缓存。

**降级**：Redis 不可用 → 用启动时本地快照；PG 不可用 → 写入拒绝，读取走 Redis 快照；双不可用 → 内置默认值。

**定时对账**：每 `config.reconcile_interval_sec` 比对 PG 最大值与快照 `based_on_pg_max_updated_at`，不一致则回填。

**最坏生效时间 SLO**（pub/sub 丢失时靠兜底轮询）：`gateway` `gateway.config.refresh_interval_sec`、`question-bank` `qbank.config.refresh_interval_sec`、其他 Rust 服务 `config.effective_slo_sec`、前端 `web.config.poll_interval_sec`。告警阈值 = SLO × `alerts.config_effective_slo_multiplier`。

### 7.5 完整配置项清单

#### `auth`（认证）

| Key | 默认值 | 说明 |
|---|---|---|
| `auth.jwt.access_ttl_sec` | 900 | Access Token 有效期 |
| `auth.jwt.refresh_ttl_days` | 30 | Refresh Token 有效期 |
| `auth.jwt.rotate_days` | 90 | 密钥轮换周期 |
| `auth.jwt.leeway_sec` | 60 | 时钟偏移容忍 |
| `auth.refresh_grace_sec` | 5 | Refresh 轮换宽限期 |
| `auth.refresh_rate_limit_per_min` | 10 | Refresh 端点限流（每用户+IP） |
| `auth.login_rate_limit.per_email` | 5 | 单邮箱登录失败阈值（15 分钟） |
| `auth.login_rate_limit.per_ip` | 20 | 单 IP 登录失败阈值（15 分钟） |
| `auth.login_rate_limit.window_sec` | 900 | 登录失败统计窗口 |
| `auth.password.min_length` | 8 | 密码最小长度 |
| `auth.argon2.memory_kb` | 65536 | argon2 内存成本 |
| `auth.argon2.iterations` | 3 | argon2 迭代次数 |
| `auth.argon2.parallelism` | 4 | argon2 并行度 |
| `auth.frontend.pre_refresh_sec` | 120 | 前端主动预刷新提前量 |
| `auth.frontend.silent_restore_timeout_sec` | 5 | 静默恢复超时 |
| `auth.cookie.same_site` | `Strict` | Refresh Cookie SameSite |
| `auth.cookie.max_age_days` | 30 | Refresh Cookie Max-Age |

#### `gateway`

| Key | 默认值 | 说明 |
|---|---|---|
| `gateway.jwks.cache_ttl_sec` | 300 | JWKS 公钥缓存 TTL |
| `gateway.config.refresh_interval_sec` | 60 | 配置轮询兜底间隔 |
| `gateway.timeout.auth_sec` | 30 | `/auth/*` 代理超时 |
| `gateway.timeout.rec_sec` | 30 | `/rec/graphql` 代理超时 |
| `gateway.body_limit.graphql_bytes` | 1048576 | GraphQL body 大小上限 |
| `gateway.ws_sse.read_timeout_sec` | 3600 | WS/SSE 代理读超时 |
| `gateway.ws_sse.proxy_buffering` | `off` | WS/SSE 关闭代理缓冲 |
| `gateway.rate_limit.config_user_per_min` | 600 | `/config` 已登录限流 |
| `gateway.rate_limit.config_ip_per_min` | 120 | `/config` 未登录限流 |
| `gateway.rate_limit.ip_per_min` | 300 | 通用 IP 级限流 |
| `gateway.cache.config_max_age_sec` | 60 | `/config` 响应缓存头 |
| `gateway.log.mask_query_token` | `true` | WS/SSE token 日志脱敏 |
| `gateway.pod.max_ws_sse_connections` | 5000 | 单 Pod WS+SSE 连接上限 |

#### `user_api`

| Key | 默认值 | 说明 |
|---|---|---|
| `user_api.submit.consistency_window_sec` | 3 | 提交后 attempts 落库延迟窗口 |
| `user_api.session.max_duration_sec` | 3600 | 单次答题会话最长时长 |
| `user_api.session.max_questions` | 50 | 单次会话最多题目数 |
| `user_api.leaderboard.top_cache_size` | 100 | Top N 缓存条数 |
| `user_api.leaderboard.top_cache_ttl_sec` | 60 | Top N 缓存 TTL |
| `user_api.leaderboard.old_key_retention_hours` | 24 | 旧榜期 key 保留时间 |
| `user_api.leaderboard.page_size_max` | 100 | 分页大小上限 |
| `user_api.leaderboard.periods` | `["daily","weekly","all"]` | 支持的榜期 |
| `user_api.admin.allowed_cidrs` | `["10.0.0.0/8"]` | 管理路由内网白名单 |
| `user_api.sse.max_connections_per_pod` | 5000 | 单 Pod SSE 连接上限 |
| `user_api.pg.pool_size_per_pod` | 20 | PG 连接池上限 |

#### `user_api_sse`

| Key | 默认值 | 说明 |
|---|---|---|
| `user_api_sse.max_connections_per_pod` | 5000 | 单 Pod SSE 连接上限 |
| `user_api_sse.shutdown.drain_sec` | 30 | 优雅关闭 drain 时间 |

#### `recommendation`

| Key | 默认值 | 说明 |
|---|---|---|
| `recommendation.slo.read_p99_ms` | 100 | 推荐读 P99 目标 |
| `recommendation.recommend.default_count` | 10 | 默认推荐题目数 |
| `recommendation.recommend.max_count` | 50 | 推荐题目数上限 |
| `recommendation.recommend.chapter_dims` | 8 个章节 | 画像维度定义 |
| `recommendation.recommend.delta.wrong` | 1.0 | 答错增量 |
| `recommendation.recommend.delta.correct` | 0.2 | 答对衰减 |
| `recommendation.pgvector.m` | 16 | HNSW m 参数 |
| `recommendation.pgvector.ef_construction` | 64 | HNSW ef_construction |
| `recommendation.pg.pool_size_per_pod` | 20 | PG 连接池上限 |
| `recommendation.flusher.idle.lag_threshold` | 50 | 空闲判定的 Kafka lag 阈值 |
| `recommendation.flusher.idle.qps_threshold` | 100 | 空闲判定的 QPS 阈值 |
| `recommendation.flusher.idle.batch_size` | 800 | 空闲批量 |
| `recommendation.flusher.timer.interval_sec` | 30 | 定时兜底间隔 |
| `recommendation.flusher.timer.batch_size` | 100 | 定时兜底批量 |
| `recommendation.flusher.emergency.threshold` | 5000 | 紧急触发 dirty 队列阈值 |
| `recommendation.flusher.max_concurrency` | 8 | flusher 并发上限 |
| `recommendation.flusher.lock_ttl_sec` | 30 | 用户级锁 TTL |

#### `battle`

| Key | 默认值 | 说明 |
|---|---|---|
| `battle.match.questions_per_room` | 10 | 每局题目数 |
| `battle.match.per_question_sec` | 30 | 每题时限 |
| `battle.match.total_sec` | 600 | 整局时限 |
| `battle.match.room_ttl_sec` | 7200 | 房间键 TTL（含重连窗口） |
| `battle.match.queue_timeout_sec` | 60 | 匹配超时 |
| `battle.match.max_rooms_per_pod` | 1000 | 单 Pod 房间上限 |
| `battle.token_refresh_ahead_sec` | 120 | token 刷新提示提前量 |
| `battle.token_expired_grace_sec` | 300 | token 过期后未刷新强制断开宽限 |
| `battle.revoked_version_poll_sec` | 60 | 撤销版本号轮询间隔 |
| `battle.ws.max_connections_per_pod` | 5000 | 单 Pod WS 连接上限 |
| `battle.ws.ping_interval_sec` | 30 | WS 心跳间隔 |
| `battle.score.correct` | 100 | 答对得分 |
| `battle.score.wrong` | -20 | 答错扣分 |
| `battle.score.speed_bonus_max` | 50 | 速度加成上限 |
| `battle.reconcile.interval_sec` | 600 | 对账周期 |
| `battle.reconcile.lock_ttl_sec` | 300 | 对账锁 TTL |
| `battle.reconcile.batch_size` | 500 | 对账批量 |
| `battle.reconcile.stale_threshold_sec` | 86400 | 未对账告警阈值 |
| `battle.reconcile.zset_retention_sec` | 86400 | `battle:finished` ZSET 保留时间 |

#### `qbank`

| Key | 默认值 | 说明 |
|---|---|---|
| `qbank.l1.capacity_mb` | 256 | L1 LRU 容量 |
| `qbank.l1.warmup_top_n` | 500 | 新 Pod 预热 Top N |
| `qbank.l2.question_ttl_sec` | 43200 | 单题缓存 TTL（12h） |
| `qbank.l2.chapter_ttl_sec` | 21600 | 章节缓存 TTL（6h） |
| `qbank.l2.meta_ttl_sec` | 86400 | 元数据缓存 TTL（24h） |
| `qbank.cos.singleflight_max` | 100 | singleflight 并发上限 |
| `qbank.cos.head_concurrency` | 20 | 批量 COS HEAD 并发上限 |
| `qbank.cos.timeout_ms` | 3000 | COS 回源超时 |
| `qbank.cos.stale_serve_max_sec` | 3600 | stale 数据最长服务时间 |
| `qbank.cos.compensation_interval_sec` | 300 | COS 与 PG 元数据补偿周期 |
| `qbank.config.refresh_interval_sec` | 30 | 配置轮询间隔 |
| `qbank.grpc.fiber_bridge_queue_size` | 10000 | gRPC → Fiber 桥接队列上限 |
| `qbank.grpc.max_concurrent_fibers` | 5000 | 最大并发 Fiber |
| `qbank.hpa.max_replicas` | 8 | 副本上限 |
| `qbank.alerts.l1_hit_ratio` | 0.80 | L1 命中率告警阈值 |
| `qbank.alerts.l2_hit_ratio` | 0.95 | L2 命中率告警阈值 |
| `qbank.flarerpc.ipc_p999_warn_ms` | 1 | FlareRPC IPC P999 告警 |
| `qbank.flarerpc.quic_p999_warn_ms` | 10 | FlareRPC QUIC P999 告警 |

#### `consumers`

| Key | 默认值 | 说明 |
|---|---|---|
| `consumers.max_retries` | 3 | 消息最大重试次数 |
| `consumers.retry_ttl_sec` | 3600 | 重试计数 TTL |
| `consumers.idempotency_ttl_sec` | 86400 | 幂等标记 TTL |
| `consumers.batch_size` | 100 | 单次 poll 最大消息数 |
| `consumers.commit_interval_ms` | 5000 | offset 提交间隔 |
| `consumers.offset_reset_policy` | `latest` | offset 重置策略 |
| `consumers.keda.lag_threshold` | 100 | KEDA 每分区积压阈值 |

#### `config`（配置管理自身）

| Key | 默认值 | 说明 |
|---|---|---|
| `config.write_retry_times` | 3 | Redis 快照写重试次数 |
| `config.reconcile_interval_sec` | 300 | PG↔Redis 对账周期 |
| `config.effective_slo_sec` | 30 | Rust 服务配置最坏生效时间 |
| `config.snapshot_age_warn_sec` | 300 | 快照陈旧告警阈值 |

#### `web`（前端配置，`/config?ns=web` 返回）

| Key | 默认值 | 说明 |
|---|---|---|
| `web.features.battle_enabled` | `true` | 对战功能开关 |
| `web.features.recommendation_enabled` | `true` | 推荐功能开关 |
| `web.features.sse_enabled` | `true` | SSE 推送开关 |
| `web.cdn.media_base_url` | `https://cdn.example.com` | 媒体 CDN 域名 |
| `web.ui.theme` | `default` | UI 主题 |
| `web.ui.page_size` | 20 | 列表分页大小 |
| `web.quiz.timer_warning_sec` | 10 | 倒计时警告阈值 |
| `web.maintenance.enabled` | `false` | 维护模式开关 |
| `web.maintenance.message` | `""` | 维护提示文案 |
| `web.config.poll_interval_sec` | 60 | 前端配置轮询间隔 |
| `web.leaderboard.top_cache_ttl_sec` | 60 | 前端排行榜缓存 TTL |
| `web.auth.pre_refresh_sec` | 120 | 前端主动预刷新提前量（可由 `auth.frontend.pre_refresh_sec` 覆盖） |
| `web.auth.silent_restore_timeout_sec` | 5 | 静默恢复超时 |

#### `alerts`（告警阈值）

| Key | 默认值 | 说明 |
|---|---|---|
| `alerts.auth_refresh_failure_ratio` | 0.05 | Refresh 失败率告警 |
| `alerts.frontend_refresh_failure_ratio` | 0.05 | 前端刷新失败率告警 |
| `alerts.login_failure_ratio` | 0.3 | 登录失败率告警 |
| `alerts.jwt_reject_ratio` | 0.05 | JWT 拒绝率告警 |
| `alerts.dlq_message_total` | 1 | DLQ 消息数告警阈值 |
| `alerts.config_effective_slo_multiplier` | 2 | 配置生效 SLO 告警倍数 |
| `alerts.profile_flush_lag_sec` | 300 | 画像 flush 滞后告警 |
| `alerts.outbox_pending_count` | 1000 | Outbox 积压告警 |
| `alerts.consumer_lag_warn` | 500 | 消费者 lag 告警 |
| `alerts.redis_evicted_keys_warn` | 1 | Redis 实例 1 淘汰 key 告警 |
| `alerts.battle_reconcile_pending_warn` | 1 | 对账待处理告警 |

#### `deploy`（部署参数，部分由 K8s HPA/KEDA 使用）

| Key | 默认值 | 说明 |
|---|---|---|
| `deploy.gateway.replicas_min` / `max` | 3 / 6 | gateway 副本范围 |
| `deploy.gateway.az_count` | 3 | gateway 跨 AZ 数 |
| `deploy.gateway.cronhpa.peak_replicas` | 6 | 考试周峰值副本 |
| `deploy.gateway.hpa.qps_target` | 3000 | gateway HPA QPS 目标 |
| `deploy.auth.hpa.qps_target` | 500 | auth HPA QPS 目标 |
| `deploy.user_api.hpa.qps_target` | 1000 | user-api HPA QPS 目标 |
| `deploy.recommendation.hpa.p99_target_ms` | 100 | recommendation HPA P99 目标 |
| `deploy.battle.keda.connections_per_pod` | 5000 | battle KEDA 每 Pod 连接目标 |
| `deploy.sse.keda.connections_per_pod` | 5000 | SSE KEDA 每 Pod 连接目标 |
| `deploy.termination_grace_period_sec` | 60 | Pod 优雅关闭宽限 |
| `deploy.drain_sec` | 30 | drain 时长 |

### 7.6 L4 前端配置

构建时只含部署确定的内容；运行时 `/config?ns=web` 返回 `web` namespace 全部配置。挂载前拉取存入 Pinia，每 `web.config.poll_interval_sec` 轮询。`/config` 只返回 `web` / `public` namespace，限流见 6.1。

### 7.7 配置项注册机制

**动态 schema**：`config.schemas` 表定义每个 namespace 的 key 类型、范围、默认值、描述。新增配置项通过 `adminRegisterConfigSchema` mutation 注册，**校验规则**立即可用，不发版。

**服务依赖声明**：各服务启动时通过代码中的静态清单声明依赖的 namespace/key。启动时检查 Redis 快照是否包含所有声明的 key，缺失的记录告警（`config_missing_key_total`）并用内置默认值兜底。新增配置项若需要消费端代码读取，**仍需发版**——动态 schema 只解决校验规则和存储不发版。

**校验**：`adminUpsertConfig` 写入时按 schemas 表校验类型和约束。

### 7.8 配置变更治理

审计（`config.config_audit`）、回滚（`adminRollbackConfig`）、灰度（`enabled` 字段临时禁用）、紧急开关（`web.features.*` 和 `gateway.*` 走快速通道，仅管理员）。

---

## 八、数据模型（PG）

分 schema 隔离：`auth` / `app` / `battle` / `question` / `outbox` / `config`。跨 schema 不建外键。

**auth schema**：

| 表 | 关键字段 |
|---|---|
| `users` | id (UUID PK)、email (UNIQUE)、password_hash、status、email_verified、roles (TEXT[])、时间戳 |
| `refresh_tokens` | id (UUID PK)、user_id、token_hash (sha256)、expires_at、rotated_at、revoked、时间戳；user_id + revoked 索引 |
| `login_audit` | id (BIGSERIAL PK)、user_id、email、ip、ua、success、event_type（login/refresh/revoke/refresh_failed）、grace_used、created_at |

**app schema**：

| 表 | 关键字段 |
|---|---|
| `user_profiles` | user_id (UUID PK)、nickname、avatar_url、时间戳（唯一写者：`profile-bootstrap-consumer`） |
| `quiz_sessions` | id (UUID PK)、user_id、mode、chapter_id、status、时间戳 |
| `attempts` | attempt_id (UUID PK)、user_id、question_id、session_id、chapter_id、is_correct、answer、submitted_at |
| `user_profile_vector` | user_id (UUID PK)、embedding (vector)，updated_at；HNSW 余弦索引（参数来源 `recommendation.pgvector.*`） |

**battle schema**：`battles`（battle_id PK）、`battle_participants`（battle_id + user_id PK）。

**question schema**：`question_versions`（含 `deleted_at` 软删）、`admin_audit`。

**outbox schema**：`user_api`（未发布部分索引）。

**config schema**：`configs`、`config_audit`、`schemas`。

**画像向量定义**：维度由 `recommendation.recommend.chapter_dims` 决定。答错按 `recommendation.recommend.delta.wrong` 累加，答对按 `recommendation.recommend.delta.correct` 递减。Flusher 时 L2 归一化后存。

**题目软删的历史展示**：attempts 引用已软删题目时，从 COS 取旧版本内容或返回「题目已删除」占位。

---

## 九、接口契约

### 9.1 GraphQL（`user-api`，`/graphql`）

**核心类型**：`User`、`Question`、`QuestionContent`、`Option`、`QuizSession`、`SubmitAttemptPayload`、`LeaderboardEntry`、`LeaderboardPage`。

**枚举**：`QuizMode`（PRACTICE / EXAM / CHAPTER）、`Period`（DAILY / WEEKLY / ALL）。

**用户 Query**：`me`、`quizSession(id)`、`leaderboard(period, page, size)`、`myRank(period)`。

**用户 Mutation**：`startQuizSession(mode, chapterId)`、`submitAttempt(input)`、`updateProfile(input)`。

**Subscription**：`leaderboardUpdated(period)`（可选，SSE 通道）。

**Admin 类型**：`ConfigEntry`、`ConfigMutationPayload`（含 `applied`、`degraded`、`entry`、`errorMessage`）、`ConfigSchemaEntry`、`ConfigAuditEntry`、`QuestionVersion`。

**Admin Query**：`adminConfigs(namespace)`、`adminConfigSchemas(namespace?)`、`adminConfigHistory(namespace, key, limit)`、`adminQuestionVersions(questionId)`。

**Admin Mutation**：`adminUpsertConfig`、`adminDeleteConfig`、`adminRegisterConfigSchema`、`adminRollbackConfig`（均返回 `ConfigMutationPayload` 或 `ConfigSchemaEntry`）、`adminUploadQuestion`、`adminUpdateQuestion`、`adminSoftDeleteQuestion`。

**Admin 权限**：`X-User-Roles` 含 `admin` 且 `X-Real-IP` 在 `user_api.admin.allowed_cidrs` 内。

### 9.2 GraphQL（`recommendation-service`，`/rec/graphql`）

**类型**：`ChapterStrength`、`ProfileSummary`。**Query**：`recommendQuestions(chapterId?, count)`、`userProfileSummary`。

### 9.3 gRPC（`question-bank`，对外）

包名 `qbank.v1`，服务 `QuestionBank`。方法：`GetQuestion`、`BatchGetQuestions`、`GetChapterQuestions`（流式）、`GetQuestionMeta`、`GetRandomQuestions`。

### 9.4 FlareRPC（`question-bank`，对内）

包名 `qbank.internal`。`CacheService`（Get / MGet）、`FetcherService`（Fetch 流式）。

### 9.5 REST（保留的必要端点）

| 服务 | 路径 | 说明 |
|---|---|---|
| `auth-service` | `/auth/register`、`/auth/login`、`/auth/refresh-token`、`/auth/logout`、`/.well-known/jwks.json` | OAuth2 风格；refresh 端点无 body，从 Cookie 取 refresh token |
| `user-api` | `/config?ns=web` | 前端启动快路径 |
| `user-api-sse` | `/events` | SSE 协议限制；`gateway` 从 query 提取 token |

**为何不 GraphQL 化**：认证是 OAuth2 标准端点、网关友好、刷新失败需要明确 HTTP 401；`/config` 是启动时最简单 key-value 拉取；SSE 是长连接协议。

---

## 十、Redis 键空间（分三个实例）

**实例 1：对战/会话（`noeviction` 淘汰策略）**

| Key | 类型 | 说明 |
|---|---|---|
| `session:{id}` | String | 会话 JSON，TTL `user_api.session.max_duration_sec` |
| `battle:room:{id}` 及其子键 | Hash/List | 房间元数据、玩家、题目、答案，TTL `battle.match.room_ttl_sec` |
| `battle:user:{uid}` | String | 当前房间 id |
| `battle:queue:{mode}` | List | 匹配队列 |
| `battle:finished` | ZSET | 结算索引，保留 `battle.reconcile.zset_retention_sec` |
| `battle:broadcast` | Pub/Sub | 跨 Pod 广播 |

**实例 2：排行榜/画像/限流/黑名单**

| Key | 类型 | 说明 |
|---|---|---|
| `leaderboard:daily:{date}` / `weekly:{week}` / `all` | ZSET | 排行榜 |
| `leaderboard:{period}:{date}:top100` | String | Top 100 快照，TTL `user_api.leaderboard.top_cache_ttl_sec` |
| `profile:delta:{user_id}` / `profile:dirty` / `profile:lock:{user_id}` | Hash/ZSET/String | 画像增量与 flusher，lock TTL `recommendation.flusher.lock_ttl_sec` |
| `revoked:{jti}` | String | token 黑名单，TTL = 剩余有效期 |
| `auth:revoked` | Pub/Sub | 撤销频道 |
| `auth:revoked_version` | String | 撤销版本号 |
| `auth:login_fail:{email}:{ip}` | String | 登录失败计数，TTL `auth.login_rate_limit.window_sec` |
| `auth:refresh_rate:{user_id}` | String | refresh 端点限流计数，TTL 60 秒 |
| `notification:broadcast` | Pub/Sub | 推送频道 |

**实例 3：QBank L2 / 配置 / 幂等标记**

| Key | 类型 | 说明 |
|---|---|---|
| `qbank:question:{id}` / `chapter:{id}:questions` / `meta:{id}` | String | 题库缓存，TTL `qbank.l2.*` |
| `qbank:hot:{chapter}` | ZSET | 热点统计 |
| `qbank:invalidate` | Pub/Sub | 失效频道 |
| `config:{namespace}` | String | 配置快照（含 `based_on_pg_max_updated_at`） |
| `config:changed:{namespace}` | Pub/Sub | 配置变更频道 |
| `attempt:redis:image:{attempt_id}` / `attempt:redis:rank:{attempt_id}` | String | 幂等标记，TTL `consumers.idempotency_ttl_sec` |
| `battle:processed:{battle_id}` | String | 幂等标记，TTL `consumers.idempotency_ttl_sec` |
| `retry:{topic}:{partition}:{offset}` | String | 重试计数，TTL `consumers.retry_ttl_sec` |

**拆分理由**：实例 1 用 `noeviction` 保护对战状态不被 QBank L2 挤占；实例 3 承载数据量最大、TTL 最长的 QBank L2，与对战状态隔离。三个实例均主从 + 哨兵，AOF everysec + RDB 每小时。

---

## 十一、Kafka Topic 拓扑

| Topic | 生产者 | 消费者 | 分区 | 幂等键 | DLQ |
|---|---|---|---|---|---|
| `attempts` | `user-api`（Outbox） | `attempt-consumer` | 12 | attempt_id | `attempts_dlq` |
| `battle_result` | `battle-service` | `battle-result-consumer` | 6 | battle_id | `battle_result_dlq` |
| `user_registered` | `auth-service`、`user-api`（补偿） | `profile-bootstrap-consumer` | 3 | user_id | `user_registered_dlq` |

**分区数规划**：分区数 = 预期最大消费者副本数 × `consumers.keda.partition_overprovision_factor`（默认 1.2），不能中途减少。初期按日活峰值 / 1000 估算。

---

## 十二、前端架构

**模块划分**：Pinia store（session / quiz / profile / battle / config / auth）、composables（`useQuizEngine`、`useWebSocket`、`useSSE`、`useRuntimeConfig`、`useAuthRefresh`）、api 层（Axios + GraphQL 封装 + 刷新拦截器）、components、views。

**GraphQL 端点调用编排**：前端 api 层对两个 GraphQL 端点（`/graphql`、`/rec/graphql`）并行调用。推荐页一次 `Promise.all` 拉齐用户信息、推荐、排行榜。**不引入 BFF 或 GraphQL 联邦**。

**鉴权与刷新**：见第五章。Axios 拦截器只对 HTTP 401 触发刷新；GraphQL 业务错误不触发；single-flight；多 Tab `BroadcastChannel` 协同；静默恢复；刷新失败降级。

**submitAttempt 一致性**：提交后依赖 mutation 返回值做乐观 UI，不立即查询 attempts。允许 `user_api.submit.consistency_window_sec` 的延迟；超时走补偿。

**运行时配置**：挂载前拉 `/config?ns=web` 存入 Pinia，每 `web.config.poll_interval_sec` 轮询。

**媒体**：`<img>` 直接指向 CDN 域名（`web.cdn.media_base_url`）。

**SSE**：EventSource 连 `/events`（query 带 token）。断线后重连并主动拉最新状态。收到服务端撤销事件时关闭连接并跳登录。

**WebSocket**：连 `/ws/battle?token=...`。收到 `token_refresh_required` 提示时调刷新端点获取新 token，通过 WS 帧回传。token 过期后提交答案被拒，`battle.token_expired_grace_sec` 内未刷新则连接被强制断开。

---

## 十三、可靠性设计

### 13.1 Outbox（仅 `user-api`）

`submitAttempt`：同一 PG 事务更新会话 + 写 outbox → 返回 accepted → Outbox Publisher 异步轮询发 Kafka → 更新 published_at。

**producer 幂等的实际价值**：Kafka 幂等 producer 只在单 session 内去重，producer 重启后 sequence 重置。Outbox 跨重启重发时 producer 幂等不保证不重复。**真正兜底是消费端 Lua 原子幂等**。producer 幂等的价值是减少短窗口重复、降低消费端压力和 DLQ 概率。

`auth-service` 的 `user_registered` 接受事件丢失（`profile-bootstrap-consumer` 兜底，`user-api` 懒创建发补偿事件）。`battle-service` 的 `battle_result` 丢失由对账任务补偿。

### 13.2 幂等设计

**attempt-consumer**：PG 用 `attempt_id` 主键 + `ON CONFLICT DO NOTHING`；画像和排行榜用 Lua 原子脚本（`if not EXISTS 标记 then 累加; SET 标记 end`），标记 TTL `consumers.idempotency_ttl_sec`。

**battle-result-consumer**：PG 用 `battle_id` 主键；排行榜用 Lua 原子脚本。

**关键**：所有累加操作与标记写入在同一 Lua 脚本中原子执行，不存在「先操作后写标记」的窗口。

### 13.3 DLQ

每个消费者手动提交 offset。重试 `consumers.max_retries` 次后 produce 到 DLQ + 提交 offset。DLQ 独立告警，人工排查后重放。

### 13.4 优雅关闭（统一模式）

SIGTERM → readiness 503 → drain `deploy.drain_sec` → 停止接受新请求 / 发 reconnect → 排空在途 → flush Kafka producer → 关连接池 → 退出。`terminationGracePeriodSeconds` = `deploy.termination_grace_period_sec`。

`question-bank` 特化：gRPC Server Shutdown → Flare 停止接受新 Fiber → 等待在途 Fiber → 关客户端 → 退出。

### 13.5 降级策略与 Redis 持久化

| 故障 | 行为 |
|---|---|
| `question-bank` COS 回源失败 | 返回 stale L1/L2 数据（最长 `qbank.cos.stale_serve_max_sec`） |
| Redis 实例 1（对战/会话）故障 | 对战中断，玩家重开；会话丢失，前端重新开始 |
| Redis 实例 2（排行榜/画像/限流/黑名单）故障 | 排行榜降级为空列表；画像增量丢失一批；限流降级为放行；**黑名单查询失败时 `gateway` 按 `gateway.blacklist.fail_open`（默认 true）放行 + 告警** |
| Redis 实例 3（QBank/配置/幂等）故障 | QBank 回源 COS；配置用本地快照；幂等标记丢失窗口依赖 PG 主键兜底 |
| PG 故障 | 读服务返回缓存或降级；写路径经 Outbox 缓冲；配置写入返回错误 |
| Kafka 故障 | Outbox 积压，`submitAttempt` 仍返回 accepted，恢复后重发 |
| `auth-service` 故障 | 已登录用户继续正常访问；token 刷新失败 → 用户在 access token 过期后降级到登录页；新登录不可用 |

**Redis 持久化**：三个实例均 AOF everysec + RDB 每小时。对战状态可接受丢失 ≤ 1 秒，丢失后玩家重开。排行榜、画像 delta 可接受丢失 ≤ 1 秒。幂等标记丢失后 PG 主键冲突兜底。

### 13.6 Battle 结算对账

结算时 ZADD `battle:finished`（独立于房间键）；Redis 锁选主执行对账（`battle.reconcile.lock_ttl_sec`）；每 `battle.reconcile.interval_sec` 按时间窗口批量取（`battle.reconcile.batch_size`）；PG 批量查询差集；重发成功后 `ZREM`；超过 `battle.reconcile.stale_threshold_sec` 未对账告警。

---

## 十四、安全设计

| 层 | 措施 |
|---|---|
| 传输 | 对外 HTTPS/WSS；服务间 gRPC 用 mTLS |
| 认证 | `gateway` 独占验签 + 黑名单，注入 `X-User-*` / `X-Real-IP` header |
| header 防伪造 | `gateway` 清除客户端传入的 `X-User-*` / `X-Real-IP` / `X-Forwarded-For` / `X-Request-Id` / `X-Trace-Id` |
| Token 存储 | Access 存内存，Refresh 存 httpOnly + Secure + SameSite=Strict Cookie；均不落 localStorage |
| 无感续签 | 见第五章：single-flight 刷新、静默恢复、多 Tab 协同、WS 新 token 帧验签 + 查黑名单、SSE 连接期间不续签、刷新失败降级 |
| WS 特例 | `gateway` 从 query 提取 token 验签，日志脱敏（`gateway.log.mask_query_token`）；`battle-service` 连接期间新 token 帧验签 + 查黑名单；订阅 `auth:revoked` + 每 `battle.revoked_version_poll_sec` 轮询版本号兜底 |
| SSE 撤销 | `user-api-sse` 订阅 `auth:revoked`，收到后断开该用户所有本地连接 |
| 撤销 | Redis 黑名单 + pub/sub（实例 2）；撤销时清理该用户所有 refresh token |
| 密码 | argon2id（参数 `auth.argon2.*`），登录响应时间恒定化 |
| 限流 | `gateway` 单 Pod 限流（`gateway.rate_limit.*`）+ `auth-service` 独立限流（`auth.login_rate_limit.*` / `auth.refresh_rate_limit_per_min`） |
| 密钥 | KMS + ESO；COS 用 TKE 服务角色 |
| 网络 | `auth-service` NetworkPolicy 只允许 `gateway` 和内部 gRPC；`question-bank` 只允许内部 gRPC；所有对外服务只允许 `gateway` |
| 媒体 | COS 私有桶 + CDN 回源鉴权；敏感资源走短有效期预签名 URL |
| 审计 | 登录审计（含 refresh 事件）、管理操作审计、配置变更审计 |
| Admin 权限 | `X-User-Roles` 含 `admin` 且 `X-Real-IP` 在 `user_api.admin.allowed_cidrs` 内 |
| 依赖 | cargo audit / cargo deny、trivy、Dependabot |

**GraphQL 鉴权失败语义**：`gateway` 验签失败返回 HTTP 401；业务层鉴权失败返回 GraphQL error（HTTP 200 + errors）。

**JWT 时钟偏移**：`gateway` 验签允许 `auth.jwt.leeway_sec` 容忍；前端 `exp` 基于服务端返回值计算。

---

## 十五、可观测性

### 15.1 三支柱

| 维度 | 方案 | 关键 |
|---|---|---|
| Metrics | Prometheus + Grafana | 每服务 `/metrics` |
| Traces | OTel → Tempo | Rust `tracing-opentelemetry`，C++ OTel C++ SDK |
| Logs | Loki | 结构化 JSON，含 trace_id / user_id / service；`gateway` 日志脱敏 WS/SSE token |

**trace 传播**：HTTP/gRPC 注入 `traceparent`；FlareRPC 同机 IPC 不注入，跨机 QUIC 注入；gRPC span → Fiber span 由桥接层显式创建父子关系。

### 15.2 必配指标与告警

告警阈值来源统一为第七章 `alerts` namespace，或各服务 namespace 内的 `*.alerts.*` key。

| 指标 | 服务 | 告警阈值 key |
|---|---|---|
| `qbank_l1_hit_ratio` / `qbank_l2_hit_ratio` | `question-bank` | `qbank.alerts.l1_hit_ratio` / `qbank.alerts.l2_hit_ratio` |
| `qbank_cos_fallback_total` / `qbank_cos_head_batch_size` | `question-bank` | 突增（基线告警） |
| `qbank_flarerpc_latency` | `question-bank` | `qbank.flarerpc.ipc_p999_warn_ms` / `qbank.flarerpc.quic_p999_warn_ms` |
| `qbank_grpc_fiber_bridge_queue_depth` | `question-bank` | 持续 > 0 |
| `profile_dirty_size` / `profile_flush_lag_seconds` / `profile_flush_failed_total` | `recommendation-service` | `alerts.profile_flush_lag_sec` / 非零 |
| `{topic}_consumer_lag` | Consumers | `alerts.consumer_lag_warn` |
| `dlq_message_total` | Consumers | `alerts.dlq_message_total` |
| `battle_active_rooms` / `battle_reconcile_pending_total` | `battle-service` | 突降 / `alerts.battle_reconcile_pending_warn` |
| `battle_token_blacklist_check_total` / `battle_token_expired_rejected_total` | `battle-service` | 突增 |
| `ws_connection_count` / `sse_connection_count` | `battle-service` / `user-api-sse` | `battle.ws.max_connections_per_pod` / `user_api_sse.max_connections_per_pod` |
| `auth_login_failure_ratio` | `auth-service` | `alerts.login_failure_ratio` |
| `auth_refresh_total{result}` / `auth_refresh_rate_limited_total` / `auth_refresh_rotation_conflict_total` / `auth_refresh_grace_used_total` | `auth-service` | `alerts.auth_refresh_failure_ratio` / 非零 / 非零 / 持续非零 |
| `frontend_token_refresh_success_total` / `frontend_token_refresh_failed_total` / `frontend_silent_restore_total{result}` | 前端上报 | `alerts.frontend_refresh_failure_ratio` |
| `gateway_jwt_reject_ratio` / `gateway_rate_limit_rejected{path}` | `gateway` | `alerts.jwt_reject_ratio` / 突增 |
| `gateway_ws_token_extracted_total` / `gateway_log_token_masked_total` | `gateway` | 正常计数，脱敏失败告警 |
| `outbox_pending_count` / `outbox_publish_lag_seconds` | `user-api` | `alerts.outbox_pending_count` |
| `config_read_fallback_total` / `config_write_failed_total` | 所有服务 / `user-api` | 降级计数 / 失败非零 |
| `config_snapshot_age_seconds` / `config_effective_latency_seconds` | 所有服务 | `config.snapshot_age_warn_sec` / `alerts.config_effective_slo_multiplier` |
| `config_missing_key_total` | 所有服务 | 非零 |
| `sse_revoked_disconnect_total` | `user-api-sse` | 正常计数 |
| `redis_instance_evicted_keys{instance}` | 三个 Redis 实例 | `alerts.redis_evicted_keys_warn`（实例 1） |

---

## 十六、部署与运维

### 16.1 CI 流水线

lint → unit test → build → integration test → image build → trivy scan → push → staging → e2e → 审批 → prod（金丝雀）。

### 16.2 K8s 部署

副本范围、HPA/KEDA 目标、连接上限等参数均来自第七章 `deploy` namespace 或各服务 namespace。

| 服务 | 副本 | HPA/KEDA 信号 | Ingress 超时 | 备注 |
|---|---|---|---|---|
| `gateway` | `deploy.gateway.replicas_min`–`max`（跨 AZ） | QPS + 连接数 | 按路径 | 单 Pod WS+SSE 连接上限 `gateway.pod.max_ws_sse_connections`；`/healthz` / `/readyz`；CronHPA 按 `deploy.gateway.cronhpa.peak_replicas` |
| `auth-service` | `deploy.auth.replicas_min`–`max` | 请求速率 | 默认 | NetworkPolicy 收紧 |
| `user-api` | `deploy.user_api.replicas_min`–`max` | 请求速率 + PG 排队 | 默认 | 不含 SSE |
| `user-api-sse` | `deploy.sse.replicas_min`–`max` | SSE 连接数 | `gateway.ws_sse.read_timeout_sec` | 独立 Deployment |
| `recommendation-service` | `deploy.recommendation.replicas_min`–`max` | 请求速率 + 向量查询 P99 | 默认 | PG 读副本 |
| `battle-service` | `deploy.battle.replicas_min`–`max` | WS 连接数 | `gateway.ws_sse.read_timeout_sec` | 独立 Deployment；对账任务 Redis 锁选主 |
| `question-bank` | `deploy.qbank.replicas_min`–`max`（上限 `qbank.hpa.max_replicas`） | Redis 延迟 + L1 命中率 | —（内网 gRPC） | 新 Pod 预热 |
| `attempt-consumer` | `deploy.attempt_consumer.replicas_min`–`max`（≤ topic 分区数） | KEDA lag（`consumers.keda.lag_threshold`） | — | 独立 Deployment |
| `battle-result-consumer` | `deploy.battle_result_consumer.replicas_min`–`max` | KEDA lag | — | 独立 Deployment |
| `profile-bootstrap-consumer` | `deploy.profile_bootstrap_consumer.replicas_min`–`max` | KEDA lag | — | 独立 Deployment |

**有状态组件**：PG 用腾讯云 DBaaS（含读副本）；Redis 三个独立实例（主从 + 哨兵，AOF everysec + RDB 每小时）；Kafka 用 CKafka。

**节点层**：TKE 超级节点承载弹性，包年包月承载基线；CronHPA 处理峰谷，KEDA/HPA 处理突发。所有 Rust/C++ 服务不看 CPU。

### 16.3 配置与密钥

JWT 私钥经 KMS + ESO 注入。DB/Redis/Kafka 凭证经 ESO 从 KMS 拉取。COS 用 TKE 服务角色。非敏感配置用 ConfigMap。

---

## 十七、测试策略

| 层 | 范围 | 工具 | 关键用例 |
|---|---|---|---|
| 单元 | 纯逻辑 | cargo test / gtest / vitest | 状态机、归一化、评分、Lua 脚本、刷新调度 |
| 集成 | 单服务 + 真实依赖 | testcontainers | Outbox 发布、Lua 原子幂等、DLQ、配置热更新与对账、refresh 轮换宽限期 |
| 契约 | GraphQL / gRPC / proto | graphql-inspector / buf breaking | schema 变更不破坏调用方 |
| E2E | 全链路 | Playwright | 注册→答题→提交→排行榜→推荐→长时间挂机后无感续签→登出 |
| 性能 | `question-bank` / `battle-service` / `gateway` | wrk / 自研 | 题库 P99、对战并发房间、gateway 峰值 QPS + 连接数 |
| 故障注入 | 韧性 | chaos-mesh | Redis 实例单独中断、PG/Kafka 中断、Pod 被杀、gateway 单 AZ 故障、auth-service 故障 |

**特有测试**：

- **配置驱动验证**：篡改关键配置（如 `battle.match.per_question_sec`、`qbank.l1.capacity_mb`、`auth.refresh_grace_sec`）后，服务行为在 SLO 内按新值生效，无重启。
- **无感续签全场景**：access token 过期后自动刷新；刷新页面静默恢复；多 Tab 并发刷新不冲突；WS 连接期间 token 刷新；token 过期未刷新被拒操作；撤销后 SSE/WS 断开；刷新失败降级到登录页；`auth-service` 故障时已登录用户继续访问。
- **幂等原子性**：消息重复投递 10 次，画像和排行榜只累加一次。
- **信任边界**：绕过 `gateway` 直连 Pod 被 NetworkPolicy 拒绝；伪造 `X-Real-IP` 被 `gateway` 清除；Admin 从非内网 IP 访问被拒。
- **WS token 全生命周期**：握手验签 → 刷新帧验签 + 查黑名单 → 撤销 pub/sub 断开 → pub/sub 丢失时版本号轮询兜底。
- **gRPC → Fiber 桥接**：高并发下桥接队列不爆。
- **FlareRPC**：IPC 和 QUIC 两种模式 P99 基准。
- **优雅关闭**：SIGTERM 后在途请求完成或超时可控，无悬挂 Fiber。
- **trace 跨协议**：gRPC span → Fiber span 父子关系正确。
- **配置**：写入 → 快照更新 → 各服务在 SLO 内生效；Redis 快照写失败 → 对账任务回填；pub/sub 丢失 → 轮询兜底；服务声明 key 缺失 → 告警。
- **Battle 对账**：结算消息丢失 → 锁选主 → 批量查询 → 重发 → PG 有记录；多副本不重复执行。
- **Redis 隔离**：QBank L2 大量写入不导致实例 1 淘汰对战键。
- **Gateway 容量**：峰值 QPS + 连接数下 P99 不劣化；单 AZ 故障后恢复。

---

## 十八、落地里程碑

| 里程碑 | 内容 | 并行 | 验收标准 |
|---|---|---|---|
| **M0 基础设施** | PG（分 schema + 读副本）、三个 Redis 实例、CKafka、ESO、OTel、CI 骨架、配置表 + schema 注册 + Redis 快照骨架、**全部默认配置项注册** | — | 三支柱可观测、CI 绿、配置读写 + schema 注册闭环、Redis 隔离验证、**代码中无字面量阈值（静态扫描通过）** |
| **M1 认证闭环** | `auth-service`（refresh 轮换 + 宽限期 + 独立限流）+ `gateway` Lua（验签/黑名单/限流/header 注入含 `X-Real-IP`）+ NetworkPolicy + WS token 提取与日志脱敏 + **前端无感续签** | 与 M2 并行 | 登录拿 token；无感续签全场景通过；刷新页面静默恢复；多 Tab 不冲突；刷新失败跳登录；绕过 gateway 被拒；伪造 header 被清；Admin 非内网 IP 被拒 |
| **M2 核心答题闭环** | `user-api`（GraphQL + Outbox）+ `user-api-sse` + `attempt-consumer`（Lua 原子幂等）+ `question-bank`（gRPC 单进程 + 抽象层 + L1/L2 + 批量元数据回源） | 与 M1 并行 | 提交进 Kafka、重复投递只累加一次、SSE 撤销可断开、题库命中可观测、抽象层就位、**篡改配置行为按新值生效** |
| **M3 推荐** | `recommendation-service`（读时合并 + flusher + pgvector + PG 读副本） | 依赖 M2 | 推荐 P99 < `recommendation.slo.read_p99_ms`、flusher 指标正常 |
| **M4 对战** | `battle-service`（WS + Redis 状态机 + Pub/Sub + 新 token 帧验签 + 查黑名单 + 撤销订阅 + 版本号轮询兜底 + 锁选主对账 + ZSET 索引 + 连接期间无感续签）+ `battle-result-consumer` | 依赖 M2 的 QBank | 完整对战、Pod 重启可重连、结算丢失可对账、多副本不重复对账、token 过期前收到刷新提示、过期未刷新被拒操作 |
| **M5 推送与题库管理** | SSE 业务推送内容 + Admin GraphQL（题库 + 配置，`ConfigMutationPayload` 返回）+ 软删 + 缓存失效 + 服务启动声明 key 清单 | 可后置 | 实时推送可达、上传后秒级失效、软删题目历史可展示、配置部分成功可感知 |
| **M6 QBank 拆进程** | Cache Proxy / COS Fetcher 独立进程，抽象层实现替换为 FlareRPC client | 按需 | FlareRPC P999 达标、trace 不断、无需重写业务 |
| **M7 精细化** | HPA/KEDA 调优、TKE 成本优化、故障注入、配置治理完善、无感续签全场景压测 | 收尾 | 成本与容量曲线符合预期、配置 SLO 达标、gateway 单 AZ 故障恢复、长时间挂机无感续签成功率达标 |

**关键依赖链**：M0 → (M1 ∥ M2) → M3 → M4 → M5 → M6（按需）→ M7。

---

## 十九、架构边界清单

以下十六条是这套架构的「不变式」：

1. **`gateway` 独占验签和黑名单查询**，注入身份 header，清除客户端传入的同名 header（必须含 IP 相关）；NetworkPolicy 禁止绕过 `gateway` 直连 Pod。
2. **`auth-service` 只签发与撤销**，不是每请求的同步依赖；`gateway` 本地验签保证已登录用户在 `auth-service` 故障时继续访问。
3. **无感续签是认证体系的体验基础**：Access Token 存内存、Refresh Token 存 httpOnly Cookie、single-flight 刷新、静默恢复、多 Tab 协同、WS 新 token 帧验签 + 查黑名单、SSE 连接期间不续签、刷新失败统一降级到登录页。
4. **`user-api` 不做异步重计算**——画像 flusher、attempts 落库、排行榜写全在 Consumers / `recommendation-service`。
5. **`recommendation-service` 独占 `user_profile_vector` 的写权**；`profile-bootstrap-consumer` 是 `user_profiles` 的唯一写者。
6. **`user-api-sse` / Consumers 按连接数和 lag 独立扩缩**，不与 GraphQL 或彼此共用 Deployment。
7. **`question-bank` 对外标准 gRPC、对内 FlareRPC**；抽象层初期就位，FlareRPC 由「模块独立部署」驱动。
8. **`battle-service` 状态权威在 Redis**；房间键与对账索引 `battle:finished` ZSET 生命周期分离。
9. **WS 新 token 帧验签必须查黑名单**，pub/sub 丢失由版本号轮询兜底；**SSE 订阅 `auth:revoked`** 断开撤销用户。
10. **Kafka 只承载异步事件**，画像 flush 走 Redis 不走 Kafka。
11. **所有累加操作的幂等由 Lua 原子脚本保证**——标记检查、累加、写标记在同一脚本内，不存在「先操作后写标记」的窗口。
12. **媒体直连 CDN**，不经 `gateway` 或后端服务。
13. **配置四层管理**，schema 动态注册（校验规则不发版，消费端代码仍需发版）；快照携带 `based_on_pg_max_updated_at`，对账比较此字段；PG 不可用时写入拒绝。
14. **`question-bank` 副本上限 + 新 Pod 预热**；批量元数据优先 MGET，未命中批量 COS HEAD；admin 写 COS metadata 先于 PG。
15. **Redis 分三个实例**：对战/会话（`noeviction`）、排行榜/画像/限流/黑名单、QBank L2/配置/幂等标记；AOF everysec，对战状态可接受丢失 ≤ 1 秒。
16. **所有可调参数外置到 config**：阈值、超时、TTL、批量大小、并发上限、重试次数、比例、窗口、限流速率、评分系数、SLO 目标、告警阈值、副本范围、连接上限均来自配置读取；代码中不出现字面量数值，CI 静态扫描强制。