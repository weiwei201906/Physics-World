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
│   ├── migrations/           # PostgreSQL schema / migration
│   └── deploy/               # Kubernetes manifests
├── contracts/
│   ├── graphql/              # user-api 与 recommendation SDL
│   └── proto/                # qbank.v1 标准 gRPC 契约
├── docs/
│   ├── architecture.md       # 架构基线（用户提供）
│   └── git-workflow.md       # 分支、提交、评审与合并规范
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
cd backend && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo nextest run
cd backend/question-bank && blade test //...
```

> 当前 Linux 环境已具备 g++ 15 / clang 21 / cmake 4 / protoc 3.21；Node.js、Rust 默认工具链与 Blade 尚未就绪，对应构建与测试命令待安装后执行。运行 cmake/clang/protoc 前需清除 Trae 注入的 `LD_LIBRARY_PATH` 与 `LD_PRELOAD`。
