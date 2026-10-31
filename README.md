# Physics-World

面向学校的物理答题游戏：单人练习、章节刷题、实时对战、个性化推荐与排行榜。

技术栈固定为 Vue 3 + TypeScript（前端）与 Rust（业务服务），部署目标为 TKE。题库服务尚未实现，当前仅保留 `contracts/proto` 契约与 Rust 客户端 `qbank-client`。完整架构说明见 [docs/architecture.md](docs/architecture.md)。

## 目录结构

```text
Physics-World/
├── frontend/                 # Vue 3 + Vite + Pinia + Vitest
├── backend/
│   ├── Cargo.toml            # Rust workspace（按微服务划分，不按语言分目录）
│   ├── common/               # 共享 config/error/telemetry/identity
│   ├── auth-service/         # Rust + Axum
│   ├── user-api/             # Rust + async-graphql
│   ├── user-api-sse/         # Rust + SSE
│   ├── recommendation-service/
│   ├── battle-service/       # Rust + WebSocket
│   ├── attempt-consumer/     # Kafka consumer
│   ├── battle-result-consumer/
│   ├── profile-bootstrap-consumer/
│   ├── qbank-client/         # Rust gRPC 客户端（题库服务契约）
│   └── migrations/           # PostgreSQL schema / migration
├── contracts/
│   ├── graphql/              # user-api 与 recommendation SDL
│   └── proto/                # qbank.v1 标准 gRPC 契约
├── docs/
│   ├── architecture.md       # 架构基线（用户提供）
│   ├── git-workflow.md       # 分支、提交、评审与合并规范
│   └── specs/                # 规格与任务清单
├── deploy/                   # 网关模板（deploy/gateway/default.conf.template）、Dockerfile 与 docker-compose
├── scripts/                  # 开发脚本与 Git hooks
```

## 并行开发

多智能体通过 Git worktree 隔离：

```bash
git worktree add .worktrees/frontend       -b feat/frontend-brand-i18n       develop
git worktree add .worktrees/backend-core   -b feat/backend-core-services     develop
```

每个智能体只在自己 worktree 内提交；合并前必须由独立 review 智能体审查。

## 本地验证

```bash
npm --prefix frontend run type-check
npm --prefix frontend run lint:check
npm --prefix frontend test
cd backend && cargo fmt --all -- --check
cd backend && cargo clippy --workspace --all-targets -- -D warnings
cd backend && cargo test --workspace
```

题库服务尚未实现；`contracts/proto/qbank.proto` 与 `backend/qbank-client` 保留稳定的 gRPC 契约，便于后续接入。

网关与前后端联调（推荐方式，一条命令拉起数据库、5 个服务、网关与前端）：

```bash
bash scripts/dev_stack.sh up
# 浏览器访问 http://127.0.0.1:8080
bash scripts/dev_stack.sh status
bash scripts/dev_stack.sh down
```

## 部署与数据库

本地开发/测试使用的 PostgreSQL 18 与 Redis 8 由脚本拉起（端口 `15432`/`16379`），不依赖 Docker：

```bash
bash scripts/dev_db.sh start    # 启动，并打印 DATABASE_URL / REDIS_URL
bash scripts/dev_db.sh status   # 查看状态
bash scripts/dev_db.sh stop     # 停止
bash scripts/dev_db.sh reset    # 清空并重新初始化
```

`scripts/dev_stack.sh up` 会在数据库就绪后构建并启动 `auth-service`、`user-api`、`user-api-sse`、
`recommendation-service`、`battle-service`，渲染 nginx 网关并监听 `8080`。

生产/集成部署使用 `deploy/docker-compose.yml`，其中已包含 `postgres:18-alpine` 与 `redis:8-alpine`（数据持久化到命名卷 `pgdata`/`redisdata`，仅通过内部网络访问，不暴露宿主机端口）：

```bash
docker compose -f deploy/docker-compose.yml up -d --build
```

> 该命令需要能访问容器镜像仓库的环境；本机无法拉取镜像时请使用 `scripts/dev_stack.sh`。

`auth-service` 与 `user-api` 会等待 `postgres`/`redis` 健康后启动，并在启动时自动执行 `backend/migrations` 下的迁移（`sqlx::migrate!`）。默认凭据与服务端口见 compose 文件。

> 注意：`scripts/dev_db.sh` 只用于本机测试；生产部署使用 compose 中的 `postgres`/`redis` 服务。

## 许可证

本项目采用 [GNU General Public License v3.0](LICENSE)（SPDX 标识：`GPL-3.0-only`）。任何分发或修改后的版本都必须以同一许可证开源。
