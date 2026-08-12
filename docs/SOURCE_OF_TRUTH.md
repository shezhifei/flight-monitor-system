# 文档事实来源

文档基线：**2026-08-11**。改文档前先对这里列出的源码/脚本/配置，不要抄其他文档当真相。

## 1. 默认 HTTP 后端

| 事实 | 位置 |
|---|---|
| HTTP / SSE / 静态前端 | `services/api-server/` |
| 路由模块 | `services/api-server/crates/api/src/routes/*.rs` |
| 路由注册 | `services/api-server/crates/server/src/web.rs` |
| 进程入口与装配 | `services/api-server/crates/server/src/main.rs` |
| 应用服务 | `services/api-server/crates/application/src/services/*.rs` |
| 领域模型 | `services/api-server/crates/domain/src/models/*.rs` |
| Postgres 仓储 | `services/api-server/crates/infrastructure/src/repositories/*.rs` |

新 HTTP 行为默认写进 `services/api-server/`。Python HTTP 不是当前主链。

## 2. Python AI 侧车与 worker

| 事实 | 位置 |
|---|---|
| 侧车源码 | `services/ai-sidecar/src/` |
| 入口 | `scripts/host/ai_sidecar_entrypoint.py` |
| DI | `services/ai-sidecar/src/di/container.py` |
| Runtime providers | `services/ai-sidecar/src/infrastructure/runtime/providers.py` |
| 应用层 | `services/ai-sidecar/src/application/` |
| 历史 worker（本地归档） | `legacy-backend/src/application/worker_main.py` 等 |

约束：

- 可做 AI 侧车、批处理、辅助脚本；不新增默认 Python HTTP 路由。
- 新代码进 `services/ai-sidecar/`，不要往 `legacy-backend/` 加功能。
- **写边界**：侧车只写控制面表（如 `ai_*`、`agent_*`、`aip_*`、`todo_agent_context`、`ai_runtime_commands`）。禁止直写核心真相表（`flights`、`dispatch_orders`、`todos`、`business_cases`、`domain_event_outbox` 等）。写业务域经 Rust DomainAction 或内部 API（Service Identity JWT）。失效/归档的旧 handler 不得假装成功（有测试约束）。

## 3. 启动与部署

| 事实 | 位置 |
|---|---|
| 统一入口 | `scripts/fms.ps1` |
| Docker 标准 | `-Runtime docker` + `deploy/docker/docker-compose.distributed.yml` |
| Docker 边缘 | `deploy/docker/docker-compose.edge.yml` |
| Host Caddy | `scripts/host/start_caddy_http3_proxy.ps1` |
| 双击入口 | `deploy/docker/Start-FlightMonitorDocker.bat` 等 |

## 4. Vault 与密钥

- 脚本：`scripts/vault/*.ps1`
- 说明：`docs/DEPLOYMENT.md`
- env 示例只放非敏感 bootstrap 参数
- 长期秘密在 Vault `kv/fms/*`，不进 `.env`、compose、文档样例

## 5. 配置与 DI

| 事实 | 位置 |
|---|---|
| Rust 配置 | `services/api-server/crates/infrastructure/src/config/mod.rs` |
| 侧车 DI | `services/ai-sidecar/src/di/container.py` |
| 侧车 runtime providers | `services/ai-sidecar/src/infrastructure/runtime/providers.py` |
| 历史 Python 配置 | `legacy-backend/config/app_config.yaml` |

## 6. 前端

| 事实 | 位置 |
|---|---|
| Vue 源码 | `frontend/vue-app/` |
| 构建产物 | `frontend/vue-app/dist/` |
| 静态挂载 | `services/api-server/crates/api/src/routes/static_files.rs` |
| 兼容资源 | `frontend/js/`、`frontend/static/`、`frontend/vendor/` 等 |
| 差异审计（本地 ops） | `docs/operations/frontend-parity-audit.md` |

正式路径：`/frontend/<page>.html`。兼容：`/frontend/html/<page>.html`。  
根路径 `/` 当前仍 302 到兼容登录页；业务入口请用 `/frontend/login.html`。

## 6.1 移动端（Android Flutter + Rust）

| 事实 | 位置 |
|---|---|
| Flutter App | `mobile/flutter-app/` |
| Rust 逻辑（零 frb） | `mobile/core/crates/mobile-core/` |
| frb façade | `mobile/core/crates/mobile-ffi/` |
| CI | `.github/workflows/mobile.yml` |
| 执行计划 / 交接 | 本地 `docs/plans/android-flutter-rust-rebuild-*.md`（`docs/plans/*` 默认 gitignore） |
| 端点 / 推送 / release | `docs/mobile/endpoint-checklist.md`、`push-channel-eval.md`、`release-notes.md` |
| 旧 Kotlin App 归档 | `legacy/android-kotlin/`（只读对拍，不再修改） |

约束：后端零改动；`mobile-core` 禁止依赖 flutter_rust_bridge；token/secret 不进日志；release base_url 强制 https（`--dart-define=API_BASE_URL`）。

## 7. 数据库与事件

| 事实 | 位置 |
|---|---|
| 迁移 | `migrations/*.sql` |
| 当前最新 | `118_extend_dispatch_alerts_overrun.sql` |
| 空库自举 | `sqlx migrate run --source migrations` |
| Outbox / CDC 设计 | `docs/architecture/ADR-0003-domain-event-outbox-cdc-relay.md` |

`CREATE INDEX CONCURRENTLY`：独占迁移文件，首行 `-- no-transaction`。

## 8. 消息与实时

| 事实 | 位置 |
|---|---|
| MQ gateway 编排 | `deploy/docker/docker-compose.distributed.yml` |
| MQ 镜像 | `Dockerfile.mq-gateway` |
| MQ 源码 | `services/mq-gateway/` |
| SSE | `services/api-server/crates/api/src/sse/` |
| Runtime diagnostics 表 | `migrations/068_create_runtime_diagnostic_events.sql` |

## 9. Application 层数据访问

- `services/api-server/crates/application/src/services/**` 经 `fms_domain` ports 访问数据，不直接 `sqlx::query*`，不直接依赖具体 repository 类型。
- `domain_event_outbox` 经 `DomainEventOutboxRepository`。
- 债务清单：`application/tests/application_boundary_inventory.rs`（清单条目只允许减少）。
- 路由层：`api/tests/layer_boundary_guard.rs`。

## 10. 文档规则

- 后端叙述以 Rust 为主。
- 启动/部署/端口/Vault/迁移/前端正式路径变化时，同步 `README.md`、`QUICK_START.md`、`docs/DEPLOYMENT.md`。
- 路由变化同步 `docs/API_ROUTE_SNAPSHOT.md`。
- 不把 Python HTTP、旧 cutover、旧 nginx 分流写成当前默认路径。
- 计划、审计、runbook 若保留，标明性质；默认不进 git（见 `.gitignore`），技术债主计划除外。
