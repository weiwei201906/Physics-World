# Spec: 用真实 PostgreSQL/Redis 跑通前后端联调

## 1. 目标

让 `auth-service` 与 `user-api` 不再使用进程内内存存储，改为连接真实 PostgreSQL 18 与 Redis 8，
并在本机（无 root、无 Docker socket）可重复启动数据库、执行迁移、跑通全链路冒烟测试。
完成后提交并推送到 `develop`。

验收口径：

1. `scripts/dev_db.sh start` 在本机启动 PG（127.0.0.1:15432）与 Redis（127.0.0.1:16379），可 `stop`/`status`/`reset`。
2. 迁移落库到 PG：`auth.users`、`auth.refresh_tokens`、`app.user_profiles`、`app.quiz_sessions`、
   `app.attempts`、`question.chapters`、`question.questions`、`question.question_versions`、
   `config.configs`、`config.config_audit`、`config.schemas`。
3. 注册/登录/刷新/登出数据持久化在 PG；重启 auth-service 后原账号仍可登录。
4. 答题会话与尝试写入 PG；重启 user-api 后 `quizSession(id)` 仍能查询到该会话。
5. 排行榜分数写入 Redis ZSET（`leaderboard:daily:{date}`、`leaderboard:weekly:{iso-week}`、`leaderboard:all`）。
6. 配置写入 PG（configs + 审计），并写 Redis 快照 `config:{namespace}`；`GET /config` 返回快照。
7. `scripts/smoke_backend.sh` 在真实 PG/Redis 上全绿，并新增持久化断言（重启服务后数据仍在）。
8. `cargo test --workspace`、`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings` 全绿。
9. `docker-compose.yml` 增加 postgres/redis 服务（healthcheck + volume），后端通过 `DATABASE_URL`/`REDIS_URL` 连接。
10. 代码注释与提交信息 ASCII；不提交任何密钥。

## 2. 本机数据库运行方式（已实测可行）

- 系统：Ubuntu 26.04，非 root，无 Docker socket 权限。
- 通过 `apt-get download` 拉取 `postgresql-18`、`redis-tools`、`liburing2`，`dpkg-deb -x` 解包到 `$PW_DB_ROOT`（默认 `/tmp/pwdb`）。
- `initdb` 必须使用 `LC_ALL=C LANG=C`，并且剔除 Trae 注入的 `LD_LIBRARY_PATH`/`LD_PRELOAD`。
- PG：`-p 15432 -k $PW_DB_ROOT -c listen_addresses=127.0.0.1`，初始超级用户 `postgres`，`--auth=trust`。
- Redis：`--port 16379 --bind 127.0.0.1 --daemonize yes`，同样需要 `LC_ALL=C`。
- 角色/库：`physics_world` / 密码 `physics_world` / 数据库 `physics_world`（仅本地开发）。

## 3. 代码变更

### 3.1 依赖（`backend/Cargo.toml` workspace）

新增 workspace 依赖：

- `sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json", "migrate"] }`
- `redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }`
- `async-trait = "0.1"`

各 crate 通过 `workspace = true` 引用。仅使用运行时 `sqlx::query`/`query_as`，不使用 `query!` 宏，避免构建期需要数据库。

### 3.2 迁移文件 `backend/migrations/init.sql`

创建 schema 与表（遵循 `docs/architecture.md` 第八章）：

- `auth.users(id uuid pk, email text unique, password_hash text, nickname text, avatar_url text, roles text[], status text default 'active', email_verified bool default false, created_at timestamptz, updated_at timestamptz)`
- `auth.refresh_tokens(id uuid pk, user_id uuid, token_hash text unique, family_id uuid, expires_at timestamptz, rotated_at timestamptz, revoked bool, created_at timestamptz)`；索引 `(user_id, revoked)`、`(family_id)`。
- `app.user_profiles(user_id uuid pk, email text, nickname text, avatar_url text, roles text[], created_at timestamptz, updated_at timestamptz)`
- `app.quiz_sessions(id uuid pk, user_id uuid, mode text, chapter_id text, status text, current_index int, total_questions int, question_ids jsonb, started_at timestamptz, completed_at timestamptz)`；索引 `(user_id, started_at desc)`。
- `app.attempts(attempt_id uuid pk, user_id uuid, session_id uuid, question_id text, is_correct bool, answer text, submitted_at timestamptz)`；唯一 `(session_id, question_id)`；索引 `(user_id, submitted_at desc)`。
- `question.chapters(id text pk, name text, position int)`
- `question.questions(id text pk, chapter_id text, difficulty int, stem text, image_url text, explanation text, options jsonb, answer_option_id text, deleted_at timestamptz, created_at timestamptz, updated_at timestamptz)`
- `question.question_versions(id uuid pk, question_id text, version int, content jsonb, correct_option_id text, deleted_at timestamptz, created_at timestamptz)`；唯一 `(question_id, version)`。
- `config.configs(namespace text, key text, value text, updated_at timestamptz, primary key(namespace,key))`
- `config.config_audit(id bigserial pk, namespace text, key text, old_value text, new_value text, changed_by uuid, changed_at timestamptz)`
- `config.schemas(namespace text, key text, type_name text, required bool, default_value text, description text, primary key(namespace,key))`

