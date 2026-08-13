# 航班监控系统手册

文档基线：**2026-08-11**。面向开发与运维，描述分层、子系统与运行约束。细节命令以 `QUICK_START.md` / `docs/DEPLOYMENT.md` 为准；路由明细以 `docs/API_ROUTE_SNAPSHOT.md` 与源码为准。

## 1. 系统定位

覆盖机场运行主链：

- 航班主数据、状态、时间线、归档、标签、外部同步
- 派工资源、工单、协同、重排、排班、Flowable 派发
- 业务事项、append、部门可见性、工作流与表单
- 认证、权限、会话、操作员上下文、设备
- 异常、通知、交接、健康检查、SSE、运行诊断
- AI 工具执行、待审批、NL Query、LLM Eval
- 移动作业工作台与附件
- Vue 3 多页前端（旧静态页仅兼容）

## 2. 架构原则

1. **HTTP 默认在 Rust**：新路由与后端行为写在 `services/api-server/`。
2. **Python 收窄角色**：AI 侧车 + 可选 worker/runtime/脚本；不新增 Python HTTP 主链。
3. **显式装配**：进程启动时注入依赖；不用导入期全局单例当正式接线方式。
4. **密钥走 Vault**：CE + AppRole + Agent 渲染文件；缺失关键 secret 应拒绝启动。
5. **存储分工**：PostgreSQL 业务/配置/outbox；Redis 缓存与实时状态；RocketMQ gateway 消息边界；Flowable 独立 Tomcat/REST。

## 3. 目录与分层

```text
services/api-server/          # 默认 HTTP 后端（Cargo workspace）
  crates/api/                 # routes, middleware, SSE, static
  crates/application/         # use cases, DTO
  crates/domain/              # models, ports
  crates/infrastructure/      # config, DB, repos, integrations
  crates/server/              # 进程入口与装配

services/ai-sidecar/          # Python AI 侧车（最小运行集）
  src/domain/ai/
  src/application/            # services, plugins, ports, api
  src/infrastructure/ai/      # LLM、工具、配置、MCP、缓存、runtime
  src/di/                     # DI 容器

services/mq-gateway/          # RocketMQ gateway
legacy-backend/               # 历史 Python 后端（gitignore）
frontend/vue-app/             # Vue 3 多页
migrations/                   # SQL 增量迁移
scripts/                      # fms.ps1、host、vault、侧车入口
deploy/docker/                # 标准 / 边缘 compose
config/                       # 侧车相关配置（如 ai_config.py）
```

## 4. 启动与运行时

### 4.1 Docker 标准拓扑

```powershell
.\scripts\fms.ps1 -Command start -Runtime docker
```

编排：`deploy/docker/docker-compose.distributed.yml`。  
常见服务：`postgres`、`redis`、`rocketmq-namesrv`、`rocketmq-broker`、`mq-gateway`、`rust-api`、`flowable`、Vault 相关服务；宿主机常配 Caddy（18443）。

访问示例：

- `https://localhost:18443/api/v2/health/ping`
- `https://localhost:18443/frontend/login.html`
- `http://localhost:8082/flowable-rest/service/management/engine`

### 4.2 Host Rust

```powershell
.\scripts\fms.ps1 -Command start -Runtime host
```

本机联调：检测/启动依赖组件，默认构建并启动 Rust API，默认跑迁移。日志在 `.runtime/host-services/`。  
参数：`-SkipBuild`、`-SkipMigrations`、`-UseCargoRun`。  
命令：`start` / `stop` / `status` / `logs` / `restart`。

Host 适合排障，不替代 Docker 作为验收基线。

### 4.3 Python AI 侧车

```powershell
.\.venv\Scripts\python.exe scripts\host\ai_sidecar_entrypoint.py
```

- 一律用工作区 `.venv`
- 新业务 HTTP 不写进 Python
- 历史代码见 `legacy-backend/`

## 5. 子系统

### 5.1 Flight

主数据读写、腿/状态/时间线、导入、标签、归档、事件发布。  
写副作用应在应用服务层；写边界见 `docs/architecture/ADR-0002-flight-core-write-boundary.md`。航班/时间线/异常域在 `032` 迁移后强切解耦。

### 5.2 Dispatch

工单、资源、班组、设备、机位、排班、协同聊天、重排预览/应用、Flowable 触发与指派。  
路由 `routes/dispatch*.rs`，应用服务 `dispatch*.rs`。协同账本自 `035` 起；幂等相关见 `069`、`070`。

### 5.3 Business Case

