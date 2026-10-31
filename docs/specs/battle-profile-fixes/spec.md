# Spec: 修复排行榜昵称、答题记录刷新、刷新 415 与对战链路

## 1. 背景与现象

用户报告四个问题（本地栈 `scripts/dev_stack.sh`，网关 8080）：

1. 排行榜所有条目昵称恒为 `player`。
2. 答题后玩家的统计记录（总答题数/正确率/画像）不刷新。
3. 浏览器出现 `415 (Unsupported Media Type)`。
4. 对战功能不可用（无法匹配，进入房间后无题目）。

已定位根因（均有代码证据）：

| # | 根因 | 位置 |
|---|---|---|
| 1 | 排行榜昵称只查 `app.user_profiles`；该表仅在 `profile()` 被调用时懒创建，且昵称写死 `'player'`；注册只写 `auth.users` | `backend/user-api/src/pg_store.rs` `profile_names()` / `profile()` / `leaderboard()` |
| 2 | `recommendation-service` 用 `InMemoryProfileSource`，进程内计数，重启即零，也与 PG `app.attempts` 无关 | `backend/recommendation-service/src/main.rs` |
| 3 | `refresh` handler 强制 `Json<RefreshRequest>`，前端刷新令牌时无 body（token 在 httpOnly cookie），axum 返回 415 | `backend/auth-service/src/routes.rs` `refresh()` |
| 4a | 网关 `/ws/battle` 不做鉴权、不注入 `X-User-Id`，battle-service 用 `Authenticated` 提取器强制要求该头 | `deploy/gateway/default.conf.template` / `backend/battle-service/src/ws.rs` |
| 4b | 前端帧协议（`find_match`/`cancel_match`/`join_room`/`answer`/`token_update` → `match_found`/`room_state`/`question`/`answer_result`/`game_over`）与后端旧协议（`enqueue`/`answer`/`leave`/`token` → `matched`/`question`/`score`/`finished`）不一致 | `backend/battle-service/src/protocol.rs`（已重写）与 `ws.rs`（未完成） |
| 4c | 引擎题目只有 id + 正确答案，无题干/选项/昵称，房间视图无法渲染 | `backend/battle-service/src/{engine,store,main}.rs` |
| 4d | 前端在 socket `CONNECTING` 时立即 `send()`，`join_room`/`find_match` 被静默丢弃 | `frontend/src/views/battle/{BattleLobbyView,BattleRoomView}.vue` |

## 2. 目标与验收

1. `POST /auth/refresh-token`（无 body，仅 cookie）返回 200 与新的 access token；带 body 仍支持。
2. 排行榜条目显示注册昵称（`auth.users.nickname`），profile 行缺失时不再回退 `player`。
3. `recommendation` GraphQL 的 `profileSummary.totalAttempts/correctRate` 在答题后立即反映 `app.attempts` 的聚合值。
4. 两个已登录客户端可通过 `/ws/battle` 匹配、进入房间、收到题干/选项、答题后收到 `answer_result`、全部答完后收到 `game_over`。
5. `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全绿；前端 `npm run build` 不回归。

## 3. 代码变更

### 3.1 auth-service 刷新 415

文件 `backend/auth-service/src/routes.rs`，函数 `refresh`：

- 参数 `Json(request): Json<RefreshRequest>` → `body: Option<Json<RefreshRequest>>`。
- `raw` 计算改为 `body.and_then(|Json(request)| request.refresh_token).or_else(|| cookie_token(...))`。
- 与 `logout` 保持一致（它已使用 `Option<Json<RefreshRequest>>`）。

### 3.2 排行榜昵称

文件 `backend/user-api/src/pg_store.rs`，函数 `profile_names`：

```sql
SELECT u.id, u.email,
       COALESCE(NULLIF(p.nickname, ''), u.nickname) AS nickname,
       COALESCE(p.avatar_url, u.avatar_url)          AS avatar_url,
       COALESCE(p.roles, u.roles)                    AS roles,
       u.created_at
