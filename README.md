# Flight Monitor System

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

机场航班运行平台：航班主数据、派工协同、认证授权、异常观测、AI 工具执行、移动作业、实时事件。

> **文档基线：2026-08-11**  
> HTTP 默认走 **Rust**（`services/api-server/`）。Python 只做 **AI 侧车** 与可选 worker/runtime。依赖在启动时显式装配，不靠导入期单例。密钥用 Vault CE + AppRole + Vault Agent 渲染文件交付，不要把长期密钥写回 `.env` 或 compose。

## 架构

```text
Browser / Vue MPA
  -> Caddy 或 edge Nginx
  -> Rust API (Actix-web, services/api-server/)
  -> PostgreSQL / Redis / RocketMQ gateway / Flowable
  -> Python AI sidecar（工具执行、NL Query、LLM Eval）
```

要点：

| 组件 | 路径 | 职责 |
|---|---|---|
| HTTP / SSE / 静态前端 | `services/api-server/` | 默认后端 |
| AI 侧车 | `services/ai-sidecar/` | 最小 Python 运行集；入口 `scripts/host/ai_sidecar_entrypoint.py` |
| 历史 Python 后端 | `legacy-backend/` | 本地归档，已 gitignore，不作为主链 |
| 消息网关 | `services/mq-gateway/` | RocketMQ 边界 |
| 前端 | `frontend/vue-app/` | Vue 3 多页；访问 `/frontend/<page>.html` |
| 迁移 | `migrations/*.sql` | 按编号顺序；当前最新 **118** |

标准 Docker 拓扑（`deploy/docker/docker-compose.distributed.yml`）默认服务包括：`rust-api`、`flowable`、`postgres`、`redis`、`rocketmq-namesrv`、`rocketmq-broker`、`mq-gateway`（及 Vault 相关服务）。Python HTTP API 不是默认路径。

旧静态页兼容路径：`/frontend/html/<page>.html`，不要作为新功能入口。根路径 `/` 当前仍跳到兼容登录页，验收请直接打开 `/frontend/login.html`。

## 子系统

- **Flight**：主数据、状态、事件、归档、标签、外部同步与导入
- **Dispatch**：资源、工单、排班、协同、重排、审核、Flowable
- **Business Case**：事项类型、append、工作流、部门可见性、表单
- **Auth**：登录会话、权限模板、角色、在线状态、强制下线
- **AI**：实体配置、工具执行、待审批动作、NL Query、LLM Eval；Rust 管入口，Python 侧车跑模型/工具
- **Observability**：健康检查、系统状态、SSE、runtime diagnostics
- **Mobile**：工作台、操作事件、附件、设备注册/心跳
- **Frontend**：22 个页面入口（login、dashboard、flight_monitor、dispatch_board、command_center、ai_config_center 等）

## 快速启动

```powershell
.\scripts\fms.ps1 -Command start -Runtime docker
```

验证：

- `https://localhost:18443/api/v2/health/ping`
- `https://localhost:18443/frontend/login.html`
- `http://localhost:8082/flowable-rest/service/management/engine`

```powershell
.\scripts\fms.ps1 -Command stop  -Runtime docker
.\scripts\fms.ps1 -Command logs  -Runtime docker
.\scripts\fms.ps1 -Command start -Runtime host   # 本机 Rust + Vault/Redis/Tomcat/RocketMQ
```

细节见 `QUICK_START.md`、`docs/DEPLOYMENT.md`。

## 前端

```powershell
cd frontend\vue-app
npm install
npm run typecheck
npm run build
```

常用页面：`/frontend/login.html`、`dashboard.html`、`flight_monitor.html`、`dispatch_board.html`、`command_center.html`、`ai_config_center.html`、`system_status.html`、`flowable_modeler.html`。

## 数据与迁移

- 目录：`migrations/`，按数字前缀顺序应用
- 当前最新：`118_extend_dispatch_alerts_overrun.sql`
- Host 模式默认跑 `scripts/database/setup_postgresql.sql`、`sqlx migrate run` 和 `scripts/database/verify_runtime_schema.sql`
- 已有库只补未应用迁移；不要手改 `_sqlx_migrations` 伪造进度
- 含 `CREATE INDEX CONCURRENTLY` 的迁移必须独占一个文件，且首行 `-- no-transaction`

## 测试

```powershell
cd services\api-server
cargo test
cargo build --release

cd frontend\vue-app
npm run typecheck
npm run build
```

Python 侧车（始终用仓库 `.venv`）：

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar
```

## 文档（基线）

| 文档 | 用途 |
|---|---|
| `QUICK_START.md` | 本机启动与排障 |
| `docs/DEPLOYMENT.md` | Vault、Docker / host / edge 部署 |
| `docs/SYSTEM_MANUAL.md` | 分层、子系统、运行约束 |
| `docs/API_ROUTE_SNAPSHOT.md` | Rust 路由组快照 |
| `docs/SOURCE_OF_TRUTH.md` | 事实来源映射 |
| `docs/DOCUMENTATION_WORKFLOW.md` | 代码变更如何同步文档 |
| `docs/GLOSSARY.md` | 术语 |
| `docs/architecture/` | ADR、边界、技术债看板 |
| `docs/observability/` | SLO、告警响应 |

本地可留计划稿与审计报告（`docs/plans/`、`docs/operations/` 等），默认不入库；技术债主计划例外见 gitignore。

## 许可

Apache License 2.0，见 `LICENSE` 与 `NOTICE`。公开发布前做完整密钥扫描，并清理不应入库的运行时二进制（如本地 Hyper-V seed `.vhdx` 等）。