事项类型、append/timeline、workflow run、部门可见性、表单模板与提交。  
`workflow_receipt` 从 context JSON 改为运行时投影（通知表 + `receipt_group_id`）。接收人显示名用 snapshot，避免泄漏内部 ID。

### 5.4 Auth

登录/刷新/登出/心跳、SSE token、用户角色权限与模板、在线状态与踢出、操作员上下文。权限细化见 `066`。

### 5.5 AI

能力与工具、待审批、proposal、本体动作、NL Query、LLM Eval、微模型。
**一条链**：Rust 管 HTTP、鉴权、proposal 与受控写；Python 侧车跑 LLM/工具/MCP。
侧车只写控制面表（`ai_*`、`agent_*`、`todo_agent_context`、`ai_runtime_commands` 等），不直写航班/派工/todo；写域走 `DomainActionExecutor` 或内部 JWT。侧车本体能力只镜像 Rust schema，不维护独立 AIP 动作栈。

### 5.6 Observability

`/api/v2/health/*`、`/api/v2/system/*`、scheduler/streaming、运行时错误监控与 SSE hub。

### 5.7 Mobile

Workbench、operations events、uploads、device register/heartbeat/unregister。迁移：`033`。

### 5.8 并行读模型边界

以下接口不是重复实现，而是面向不同产品工作流的活读模型。修改其中一面时必须同步审计对应客户端，不能只按表名或返回字段相似度合并。

| 事实 | 运行/操作面 | 本体/管线面 | 边界 |
|---|---|---|---|
| 机位 | `PATCH /api/v2/flights/{flight_id}` 写 `flights.stand` | `/api/v2/ontology` 管理 `stand_occupations`，可选同步航班计划 | 即时运行字段与时段占用模型并存 |
| AI 审批 | Vue/ai-react 使用 `/api/v2/ai/pending-actions` | proposal 管线使用 `/api/v2/ai/proposals`，执行时回写 pending action | 人工工具审批与本体动作 proposal 两个工作流通过桥接保持一致 |
| 工单时间线 | `/api/v2/dispatch-orders/{order_id}/timeline` 投影协同账本中的工单状态事件 | `/api/v2/mobile/workbench` 提供移动作业日志/工单摘要 | 同一工单事实按桌面时间线与移动工作台契约分别读取 |

## 6. 前端

| 项 | 路径 |
|---|---|
| 源码 | `frontend/vue-app/` |
| 构建产物 | `frontend/vue-app/dist/` |
| Rust 挂载 | `api` crate `static_files.rs` |
| 正式访问 | `/frontend/<page>.html` |
| 兼容访问 | `/frontend/html/<page>.html` |

约 22 个页面入口（login、dashboard、flight_monitor、dispatch_board、command_center、ai_*、nl_query、system_status、flowable_modeler 等）。新功能只走 Vue 多页。  
根路径 `/` 跳到正式 Vue 登录页 `/frontend/login.html`。

## 7. 配置、密钥与迁移

### 配置

- Rust：compose / host env / Vault 渲染文件
- 系统配置中心：PostgreSQL `system_config`
- 侧车：`config/ai_config.py` 等

### Vault

路径约定：`kv/fms/shared`、`kv/fms/api`、`kv/fms/worker`、`kv/fms/rust-api`、`kv/fms/flowable`。启动前完成 Agent 渲染。

### 迁移

- 目录：`migrations/*.sql`
- 当前最新：`121_add_soft_delete_columns.sql`
- 按编号顺序；Host 默认可自动执行
- 空库可用 `sqlx migrate run --source migrations`
- `CREATE INDEX CONCURRENTLY`：每文件一条，首行 `-- no-transaction`

## 8. 文档治理

| 文档 | 角色 |
|---|---|
| `docs/SOURCE_OF_TRUTH.md` | 事实应对哪份源码 |
| `docs/API_ROUTE_SNAPSHOT.md` | 路由组快照 |
| `docs/DEPLOYMENT.md` | 部署与 Vault |
| `docs/DOCUMENTATION_WORKFLOW.md` | 变更如何同步文档 |
| `docs/GLOSSARY.md` | 术语 |

规则：

- 当前事实、历史计划、一次性审计不要混写
- legacy / deprecated 必须标明
- 计划稿默认本地保留，不进产品文档主链（技术债主计划除外）

## 9. 推荐阅读顺序

1. `README.md` / `QUICK_START.md`
2. `docs/DEPLOYMENT.md`
3. `docs/API_ROUTE_SNAPSHOT.md`
4. `docs/SOURCE_OF_TRUTH.md`
5. `docs/architecture/ONTOLOGY_V1.md`
6. `docs/architecture/ADR-0001` … `ADR-0003`
