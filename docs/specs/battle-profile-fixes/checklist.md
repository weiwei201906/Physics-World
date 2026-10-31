# Checklist: 修复排行榜昵称、答题记录刷新、刷新 415 与对战链路

- [x] `POST /auth/refresh-token` 无 body 时返回 200 并下发新 access token（无 415）
- [x] 注册带昵称的用户出现在排行榜时昵称为真实值，而非 `player`
- [x] 答题后 `recommendation` 的 `userProfileSummary.totalAttempts` 立即增加
- [x] `/ws/battle?token=<jwt>` 无 token 握手返回 401，带有效 token 握手返回 101
- [x] 两个客户端经网关可完整走通 `match_found` → `room_state` → `question` → `answer_result` → `game_over`
- [x] 房间玩家昵称来自 `auth.users`/`app.user_profiles`（示例输出了 `Lin` / `Chen`）
- [x] `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- [x] `scripts/smoke_backend.sh` 通过（含 gateway 静态资源、鉴权与 `battle upstream unavailable` 502）
- [x] 临时验证账号（`fixcheck-*`、`battle-*`、`smoke-*`）及排行榜缓存已清理
