# Physics-World

面向学校的物理答题游戏，覆盖单人练习、章节刷题、实时对战、个性化推荐与排行榜。

## 功能

- **练习与刷题**：按章节选择题目，限时作答，即时判定并展示解析。
- **实时对战**：匹配房间、同步答题、按正确率与速度结算。
- **个性化推荐**：根据答题记录调整章节权重，推荐下一组练习。
- **排行榜**：日榜、周榜与总榜。
- **账户体系**：注册、登录、无感续签与角色权限。
- **管理后台**：题目与配置维护。

## 技术栈

- 前端：Vue 3、TypeScript、Vite、Pinia、Vitest
- 后端：Rust、Axum、async-graphql、SQLx、Tokio
- 存储：PostgreSQL 18、Redis 8、Kafka
- 网关：Nginx（路径分流、身份注入、SSE 与 WebSocket 代理）
- 接口：GraphQL 为主，认证、配置与 SSE 使用 REST

## 架构

浏览器只访问网关；网关完成 JWT 验签后注入身份 header，下游服务信任该 header。

| 服务 | 职责 |
|---|---|
| `gateway` | 入口、静态资源、路径分流、身份注入 |
| `auth-service` | 注册、登录、令牌签发与撤销、JWKS |
| `user-api` | 用户、答题会话、提交、排行榜、管理接口（GraphQL） |
| `user-api-sse` | `/events` 长连接推送 |
| `recommendation-service` | 推荐读取与用户画像 |
| `battle-service` | 匹配、对战状态机、实时广播（WebSocket） |
| `question-bank` | 题库导入与读取（gRPC）：RustFS 大 JSON → PostgreSQL，Redis 读缓存 |
| `attempt-consumer` | 答题提交落库、画像与排行榜更新 |
| `battle-result-consumer` | 对战结果落库 |
| `profile-bootstrap-consumer` | 注册后的画像初始化 |

题库以 RustFS 上的大 JSON 为源：`question-bank` 启动时通过 SigV4 拉取题库文件，反序列化后 upsert 到 PostgreSQL（`question.chapters/questions/question_versions`），题目读取走 Redis 读缓存（`qbank:question:{id}`，默认 TTL 300 秒），导入完成后失效缓存。本地没有 RustFS 时回退到 `deploy/qbank/questions.json`。契约见 [contracts/proto/qbank.proto](contracts/proto/qbank.proto)，客户端为 `backend/qbank-client`，完整设计见 [docs/architecture.md](docs/architecture.md)。

## 目录结构

```text
Physics-World/
├── frontend/                 # Vue 3 前端
├── backend/
│   ├── common/               # 共享配置、错误、遥测、身份
│   ├── auth-service/
│   ├── user-api/
│   ├── user-api-sse/
│   ├── recommendation-service/
│   ├── battle-service/
│   ├── attempt-consumer/
│   ├── battle-result-consumer/
│   ├── profile-bootstrap-consumer/
│   ├── qbank-client/         # 题库 gRPC 客户端契约
│   ├── question-bank/        # 题库服务：RustFS JSON 导入、PG 存储、Redis 缓存
│   └── migrations/           # PostgreSQL 建表脚本
├── contracts/
│   ├── graphql/              # user-api 与 recommendation SDL
│   └── proto/                # qbank.v1 gRPC 契约
├── deploy/                   # Dockerfile、compose 与网关配置
└── docs/                     # 架构与规格文档
```

## 快速开始

一条命令拉起数据库、后端服务、网关与前端，然后访问 `http://127.0.0.1:8080`：

```bash
bash scripts/dev_stack.sh up
bash scripts/dev_stack.sh status
bash scripts/dev_stack.sh down
```

需要容器环境时使用 compose：

```bash
docker compose -f deploy/docker-compose.yml up -d --build
```

## 许可证

本项目采用 [GNU General Public License v3.0](LICENSE)（SPDX 标识：`GPL-3.0-only`）。任何分发或修改后的版本都必须以同一许可证开源。
