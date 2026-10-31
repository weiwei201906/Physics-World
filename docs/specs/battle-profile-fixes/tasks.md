# Tasks: 修复排行榜昵称、答题记录刷新、刷新 415 与对战链路

> 依赖顺序执行；每个任务完成后勾选并更新状态。验证命令统一加前缀
> `env -u LD_LIBRARY_PATH -u LD_PRELOAD`。

## T1 auth-service 刷新 415 [P]
- [x] `backend/auth-service/src/routes.rs` `refresh()` 改为 `Option<Json<RefreshRequest>>`，保留 cookie 回退。
- [x] 验证：`cargo test -p auth-service`；本地 `POST /auth/refresh-token` 仅带 cookie 返回 200。

## T2 排行榜昵称 [P]
- [x] `backend/user-api/src/pg_store.rs` `profile_names()` 改为 join `auth.users` + `app.user_profiles` 的 `COALESCE` 查询。
- [x] 验证：`cargo test -p user-api`；注册带昵称后 `leaderboard` 条目昵称正确。

## T3 recommendation 答题统计 [P]
- [x] `backend/recommendation-service/Cargo.toml` 增加 `sqlx.workspace = true`。
- [x] `backend/recommendation-service/src/store.rs` 新增 `PgProfileSource`（baseline 读 `app.user_vectors`，stats 聚合 `app.attempts`）。
- [x] `backend/recommendation-service/src/main.rs` 按 `DATABASE_URL` 选择 PG 源，失败回退内存。
- [x] 验证：`cargo test -p recommendation-service`；答题后 `profileSummary.totalAttempts` 增加。

## T4 WebSocket 鉴权
- [x] `backend/battle-service/src/validator.rs` 新增 `HttpTokenValidator`：用 `?token=` 调 auth-service `/auth/verify`，按 `x-user-id` / `x-user-exp` 生成 `TokenClaims`。
- [x] `src/ws.rs` 新增 `WsIdentity` 提取器（query `token` 或 Bearer 头），握手前校验，失败返回 401。
- [x] `backend/auth-service/src/routes.rs` `verify()` 返回 `x-user-exp` 响应头。
- [x] `deploy/gateway/default.conf.template` 移除 `/_ws_auth` 与 `/ws/battle` 的 `auth_request`（nginx 子请求拿不到 query 参数）。
- [x] `deploy/docker-compose.yml` 为 battle-service 增加 `AUTH_BASE_URL`。
- [x] 验证：无 token 握手 401；带有效 token 握手 101。

## T5 battle-service 协议收尾
- [x] `src/vector.rs` `VectorSource` 改为 `profile()` 返回 `PlayerProfile { vector, nickname }`（PG join `auth.users`/`app.user_profiles`）。
- [x] `src/store.rs` `room_state_message`/`question_message`/`game_over_message` 改 `pub(crate)`；`AnswerResult` 填 `correct_option_id`/`explanation`。
- [x] `src/engine.rs` `EngineEvent::Score` 携带 `correct_option_id`/`explanation`。
- [x] `src/ws.rs` `handle_client_message` 重写为 `find_match`/`cancel_match`/`join_room`/`answer`/`token_update`。
- [x] `src/main.rs` 用 PG `question.questions` 加载真实题目，失败回退 demo。
- [x] 验证：`cargo test -p battle-service`。

## T6 前端发送时机
- [x] `frontend/src/views/battle/BattleLobbyView.vue` 已连接时立即发送 `find_match`。
- [x] `frontend/src/views/battle/BattleRoomView.vue` 在 `connected` 变 true 时发送 `join_room`。
- [x] 验证：`cd frontend && npm run build`。

## T7 全量质量门
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## T8 本地联调验证
- [x] `scripts/dev_stack.sh down && PW_SKIP_BUILD=1 scripts/dev_stack.sh up`
- [x] 刷新令牌 200、排行榜昵称、recommendation 统计刷新。
- [x] 双客户端 `/ws/battle` 完整走通：`match_found` → `room_state` → `question` → `answer_result` → `game_over`。
- [x] 清理临时账号与验证数据。
