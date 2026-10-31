# Physics-World

面向学校的物理答题游戏：单人练习、章节刷题、实时对战、个性化推荐与排行榜。

技术栈固定为 Vue 3 + TypeScript（前端）、Rust（业务服务）、C++20 + 腾讯 Flare + Blade（题库服务），部署目标为 TKE。完整架构说明见 [docs/architecture.md](docs/architecture.md)。

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
│   ├── qbank-client/         # Rust gRPC 客户端
│   ├── question-bank/        # C++20 + Tencent Flare + Blade
│   ├── gateway/              # Ingress-Nginx + OpenResty Lua
│   ├── config/               # 动态配置默认值（按 namespace）
│   └── migrations/           # PostgreSQL schema / migration
├── contracts/
│   ├── graphql/              # user-api 与 recommendation SDL
│   └── proto/                # qbank.v1 标准 gRPC 契约
├── docs/
│   ├── architecture.md       # 架构基线（用户提供）
│   └── git-workflow.md       # 分支、提交、评审与合并规范
├── deploy/                   # Nginx 网关模板、Dockerfile 与 docker-compose
├── scripts/                  # 开发脚本与 Git hooks
└── tests/e2e/                # Playwright 全链路测试
```

## 并行开发

多智能体通过 Git worktree 隔离：

```bash
git worktree add .worktrees/frontend       -b feat/frontend-brand-i18n       develop
git worktree add .worktrees/backend-core   -b feat/backend-core-services     develop
git worktree add .worktrees/question-bank  -b feat/backend-question-bank     develop
```

每个智能体只在自己 worktree 内提交；合并前必须由独立 review 智能体审查。

## 本地验证

```bash
cd frontend && npm ci && npm run type-check && npm run lint:check && npm test
cd backend && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd backend/question-bank && blade test //...
```

网关与前后端联调：

```bash
cd frontend && npm run build
NGINX_BIN=/usr/sbin/nginx scripts/smoke_backend.sh
```

## 许可证

本项目采用 [GNU General Public License v3.0](LICENSE)（SPDX 标识：`GPL-3.0-only`）。任何分发或修改后的版本都必须以同一许可证开源。
