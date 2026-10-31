# Spec: 可运行站点交付、文档纠偏与微服务并行迭代

## 1. 目标

交付一个浏览器可直接访问的 Physics-World 站点，并在此之前完成文档审计与 Git 结构整理。
本规格覆盖三类工作：

1. 文档纠偏：删除/修正与仓库现状不符的描述。
2. Git 结构：补全 `.gitignore`、清理垃圾文件、按逻辑单元用 Conventional Commits 提交到 `develop`。
3. 微服务并行：每个后端微服务一个子智能体，各自在独立 worktree/分支内补测试与实现；主智能体负责 gateway 配置、集成验证、合并与推送。

验收口径：

1. `bash scripts/dev_stack.sh up` 后，`http://127.0.0.1:8080/` 返回前端页面，`/healthz` 返回 `{"status":"ok","service":"gateway"}`。
2. 通过 gateway 完成注册 → 登录 → Bearer 访问 `/graphql` 的 `me` 查询 → `/config` 查询 → `/rec/graphql` 推荐查询 → `/events` 返回 `text/event-stream` 头。
3. `scripts/smoke_backend.sh` 全绿并打印 `smoke test passed`。
4. `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全绿。
5. 文档中不再出现不存在的目录（`tests/e2e/`、`backend/gateway/`、`backend/config/` 空目录）、不存在的脚本参数或错误文件名（`0001_init.sql`）。
6. `git status` 干净，`git log --oneline` 每个提交单一主题，提交信息纯 ASCII，`origin/develop` 与本地同步。

## 2. 现状与约束（已实测）

- 工作目录 `/home/weiwei/Physics-World`，分支 `develop`，远端 `weiwei201906/Physics-World`。
- 本机无 root、无 Docker Hub 网络；Docker daemon 可用但无法拉镜像，`docker compose up --build` 不可作为本机验证手段。
- 本机已安装 PG18/Redis 客户端；服务端二进制须 `apt-get download` 后 `dpkg-deb -x` 解包。
- Trae 注入的 `LD_LIBRARY_PATH`/`LD_PRELOAD` 会破坏 `cargo`、`apt-get`、`protoc`、`postgres`、`redis`，脚本必须显式剔除。
- PostgreSQL 18 服务端与 Redis 8 已在本机 15432/16379 运行；`DATABASE_URL=postgres://physics_world:physics_world@127.0.0.1:15432/physics_world`，`REDIS_URL=redis://127.0.0.1:16379/0`。
- nginx 1.28.3 已解包到 `/tmp/pwroot/usr/sbin/nginx`；前端 `frontend/dist` 已存在。
- Kafka 未在本机运行，三个 consumer 不纳入站点运行路径；其子智能体只做单测与代码完善，集成验证时跳过。

## 3. 文档纠偏（精确改动）

### 3.1 `README.md`

- 目录树删除 `tests/e2e/`（不存在）；`backend/gateway/`、`backend/config/` 两行删除（空目录且实际网关位于 `deploy/gateway/`）。
- 在 `deploy/` 行后补充：`deploy/gateway/default.conf.template` 为网关配置模板，`deploy/Dockerfile.gateway` 为网关镜像。
- “本地验证”改为实际可执行命令：
  - 前端：`npm --prefix frontend run type-check && npm --prefix frontend run lint:check && npm --prefix frontend test`
  - 后端：`cd backend && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  - 题库：`cd backend/question-bank && ./tools/build.sh`（若 Flare/Blade 未就绪，明确写 `UNVERIFIED` 并给出原因，禁止写不可执行的 `blade test //...`）。
- 网关联调改为：`bash scripts/dev_stack.sh up`，浏览器访问 `http://127.0.0.1:8080`；删除 `NGINX_BIN=/usr/sbin/nginx`（本机无此路径）。
- 保留 compose 说明，但标注“需要可访问容器镜像仓库的环境；本机无法拉镜像时使用 `dev_stack.sh`”。