种子：`config.configs`（5 条现有默认项）、`config.schemas`（3 条现有默认 schema）。
章节与题目按现有 `chapter_seed()`/`option_seed()` 在服务启动时若表为空则写入（`INSERT ... ON CONFLICT DO NOTHING`）。
另见 `backend/migrations/consumers.sql`（`battle` schema，供 battle-result-consumer 使用）。

### 3.3 `backend/common`

- `settings.rs`：新增 `DatabaseSettings`（`database_url`、`redis_url`），从 env 读取 `DATABASE_URL`（回退 `AUTH_DATABASE_URL`/`USER_API_DATABASE_URL`）与 `REDIS_URL`；无值时保持空串，由 `main` 显式报错。
- `config.rs` 不动。

### 3.4 `auth-service`

- `store.rs`：抽出 `#[async_trait] pub trait AuthStore: Send + Sync`，方法同现有语义：
  `create_user`、`find_user_by_email`、`find_user`、`insert_refresh`、`rotate_refresh`、`revoke_family_of`。
  现有结构体改名为 `MemoryAuthStore`，保持同步逻辑与单测，trait 实现直接 await 包装。
- 新增 `pg_store.rs`：`PgAuthStore { pool: PgPool, redis: ConnectionManager }`，SQL 实现上述语义；
  `rotate_refresh` 用单条 `UPDATE ... WHERE token_hash=$1 AND revoked=false AND expires_at>now` + 查询 family/user；
  reuse 检测在事务内撤销整个 family；`revoke_family_of` 写 Redis `auth:revoked:family:{id}`（TTL 30 天）并更新 PG。
- `routes.rs`：`AppState.store: Arc<dyn AuthStore>`；所有调用加 `.await`。
- `main.rs`：读取 `DATABASE_URL`/`REDIS_URL`，`PgPoolOptions::new().max_connections(10)` 连接并 `sqlx::migrate!("../migrations").run(&pool)`；构造 `PgAuthStore`。
- 测试 `tests/auth_flow.rs` 改用 `Arc::new(MemoryAuthStore::new())`。

### 3.5 `user-api`

- `store.rs`：抽出 `#[async_trait] pub trait DomainStore: Send + Sync`，覆盖现有全部方法
  （`chapters`、`question`、`correct_answer`、`profile`、`update_profile`、`start_session`、`session`、
  `question_in_session`、`submit_attempt`、`leaderboard`、`my_rank`、`configs`、`config_schemas`、
  `upsert_config`、`delete_config`、`rollback_config`、`config_history`、`register_schema`、
  `upload_question`、`update_question`、`soft_delete_question`、`question_versions`、`config_snapshot`）。
  现有结构体保留为 `MemoryStore`（同步逻辑不变，trait 异步包装），供单测/集成测试使用。
- 新增 `pg_store.rs`：`PgStore { pool, redis: ConnectionManager }`：
  - profile：`INSERT ... ON CONFLICT DO NOTHING` 懒创建 + `RETURNING`；
  - session：`question_ids` 以 jsonb 保存；`submit_attempt` 在事务内插入 attempt（唯一键幂等），
    统计已答数并更新 `current_index`/`completed_at`；
  - leaderboard：Redis ZSET 写入/读取，昵称从 PG 批量查询；日/周 key 用 UTC 日期与 ISO 周；
  - config：事务写 configs + audit；成功后写 Redis `config:{namespace}` 快照（JSON：`based_on_pg_max_updated_at`、`written_at`、`values`）；
  - question：数据库实现章节/题目读取、上传/更新/软删/版本记录。
  - 启动时 `seed_question_bank_if_empty()` 写入现有 8 章 24 题。