FROM auth.users u
LEFT JOIN app.user_profiles p ON p.user_id = u.id
WHERE u.id = ANY($1)
```

- 返回值仍是 `HashMap<Uuid, UserView>`，`UserRow` 类型不变。
- `leaderboard()` 中的 `unwrap_or_else(|| "player".to_string())` 保留为最后兜底（正常注册用户不会被命中）。
- `profile()` 的懒创建逻辑不改（保留 `player` 仅用于占位），昵称来源以 `auth.users` 为准。

### 3.3 答题记录刷新（recommendation-service）

文件 `backend/recommendation-service/Cargo.toml`：新增 `sqlx.workspace = true`。

文件 `backend/recommendation-service/src/store.rs`：新增 `PgProfileSource { pool: PgPool, dims: Vec<String> }`。

- `connect(database_url, dims) -> Result<Self>`：`PgPoolOptions::new().max_connections(5)`。
- `load_baseline(user)`：`SELECT dims, weights, updated_at FROM app.user_vectors WHERE user_id=$1`；
  PG 维度是 `chapter_id:difficulty`，按 `:` 前段聚合到 `ChapterVector`（配置维度未命中则丢弃）；
  无行返回 `ChapterVector::zero(dims)`。
- `load_increments(user)` / `drain_increments(user)`：返回零向量（本服务只读消费）。
- `attempt_stats(user)`：
  - `SELECT count(*) AS total, count(*) FILTER (WHERE is_correct) AS correct FROM app.attempts WHERE user_id=$1`
  - `SELECT q.chapter_id, count(*) FROM app.attempts a JOIN question.questions q ON q.id = a.question_id WHERE a.user_id=$1 GROUP BY q.chapter_id`
  - 组装 `AttemptStats { total_attempts, correct_attempts, chapter_attempts }`。
- `updated_at(user)`：取自 `app.user_vectors.updated_at`。
- 查询失败时 `tracing::warn!` 并返回零值/默认值，保持服务可用。

文件 `backend/recommendation-service/src/main.rs`：
- `DATABASE_URL` 非空 → `PgProfileSource::connect(&url, dims).await`，失败则 warn 并回退 `InMemoryProfileSource`；否则内存实现。
- 仅替换 `source`，`engine`/`router` 不变。

### 3.4 对战：网关鉴权与身份注入

文件 `deploy/gateway/default.conf.template`：

新增 `location = /_ws_auth`（`internal`）：

```nginx
location = /_ws_auth {
    internal;
    if ($arg_token = "") { return 401; }
    proxy_pass http://${AUTH_UPSTREAM}/auth/verify;
    proxy_method GET;
    proxy_pass_request_body off;
    proxy_set_header Content-Length "";
    proxy_set_header Authorization "Bearer $arg_token";
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

`location = /ws/battle` 增加：

```nginx
auth_request /_ws_auth;
auth_request_set $identity_user_id $upstream_http_x_user_id;
proxy_set_header X-User-Id $identity_user_id;
```

保留现有 `Upgrade`/`Connection`/长超时设置；不转发客户端自带的 `X-User-Id`（`proxy_set_header` 覆盖）。

### 3.5 对战：协议与引擎（battle-service）

已完成部分：`protocol.rs` 已重写为前端契约；`engine.rs` 增加昵称/题干/选项、`join`、断线语义；`store.rs` 事件转换已改写。剩余：

1. `src/vector.rs`
   - 新增 `PlayerProfile { vector: PlayerVector, nickname: Option<String> }`。
   - `VectorSource::vector` → `profile(&self, user) -> PlayerProfile`；`ZeroVectorSource` 返回 `{default, None}`。
   - `PgVectorSource::profile`：
     - `SELECT dims, weights FROM app.user_vectors WHERE user_id=$1`；
     - `SELECT COALESCE(NULLIF(p.nickname,''), u.nickname) FROM auth.users u LEFT JOIN app.user_profiles p ON p.user_id=u.id WHERE u.id=$1`；
     - 两者失败分别降级为零向量 / `None`。
2. `src/store.rs`
   - `room_state_message`、`question_message`、`game_over_message` 改为 `pub(crate)`。
   - `EngineEvent::Score` 增补 `correct_option_id: String`、`explanation: String`；`AnswerResult` 填真实值（不再 `None`）。
3. `src/engine.rs`
   - `answer` 中把当前题的 `correct_option`/`explanation` 带入 `EngineEvent::Score`。
4. `src/ws.rs`，`handle_client_message` 重写为：

| 客户端帧 | 服务端动作 |
|---|---|
| `find_match {mode}` | `state.vectors.profile(user)`；`store.enqueue_with_profile(user, nickname, vector)`；保存返回的 `room_id`/`receiver` |
| `cancel_match` | `store.leave(user)`；清空 `room_id`/`receiver` |
| `join_room {roomId}` | `store.subscribe(room)` → 保存 receiver；`store.join(user, room)` → 发送 `room_state`，有当前题再发 `question` |
| `answer {questionIndex, optionId}` | 必须有 socket 当前 `room_id` 且 `token_exp` 未过期；`store.answer(user, room_id, index, option_id)` |
| `token_update {token}` | `tokens.validate` 且 `claims.user_id == user` 时更新 `token_exp`，否则 `invalid_token` |

- 保持 tick/广播/断线 `leave` 循环；`queue_timeout` 时清空 `room_id`/`receiver`。
5. `src/main.rs`
   - 新增 `load_catalog(database_url, limit) -> Option<Vec<QuestionSpec>>`：
     `SELECT id, stem, options, answer_option_id, COALESCE(explanation,'') FROM question.questions WHERE deleted_at IS NULL ORDER BY random() LIMIT $1`；
     `options` 为 `[{id,label,text}]`，映射为 `BattleOption { id, text }`（`text` 缺失回退 `label`）。
   - `DATABASE_URL` 非空时优先 PG；空结果或连接失败时回退现有 `demo_questions()`（改用 `QuestionSpec::demo`）。

### 3.6 对战：前端发送时机

- `frontend/src/views/battle/BattleLobbyView.vue` `findMatch()`：若 `connected.value` 为真，立即 `send(findMatchFrame(mode.value))`；否则置 `matching=true` 并 `connect()`（沿用 `watch(connected)`）。
- `frontend/src/views/battle/BattleRoomView.vue`：
  - `onMounted` 中先注册 `onMessage`，再 `connect()`；
  - 新增 `watch(connected, c => { if (c) send(joinRoomFrame(props.roomId)) })`；移除 `connect()` 后立刻 `send(join_room)` 的写法。

## 4. 协议契约（前后端一致）

客户端 → 服务端：

```json
{"type":"find_match","mode":"1v1"}
{"type":"cancel_match"}
{"type":"join_room","roomId":"<uuid>"}
{"type":"answer","questionIndex":0,"optionId":"a"}
{"type":"token_update","token":"<access>"}
```

服务端 → 客户端：

```json
{"type":"match_found","roomId":"<uuid>"}
{"type":"room_state","roomId":"<uuid>","status":"playing","players":[{"userId":"<uuid>","nickname":"...","score":0,"connected":true}],"currentIndex":0,"totalQuestions":5}
{"type":"question","index":0,"total":5,"seconds":10,"question":{"id":"...","stem":"...","options":[{"id":"a","text":"..."}]}}
{"type":"answer_result","index":0,"correct":true,"correctOptionId":"a","score":150,"totalScore":150,"explanation":"..."}
{"type":"game_over","winnerId":"<uuid>|null","players":[...],"finalScore":300}
{"type":"error","code":"...","message":"..."}
```

## 5. 验证步骤

1. `env -u LD_LIBRARY_PATH -u LD_PRELOAD cargo fmt --all`
2. `... cargo clippy --workspace --all-targets -- -D warnings`
3. `... cargo test --workspace`
4. `cd frontend && npm run build`
5. 重建并重启本地栈：`scripts/dev_stack.sh down && PW_SKIP_BUILD=1 scripts/dev_stack.sh up`
6. curl 验证：
   - `POST /auth/refresh-token` 仅带 cookie → 200（不再 415）。
   - 注册（带昵称）→ 正确答题 → `GET /graphql` 排行榜条目昵称为注册昵称。
   - `recommendation` `profileSummary` 的 `totalAttempts` 在答题后 +1。
7. 对战探针：两个账号各开 `/ws/battle?token=...`，A `find_match` → B `find_match` → 双方收 `match_found`；双方 `join_room` → 收 `room_state` + `question`；`answer` → 收 `answer_result`；答满 → 收 `game_over`。
8. 清理验证用临时账号/数据。

## 6. 不做的事

- 不引入 JWKS 直连校验（battle-service 仍信任网关注入的 `X-User-Id`）。
- 不改排行榜 Redis 计分规则（仍为答对 +100）。
- 不启动 `attempt-consumer`（recommendation 直接读 PG 聚合）。
- 不新增文档以外的运行时配置项。
- 不提交/推送（除非用户明确要求）。

## 7. 风险与回退

- nginx `auth_request` 子请求继承父请求参数，`$arg_token` 可用；若目标 nginx 版本不继承，`/_ws_auth` 会返回 401。验证步骤 7 会直接暴露该问题。
- 断线与重连竞态：引擎在 `Playing` 阶段仅标记 `connected=false`，双方都断开才结算，因此大厅→房间切页不会误判结束。
- PG 不可用时：battle catalog 回退 demo 题；向量/昵称回退零值/`None`；recommendation 回退内存实现。功能降级但不崩溃。