### 3.2 `docs/specs/database-integration/spec.md` 与 `tasks.md`

- `0001_init.sql` → `init.sql`（全文替换，含 tasks.md）。
- 补充 `consumers.sql`（battle schema）。
- `PgPoolOptions::max_connections(5)` → 与实际代码一致的 `10`（auth）/ 配置值（user-api）。
- 将已完成的条目在 tasks.md 勾选，并注明 `dev_db.sh` 需处理 redis-server 符号链接与 liburing 依赖。

### 3.3 `docs/architecture.md`

- 仅做事实校对：端口、服务名、契约路径与 `backend/Cargo.toml`、`contracts/` 实际内容一致；发现“已实现”但实际为 TODO 的条目（如 battle JWT 校验、题库 gRPC 接线）改为“计划中/未实现”。
- 不新增无依据的架构承诺。

## 4. Git 结构

### 4.1 根 `.gitignore`（新增或补全）

忽略：`backend/target/`、`frontend/node_modules/`、`frontend/dist/`（构建产物由 CI/部署生成）、`.worktrees/`、`*.log`、`.env`、`.env.*`、`/tmp` 相关本地数据库路径、`backend/question-bank/blade_out/`、`backend/question-bank/.blade/`。

### 4.2 提交序列（ASCII，Conventional Commits，按逻辑单元）

1. `docs(repo): correct directory layout and local verification commands`
2. `chore(repo): add gitignore for build artifacts and worktrees`
3. `feat(auth): camelCase auth contract with exp and refresh-token route`
4. `feat(user-api): persist domain data and expose config snapshot`
5. `feat(recommendation): add graphql recommendation service`
6. `feat(battle): add websocket battle service`
7. `feat(consumer): add attempt, battle-result, and profile-bootstrap consumers`
8. `feat(gateway): serve spa and proxy auth, graphql, sse, rec, ws`
9. `build(deploy): add compose stack and service dockerfiles`
10. `test(smoke): cover gateway end-to-end flow`
11. `feat(qbank): scaffold flare/ blade question bank service`（完成度不足时用 `chore(qbank):` 并注明状态）
12. `docs(spec): record site delivery plan and task status`

每个提交前执行对应 scope 的测试；不允许 `--no-verify`。

## 5. 微服务子智能体（并行）

每个子智能体只允许修改自己的 crate 目录，不得改 `contracts/`、`deploy/`、`scripts/`、根配置。各自在 `.worktrees/<name>` 的独立分支 `feat/<scope>-hardening` 上工作，完成后由主智能体 review + `git merge --no-ff`。

| 分支 | 目录 | 目标 |
|---|---|---|
| `feat/auth-hardening` | `backend/auth-service` | 修复契约后回归测试；覆盖注册/登录/刷新/登出/verify/jwks |
| `feat/user-api-hardening` | `backend/user-api` | GraphQL 与 REST 对齐前端；补会话/尝试/配置测试 |
| `feat/sse-hardening` | `backend/user-api-sse` | 事件广播、心跳、断线重连测试 |
| `feat/recommendation-hardening` | `backend/recommendation-service` | 推荐算法边界、GraphQL schema 测试 |
| `feat/battle-hardening` | `backend/battle-service` | 匹配、房间状态机、WebSocket 协议测试 |
| `feat/consumer-hardening` | 三个 consumer | 处理器幂等、DLQ、无 Kafka 时单测隔离 |
| `feat/qbank-hardening` | `backend/question-bank` | Flare 服务骨架、存储/缓存、proto 对齐、可离线构建 |

共同要求：`cargo fmt`、`cargo clippy -p <crate> --all-targets -- -D warnings`、`cargo test -p <crate>` 全绿；只提交 ASCII 信息；发现跨 crate 缺陷只报告不修改。

## 6. 主智能体：gateway 与集成

### 6.1 网关配置（`deploy/gateway/default.conf.template`）

