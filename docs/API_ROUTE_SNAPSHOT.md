# API 路由快照

文档基线：**2026-08-13**。端点实现以 `services/api-server/crates/api/src/routes/*.rs` 为准；生产注册以 `services/api-server/crates/server/src/web.rs` 为准。本页供导航与巡检，不替代源码。

## 1. 入口与健康

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/health/*` | 健康检查 | `routes/health.rs` |
| `/api/v2/metrics` | Prometheus 指标 | `routes/metrics.rs` |
| `/api/v2/system/*` | 系统健康、机场上下文、系统标志、导入等 | `routes/system.rs` |
| `/api/v2/system/runtime/streaming/*` | streaming / SSE 统计 | `routes/scheduler.rs` |
| `/api/v2/system/scheduler/*` | scheduler 状态与触发 | `routes/scheduler.rs` |

（表中 `routes/` 指 `services/api-server/crates/api/src/routes/`。）

## 2. 认证与权限

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/auth/login` | 登录 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/register` | 注册 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/refresh` | token 刷新 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/sse-token` | SSE token | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/logout` | 登出 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/heartbeat` | 在线心跳 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/me*` | 当前用户、资料、操作员上下文 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/users*` | 用户管理 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/roles*` | 角色管理 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/permissions*` | 权限查询与授权变更 | `services/api-server/crates/api/src/routes/auth.rs` |
| `/api/v2/auth/admin/permission-templates*` | 权限模板 | `services/api-server/crates/api/src/routes/auth_admin.rs` |

## 3. 航班、KPI 与异常

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/flights*` | 航班主数据、航班腿、状态、时间线、导入相关主链 | `services/api-server/crates/api/src/routes/flights.rs` |
| `/api/v2/archive/*` | 航班归档查询与触发 | `services/api-server/crates/api/src/routes/archive.rs` |
| `/api/v2/labels*` | 标签定义、航班标签、航班腿标签 CRUD | `services/api-server/crates/api/src/routes/labels.rs` |
| `/api/v2/labels/definitions*` | 标签定义管理（仅管理员） | `services/api-server/crates/api/src/routes/labels.rs` |
| `/api/v2/labels/flight-labels*` | 航班标签绑定管理 | `services/api-server/crates/api/src/routes/labels.rs` |
| `/api/v2/anomalies*` | 异常列表、规则、确认、解决 | `services/api-server/crates/api/src/routes/anomalies.rs` |
| `/api/v2/kpi/*` | KPI snapshot、trend、compare、baseline compare | `services/api-server/crates/api/src/routes/kpi.rs` |
| `/api/v2/dashboard/workbench` | 运行工作台聚合数据 | `services/api-server/crates/api/src/routes/dashboard.rs` |

## 4. 派工与协同

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/dispatch-orders*` | 派工工单、分配、时间线、冲突、重排、follow-up | `services/api-server/crates/api/src/routes/dispatch.rs` |
| `/api/v2/dispatch/*` | 派工规则、资源、排班、分析与直连兼容面 | `services/api-server/crates/api/src/routes/dispatch.rs`, `dispatch_resources.rs` |
| `/api/v2/dispatch/collaboration/*` | 协同群组、协同事件、工单协作视图 | `services/api-server/crates/api/src/routes/dispatch_collaboration.rs` |
| `/api/v2/dispatch/chat/*` | 派工聊天 | `services/api-server/crates/api/src/routes/dispatch_chat.rs` |
| `/api/v2/dispatch/analytics/resource-utilization/*` | 资源利用率 summary / stands / teams / equipment | `services/api-server/crates/api/src/routes/resource_utilization.rs` |
| `/api/v2/workflows/integrations/dispatch/*` | Flowable 驱动派工触发、待处理、推荐、指派 | `services/api-server/crates/api/src/routes/workflow_dispatch.rs` |

## 5. 业务事项与工作流

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/business-cases*` | 业务事项、append、timeline、状态与查询 | `services/api-server/crates/api/src/routes/business_cases.rs` |
| `/api/v2/business-case-types*` | 事项类型、BPMN、状态 | `services/api-server/crates/api/src/routes/business_case_types.rs` |
| `/api/v2/business-case-workflows*` | 业务事项工作流运行 | `services/api-server/crates/api/src/routes/business_case_workflows.rs` |
| `/api/v2/workflow-forms/*` | workflow form 模板与绑定 | `services/api-server/crates/api/src/routes/workflow_forms.rs` |
| `/api/v2/business_cases/{case_id}/workflow/forms*` | 事项工作流表单查询与提交 | `services/api-server/crates/api/src/routes/workflow_forms.rs` |
| `/api/v2/workflows/*` | Flowable definitions、instances、history、drafts | `services/api-server/crates/api/src/routes/flowable.rs` |

## 6. AI、Todo 与 NL Query

### 6.1 核心 AI 路由

主 scope 在 `routes/ai/`（目录模块）；部分子能力由同 scope 内其它模块挂接。

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/ai/capabilities` | 能力声明 | `routes/ai/` |
| `/api/v2/ai/tools*` | 工具列表、分类与执行 | `routes/ai/` |
| `/api/v2/ai/pending-actions*` | 待审批动作（通过/拒绝/diff/result） | `routes/ai/` |
| `/api/v2/ai/entities*` | 实体配置、prompt、tools、模型与连接测试 | `routes/ai/` |
| `/api/v2/ai/executions*` | 执行查询与取消 | `routes/ai/` |
| `/api/v2/ai/events/stream` | AI 事件流（可转发侧车） | `routes/ai/` |
| `/api/v2/ai/skills*` | Skills 列表 | `routes/ai_config_proxy.rs`（挂入 `/api/v2/ai`） |
| `/api/v2/ai/entities/{id}/capabilities*` | 实体能力声明与校验 | `routes/ai_config_proxy.rs` |
| `/api/v2/ai/entities/{id}/mcp*` | MCP 服务器、绑定、probe | `routes/ai_config_proxy.rs` |
| `/api/v2/ai/entities/{id}/skills*` | Skills 绑定与 probe | `routes/ai_config_proxy.rs` |
| `/api/v2/ai/cache/*` | 缓存指标与失效 | `routes/ai_config_proxy.rs` |
| `/api/v2/ai/execution-readiness*` | 执行就绪报告与相关状态 | `routes/ai_execution_readiness/` |

源码根路径：`services/api-server/crates/api/src/`。

### 6.2 扩展 AI 路由

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/ai/eval/*` | LLM Eval jobs | `routes/ai_eval.rs` |
| `/api/v2/ai/nl-query/*` | NL Query（建议、执行、schema、历史、流） | `routes/nl_query/` |
| `/api/v2/ai/copilot/*` | 业务案例草稿、批次、派发重试 | `routes/ai_copilot.rs` |
| `/api/v2/ai/ontology/*` | 本体 schema / 对象 / 动作 | `routes/ai_ontology.rs` |
| `/api/v2/ai/proposals*` | 动作建议：生成、审批、执行、统计 | `routes/ai_proposals/` |
| `/api/v2/ai/proposals/{id}/rollback*` | 回滚与补偿计划 | `routes/ai_rollback.rs` |
| `/api/v2/ai/runs/{run_id}/resume` | 运行恢复 | `routes/ai_resume.rs` |
| `/api/v2/ai/jobs/{job_id}/runs/{run_id}/checkpoints` | 检查点列表 | `routes/ai_resume.rs` |
| `/api/v2/ai/media/*` | ASR / TTS | `routes/ai_media.rs` |
| `/api/v2/ai/realtime/audio` | 实时音频 WebSocket | `routes/ai_realtime_audio.rs` |
| `/api/v2/ai/micro-models/*` | 微模型注册与执行 | `routes/ai_micro_models/` |
| `/api/v2/ai/jobs/*` | Job / Run 只读查询 | `routes/ai_jobs.rs` |

说明：独立 `/api/v2/ai-proxy/*` **已移除**（测试强制 404）。侧车转发走 `routes/ai/` 内代理逻辑，不是单独的 `ai_proxy` 模块。

### 6.3 内部 AI 端点

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/internal/ai/v1/runs/{run_id}/events` | 运行事件摄入（Service Identity） | `routes/ai_internal/` |
| `/internal/ai/v1/runs/{run_id}/complete` | 运行完成回调 | `routes/ai_internal/` |
| `/internal/ai/v1/runs/{run_id}/fail` | 运行失败回调 | `routes/ai_internal/` |

### 6.4 Todo

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/todos*` | Todo CRUD、状态、agent context | `routes/todos.rs` |

> `ai_config_proxy` / `ai_execution_readiness` 挂在 `/api/v2/ai` 父 scope 下，不要再单独挂一层同名 scope（actix 前缀会互斥）。`ai_sidecar_dependency.rs` 只做依赖元数据。

## 7. 通知、交接班、移动端与参考数据

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/notifications*` | 通知发送、收据、在线用户、dispatch 通知 | `services/api-server/crates/api/src/routes/notifications.rs` |
| `/api/v2/shift-handovers*` | 交接班创建、查询、提交、确认 | `services/api-server/crates/api/src/routes/shift_handovers.rs` |
| `/api/v2/mobile/workbench` | 移动工作台 | `services/api-server/crates/api/src/routes/mobile.rs` |
| `/api/v2/mobile/operations/events` | 移动操作事件 | `services/api-server/crates/api/src/routes/mobile.rs` |
| `/api/v2/mobile/uploads*` | 移动附件上传与下载 | `services/api-server/crates/api/src/routes/mobile.rs` |
| `/api/v2/mobile/devices*` | 设备注册、心跳、注销 | `services/api-server/crates/api/src/routes/mobile.rs` |
| `/api/v2/reference/*` | 部门、事项类型、事项状态等参考数据 | `services/api-server/crates/api/src/routes/reference.rs` |

## 8. 实时、指标与静态资源

| 路由组 | 说明 | 源码 |
|---|---|---|
| `/api/v2/sse/stream` | 统一 SSE | `api/src/sse/handler.rs` |
| `/api/v2/metrics` | Prometheus 指标 | `routes/metrics.rs` |
| `/frontend/<page>.html` | Vue 正式页（`vue-app/dist`） | `routes/static_files.rs` |
| `/frontend/assets/*` | Vue 构建资产 | `routes/static_files.rs` |
| `/frontend/html/*` | 旧 HTML 兼容页 | `routes/static_files.rs` |
| `/frontend/js/*` 等 | 旧静态资源 | `routes/static_files.rs` |
| `/api/v2/openapi.json` | OpenAPI（utoipa） | `server/src/web.rs` |
| `/swagger-ui/*` | Swagger UI | `server/src/web.rs` |
| `/` | 302 → `/frontend/login.html` | `server/src/web.rs` |

源码根：`services/api-server/crates/`。

## 8.1 统一错误响应体

错误经 `ApiError` 映射（`application/src/schemas/response.rs`）：

```json
{
  "success": false,
  "error": {
    "code": "HTTP_500",
    "message": "数据存储错误",
    "type": "http_error",
    "timestamp": "2026-08-11T00:00:00Z",
    "kind": "database"
  }
}
```

- `kind` 可选：`auth | config | database | network | unknown`。客户端按 `kind` 分支，不要抠 `message` 文案。
- 连接串、主机等细节只写日志，不进响应体。

## 9. 未挂载的路由模块

以下模块存在于 `routes/` 目录中但**未在生产环境挂载**（`web.rs`），仅在测试或元数据层面使用：

| 模块 | 状态 | 说明 |
|---|---|---|
| `event_rules.rs` | 仅测试 | 事件驱动派工规则 CRUD（调整规则、生成规则、规则预览），仅在测试 app 中注册 |
| `ping.rs` | 仅 OpenAPI | `/api/ping` 和 `/api/v2/ping` 仅在 utoipa OpenApi 文档中声明，未作为中间件路由挂载；测试中断言其返回 404 |
| `ai_sidecar_dependency.rs` | 元数据模块 | 路由依赖清单静态导出，不挂载任何 HTTP 路由 |

## 10. 巡检规则

路由变更时至少同步：

- 本文件的路由组和源码映射。
- `README.md` 或 `docs/SYSTEM_MANUAL.md` 中涉及的能力列表。
- 前端页面路径变更时同步 `QUICK_START.md` 和 `docs/SOURCE_OF_TRUTH.md`。

建议验证命令：

```powershell
cd services/api-server
cargo test -p fms-api routes
cargo test -p fms-api --test layer_boundary_guard
```
