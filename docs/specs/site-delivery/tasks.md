# Tasks: 站点交付、文档纠偏与微服务迭代

> 每步完成即勾选；[P] 表示可与相邻任务并行。

## T1 文档纠偏 [P]
- [ ] `README.md`：删除不存在的 `tests/e2e/`、`backend/gateway/`、`backend/config/`；改为可执行的验证命令；网关地址改为 `http://127.0.0.1:8080`。
- [ ] `docs/specs/database-integration/spec.md`、`tasks.md`：`0001_init.sql` → `init.sql`，补 `consumers.sql`，连接池数量对齐代码。
- [ ] `docs/architecture.md`：校对端口/服务/契约路径，未实现项标注计划中。
- [x] 验证：`grep -rn "tests/e2e|0001_init.sql|/usr/sbin/nginx" README.md docs/` 无命中（本规格文件作为审计记录保留原文件名说明）。

## T2 Git 结构 [P]
- [ ] 根 `.gitignore`：忽略 `backend/target/`、`frontend/node_modules/`、`frontend/dist/`、`.worktrees/`、`*.log`、`.env*`、题库构建产物。
- [ ] 清理无用文件/空目录，`git status` 只出现预期改动。
- [ ] 按 spec 第 4.2 节提交（ASCII，单一主题），`git push origin develop`。
- [ ] 验证：`git status --short` 为空；`git log --oneline -12` 符合序列。

## T3 运行脚本
- [ ] `scripts/dev_db.sh`：修复 redis-server 悬空符号链接、liburing 库路径、迁移执行顺序。
- [ ] 新增 `scripts/dev_stack.sh`：`up|down|status`，拉起 5 个服务 + nginx 8080 + 前端构建。
- [ ] 验证：`bash scripts/dev_db.sh status`；`bash scripts/dev_stack.sh up`；`curl /healthz`。

## T4 网关配置（主智能体）
- [ ] `deploy/gateway/default.conf.template`：静态 SPA、`/auth/`、`/graphql`、`/graphiql`、`/config`、`/events`、`/rec/`、`/ws/battle`、`/healthz`。
- [ ] `deploy/Dockerfile.gateway`、`deploy/docker-compose.yml` 变量与模板一致。
- [ ] 验证：`envsubst` 渲染成功；`nginx -t` 通过；`curl` 各路由状态码符合预期。

## T5 认证联调（主智能体）
- [ ] auth-service 重建并重启，确认 camelCase 契约、`/auth/refresh-token`、201 注册。
- [ ] 通过网关验证注册/登录/刷新/verify/jwks。
- [ ] 验证：curl 返回 `accessToken`、`expiresIn=900`、`exp`；`/auth/verify` 带 Bearer 返回 204 + identity header。

## T6 微服务子智能体 [P]
- [ ] `auth-service`：feat/auth-hardening worktree，回归 + 补测。
- [ ] `user-api`：feat/user-api-hardening，GraphQL/REST 与前端契约对齐。
- [ ] `user-api-sse`：feat/sse-hardening，广播/心跳/重连。
- [ ] `recommendation-service`：feat/recommendation-hardening，算法边界与 schema。
- [ ] `battle-service`：feat/battle-hardening，匹配与 WS 协议。
- [ ] 三个 consumer：feat/consumer-hardening，幂等/DLQ/无 Kafka 单测。
- [ ] `question-bank`：feat/qbank-hardening，Flare 骨架 + 存储/缓存 + proto。
- [ ] 验证：每个 crate `cargo fmt`、`cargo clippy -p <crate> --all-targets -- -D warnings`、`cargo test -p <crate>` 全绿；主智能体 review 后 `git merge --no-ff`。

## T7 集成验证
- [ ] `cargo test --workspace` 全绿。
- [ ] `bash scripts/smoke_backend.sh` → `smoke test passed`。
- [ ] 浏览器访问 `http://127.0.0.1:8080`：注册/登录/主界面可用（截图）。
- [ ] 验证：`/healthz`、`/events` 头、`/graphql` me、`/rec/graphql` 均有响应。

## T8 收尾
- [ ] 更新 `docs/specs/site-delivery/tasks.md` 状态并提交。
- [ ] `git push origin develop`；确认远端同步。