- 静态资源：`/` 返回 `index.html`，未命中路径回退 SPA。
- `/healthz` 由网关本地返回。
- `/auth/`、`/.well-known/jwks.json` 公开转发 auth-service。
- `/graphql`、`/graphiql`、`/config` 经 `auth_request /_auth` 注入 `X-User-Id/X-User-Roles/X-User-Email`。
- `/events` 转发 user-api-sse，关闭缓冲、`proxy_read_timeout` 提升。
- `/rec/` 转发 recommendation-service（GraphQL）。
- `/ws/battle` 升级 WebSocket；`/ws/` 其余路径 503。
- 安全响应头：`X-Content-Type-Options`、`X-Frame-Options`、`Referrer-Policy`、`Permissions-Policy`。

### 6.2 `scripts/dev_stack.sh`（新增）

- `up|down|status` 子命令；`up` 依次：`dev_db.sh start`（剔除污染环境变量）→ 构建后端 → 启动 5 个服务（写 pid/日志到 `/tmp/pwdev`）→ 渲染 nginx 模板（`envsubst`）→ 启动 nginx 8080 → `npm --prefix frontend run build`（若 `dist` 缺失）。
- 端口/进程已存在时幂等复用；`down` 按 pid 停止。
- 所有子进程环境显式 `env -u LD_LIBRARY_PATH -u LD_PRELOAD LC_ALL=C LANG=C`。

### 6.3 `scripts/dev_db.sh` 修复

- `redis-server` 解包后是悬空符号链接：改为 `cp /usr/bin/redis-check-rdb` 或解包 `redis-server` 的真实二进制。
- 解包后设置 `LD_LIBRARY_PATH=$PW_DB_ROOT/usr/lib/x86_64-linux-gnu`（liburing2）。
- 迁移目录按文件名排序执行，兼容 `init.sql`/`consumers.sql`。

## 7. 验证步骤（按顺序）

1. `bash scripts/dev_db.sh status` → PG/Redis 运行中。
2. `cd backend && env -u LD_LIBRARY_PATH cargo fmt --all -- --check && env -u LD_LIBRARY_PATH cargo clippy --workspace --all-targets -- -D warnings && env -u LD_LIBRARY_PATH cargo test --workspace`。
3. `bash scripts/dev_stack.sh up` → 5 个服务 + nginx + 前端构建成功。
4. `curl -s http://127.0.0.1:8080/healthz` → `{"status":"ok","service":"gateway"}`。
5. `curl -s http://127.0.0.1:8080/ | head` → 返回 `index.html` 内容。
6. 注册/登录：
   - `POST /auth/register` 返回 201，body 含 `accessToken`、`user.email`；
   - `POST /auth/login` 返回 `tokenType=Bearer`、`expiresIn=900`、`exp`；
   - `POST /auth/refresh-token` 轮换成功。
7. `curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/graphql -d '{"query":"{ me { id email nickname } }"}'` → 返回当前用户。
8. `curl http://127.0.0.1:8080/rec/graphql -d '{"query":"..."}'` → 返回推荐结果。
9. `curl -I http://127.0.0.1:8080/events` → `content-type: text/event-stream`。
10. `bash scripts/smoke_backend.sh` → `smoke test passed`。
11. 浏览器访问 `http://127.0.0.1:8080`：注册/登录页可用，登录后进入主界面；截图取证。
12. `git status --short` 为空；`git log --oneline -12` 与提交序列一致；`git push origin develop` 成功。

## 8. 风险与回退

- 本机无法拉 Docker 镜像 → 站点验证以 `dev_stack.sh` 为准，compose 仅做 `docker compose config` 静态校验。
- Flare/Blade 依赖网络与 LFS → 题库若无法完成，保留可编译骨架并标记未完成，不阻塞站点。
- Kafka 缺席 → consumer 不启动，其测试全部使用内存/桩实现。
- 子智能体并行改 crate 可能与主智能体集成冲突 → 所有共享文件（contracts、scripts、deploy）只由主智能体修改。
