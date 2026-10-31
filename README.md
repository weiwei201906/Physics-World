# Physics-World

面向学校的物理答题游戏：单人练习、章节刷题、实时对战、个性化推荐与排行榜。

技术栈固定为 Vue 3 + TypeScript（前端）、Rust（业务服务）、C++20 + 腾讯 Flare + Blade（题库服务），部署目标为 TKE。完整架构说明见 [docs/architecture.md](docs/architecture.md)。

## 目录结构

```text
Physics-World/
├── frontend/                 # Vue 3 + Vite + Pinia + Vitest
├── backend/
│   ├── rust/                 # Cargo workspace，多服务 crate
│   ├── cpp/                  # question-bank：Blade + Flare + gRPC
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
git worktree add .worktrees/frontend      -b feat/frontend-brand-i18n   develop
git worktree add .worktrees/backend-rust  -b feat/backend-rust-scaffold develop
git worktree add .worktrees/backend-cpp   -b feat/backend-cpp-scaffold  develop
```

每个智能体只在自己 worktree 内提交；合并前必须由独立 review 智能体审查。

## 本地验证

```bash
cd frontend && npm ci && npm run type-check && npm run lint:check && npm test
cd backend/rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo nextest run
cd backend/cpp && blade test //question-bank/...
```

> 当前 WSL 工具链尚未安装，后端构建与测试命令在工具链就绪后执行；前端测试需要 Linux 版 Node.js。
