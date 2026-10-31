# Tasks: PostgreSQL/Redis 联调

> 依赖顺序执行；每步完成即更新状态。标注 [P] 的可与相邻任务并行。

## T1 本机数据库脚本 [P]
- [x] `scripts/dev_db.sh`：`start|stop|status|reset`，自动下载/解包 PG18 + redis-tools + liburing2，initdb/启动/角色/库/迁移。
- [x] 验证：`bash scripts/dev_db.sh start`；`psql` 连 15432；`redis-cli -p 16379 ping`。
- [x] 修复：解包后的 `redis-server` 是悬空符号链接（复制系统 `redis-check-rdb`）；PG 需要 `liburing2` 库路径；迁移按文件名排序。

## T2 迁移 SQL [P]
- [x] `backend/migrations/init.sql`：auth/app/question/config schema、表、索引、默认 config/schema 种子。
- [x] `backend/migrations/consumers.sql`：battle schema 与对局持久化表。
- [x] 验证：`psql "$DATABASE_URL" -f backend/migrations/init.sql` 重跑幂等（`IF NOT EXISTS`/`ON CONFLICT`）。

## T3 workspace 依赖
- [x] `backend/Cargo.toml` 增加 sqlx、redis、async-trait；两个服务 Cargo.toml 引用。
- [x] 验证：`cargo check --workspace`（无需数据库）。

## T4 common 配置
- [x] `common/settings.rs` 增加 `DatabaseSettings::from_env()`（DATABASE_URL、REDIS_URL 及命名空间回退）。
- [x] 验证：`cargo test -p common`。

## T5 auth-service 持久化
- [x] `store.rs` 抽出 `trait AuthStore`，原实现更名 `MemoryAuthStore`。
- [x] 新增 `pg_store.rs`：users/refresh_tokens SQL、轮换与 family 撤销、Redis family 吊销键。
- [x] `routes.rs` await 化；`main.rs` 连库 + `sqlx::migrate!` + 构造 PgAuthStore。
- [x] `tests/auth_flow.rs` 改用 MemoryAuthStore，并覆盖 camelCase 契约。
- [x] 验证：`cargo test -p auth-service`。

## T6 user-api 持久化
- [x] `store.rs` 抽出 `trait DomainStore`，原实现更名 `MemoryStore`。
- [x] 新增 `pg_store.rs`：profiles、sessions、attempts、question 内容与版本、config+audit、Redis 排行榜与配置快照。
- [x] 启动种子：章节与 24 题（空表时写入）。
- [x] `state.rs`/`graphql.rs`/`routes.rs` await 化；`main.rs` 连库迁移 + PgStore。
- [x] `tests/api.rs` 改用 MemoryStore。
- [x] 验证：`cargo test -p user-api`。

## T7 冒烟与持久化断言
- [ ] `scripts/smoke_backend.sh` 接入 dev_db、导出连接串、清表、重启服务后查询会话与登录。
- [ ] 验证：`bash scripts/smoke_backend.sh` 全绿并打印 `smoke test passed`。

## T8 部署与文档
- [x] `deploy/docker-compose.yml` 增加 postgres/redis、healthcheck、volume、连接串。
- [x] `README.md` 增加数据库启动/冒烟/compose 说明。
- [ ] 验证：`docker compose config`（可用时）；`frontend npm run build` 不回归。

## T9 质量门与提交
- [ ] `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`。
- [ ] 提交 3 个 ASCII 提交并 `git push origin develop`。
- [ ] 验证：`git status` 干净、`git log` 可见、远端同步。