- `state.rs`：`AppState.store: Arc<dyn DomainStore>`。
- `graphql.rs` / `routes.rs`：所有 store 调用 `.await`；`config_snapshot` 改为 await。
- `main.rs`：连接 PG、执行迁移、构造 `PgStore`；连接失败直接返回错误。
- 测试 `tests/api.rs` 使用 `Arc::new(MemoryStore::seeded())`。

### 3.6 脚本与部署

- 新增 `scripts/dev_db.sh`：
  - 子命令 `start|stop|status|reset`；`PW_DB_ROOT`（默认 `/tmp/pwdb`）、`PW_PG_PORT`（15432）、`PW_REDIS_PORT`（16379）；
  - 首次运行自动 `apt-get download` + `dpkg-deb -x`（允许 `PW_SKIP_DOWNLOAD=1`）；
  - `start` 幂等：initdb（如无数据目录）、启动/复用 PG、启动/复用 Redis、创建角色与库、应用迁移（psql `-f`），打印 `DATABASE_URL`/`REDIS_URL`；
  - `stop` 用 `pg_ctl stop -m fast` 与 `redis-cli shutdown nosave`；
  - `reset` 删数据目录后重新 init。
  - 所有命令剔除 `LD_LIBRARY_PATH`/`LD_PRELOAD`，设置 `LC_ALL=C LANG=C`。
- `scripts/smoke_backend.sh`：调用 `dev_db.sh start`（除非 `SMOKE_SKIP_DB=1`），导出 `DATABASE_URL`/`REDIS_URL`，
  对库先清空业务表保证可重复；新增断言：
  - 冒烟后 `psql` 校验 `auth.users` ≥1、`auth.refresh_tokens` ≥1、`app.quiz_sessions` ≥1、`app.attempts` ≥1、`config.configs` ≥5；
  - 重启 user-api 后用同一 token 查询 `quizSession(id)` 成功（会话持久化）。
- `deploy/docker-compose.yml`：新增 `postgres`（`postgres:18-alpine`，POSTGRES_USER/PASSWORD/DB，volume，healthcheck `pg_isready`）与 `redis`（`redis:8-alpine`，healthcheck `redis-cli ping`）；auth-service/user-api 注入 `DATABASE_URL`、`REDIS_URL` 并 `depends_on: condition: service_healthy`。
- `README.md`：补充本机数据库启动、冒烟命令与 compose 用法；不写环境特定路径。

### 3.7 安全与规范

- 不提交 `.env`、密码以外的凭证；compose 中开发密码仅用于本地。
- 迁移与脚本注释 ASCII。
- 提交拆分：`feat(db): add postgres/redis runtime and migrations`、`refactor(backend): persist auth and user data`、`test(smoke): verify persistence`。

## 4. 验证步骤（按顺序执行）

1. `bash scripts/dev_db.sh status` → PG/Redis 均在运行。
2. `psql "$DATABASE_URL" -c "select schema_name from information_schema.schemata"` → 含 auth/app/config/question。
3. `cd backend && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`。
4. `NGINX_BIN=/tmp/nginxroot/usr/sbin/nginx KEEP_LOGS=1 bash scripts/smoke_backend.sh` → 输出 `smoke test passed`。
5. `psql` 断言各表行数（见 3.6）。
6. 重启 auth-service（脚本内）→ 原账号登录成功；重启 user-api → 会话查询成功。
7. `docker compose -f deploy/docker-compose.yml config`（若 docker 可用）校验配置。
8. `git push origin develop`，CI 无需数据库凭据（单测走内存实现）。

## 5. 风险与回退

- PgStore 与 MemoryStore 双实现可能行为漂移 → 以 `tests/api.rs` 内存实现 + 冒烟 PG 双覆盖为准。
- 本机无 Docker socket → compose 仅静态校验，实际启动以 `dev_db.sh` 为准。
- 端到端测试耗时增加 → `SMOKE_SKIP_DB=1` 可退回内存模式做快速回归。
- 若 sqlx/redis 版本与工具链冲突 → 先 `cargo check` 验证，锁定兼容版本再继续。
