# Git 协作规范

## 1. 分支模型

| 分支 | 用途 | 合并来源 | 合并目标 |
|---|---|---|---|
| `main` | 可发布基线，仅接受通过评审的里程碑 | 经评审的 `develop` | - |
| `develop` | 集成分支，所有功能分支的汇合点 | `feat/*` / `fix/*` / `chore/*` | `main`（里程碑） |
| `feat/<scope>-<topic>` | 新功能 | `develop` | `develop` |
| `fix/<scope>-<topic>` | 缺陷修复 | `develop` | `develop` |
| `chore/<topic>` | 构建、CI、依赖、脚手架 | `develop` | `develop` |
| `docs/<topic>` | 文档 | `develop` | `develop` |

`scope` 取值：`frontend`、`auth`、`user-api`、`sse`、`recommendation`、`battle`、`consumer`、`qbank`、`shared`、`contracts`、`gateway`、`repo`、`deploy`。

分支命名示例：`feat/frontend-brand-i18n`、`feat/backend-core-services`、`chore/ci-pipeline`。

## 2. 多智能体并行

- 每个智能体使用独立 worktree：`.worktrees/<name>`，对应独立 feature 分支。
- 智能体不得切换到其他分支，不得合并其他分支，不得改写 `main`。
- 智能体提交前必须确认 `git branch --show-current` 是自己的分支。
- 跨目录共享文件（`contracts/`、根配置）由编排者在基线提交中先落地，智能体只读使用。

## 3. 提交规范

采用 Conventional Commits，提交信息必须是纯 ASCII：

```text
<type>(<scope>): <summary>

[optional body]
```

允许的 `type`：`feat`、`fix`、`refactor`、`test`、`docs`、`chore`、`build`、`ci`、`perf`。

示例：

```text
test(frontend): cover single-flight refresh queue
feat(frontend): add quiz timer and answer feedback
feat(backend-rust): scaffold auth service with refresh rotation
ci(backend): add cargo fmt and clippy pipeline
```

禁止事项：

- 提交信息、源码、脚本、配置中出现中文或其他非 ASCII 字符（`frontend/src/locales/*.json` 为界面文案资源，不受此限）。
- 一个提交混合多个智能体的无关改动。
- `--no-verify` 跳过 hooks。

TDD 提交顺序：先 `test(scope): ...`（红灯），再 `feat(scope): ...`（绿灯），必要时 `refactor(scope): ...`。

## 4. Review 门禁

任何分支合并到 `develop` 前，必须由独立 review 智能体执行：

1. `git diff develop...<branch>` 全量审查。
2. 检查：架构边界、ASCII 约束、TDD 测试覆盖、错误处理、安全（token/header）、无障碍与响应式。
3. 运行当前可用检查（lint、type-check、test、fmt、clippy、blade test）；工具链缺失时标记 `UNVERIFIED` 并列入风险。
4. 输出问题清单：`BLOCKER` / `MAJOR` / `MINOR`，逐条给出文件与行号。
5. `BLOCKER`、`MAJOR` 必须修复后复审；通过后由编排者执行 `git merge --no-ff`。

## 5. 合并与发布

- 功能分支合并：`git merge --no-ff feat/...`，保留分支拓扑。
- `develop` 合并 `main` 前打标签：`v0.x.y`。
- 回滚使用 `git revert`，禁止对 `main` / `develop` 强制推送。

## 6. Hooks

安装：

```bash
./scripts/install-hooks.sh
```

- `commit-msg`：校验 Conventional Commits 格式与 ASCII。
- 工具链就绪后可扩展 `pre-commit` 执行 lint / type-check / test。
