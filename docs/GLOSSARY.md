# 术语表

文档基线：**2026-08-11**。同一概念只保留一套说法。

> 默认 HTTP 后端：`services/api-server/`（Rust）。  
> AI 侧车：`services/ai-sidecar/`。  
> 历史 Python 后端：`legacy-backend/`（本地归档）。

## 当前主链

| 中文 | 英文 | 说明 |
|---|---|---|
| 装配入口 | Composition / wiring | Rust 在 `crates/server` 创建并注入依赖；侧车在 `di/container.py`。不用导入期全局单例当正式接线。 |
| 端口 / 仓储 | Port / Repository | 领域端口在 `fms_domain`；实现落在 infrastructure。应用层经端口访问数据。 |
| 路由层 | API routes | `crates/api/src/routes`；只做 HTTP，业务在 application。 |
| 领域事件 outbox | Domain event outbox | 与业务写同事务落库，再经 CDC/relay 发布；见 ADR-0003。 |
| 正式前端页 | Primary frontend page | `/frontend/<page>.html`，来自 `frontend/vue-app/dist/`。 |
| 信号面 | Signal surface | 本仓库运营台视觉语言。说明 `docs/architecture/SIGNAL_SURFACE.md`，标本 `frontend/signal-surface-preview.html`。 |
| 兼容静态页 | Legacy static page | `/frontend/html/<page>.html`，仅兼容，不扩新功能。 |
| AI 侧车 | AI sidecar | Python 进程，跑工具/LLM/NL Query 等；由 Rust 代理。 |
| 待审批动作 | Pending action | 工具执行后等人审的对象，表 `ai_pending_actions`。 |
| 业务事项 | Business case | 运行中的业务工作项；含 append、workflow、表单。 |
| 派工工单 | Dispatch order | 派工主实体；重排、协同、时间线围绕它展开。 |
| 运行资源本体 | Ops ontology | `Aircraft` / 占用 / 口 / 周转链；HTTP `/api/v2/ontology`。 |
| 动作本体 | flight-ops.v1 | 同一对象上的只读 / 建议 / 受控写；HTTP `/api/v2/ai/ontology`。 |
| 本体动作服务 | Ontology action services | 每条只读或建议动作一个应用服务，由 HTTP 选型。 |
| 受控写执行器 | DomainActionExecutor | 审批后的写动作落到既有领域服务。 |
| 事实来源 | Source of truth | 文档应对的唯一代码依据，见 `docs/SOURCE_OF_TRUTH.md`。 |
| 文档基线 | Doc baseline | 当前约定仍有效的主文档集合与日期戳。 |

## 部署相关

| 中文 | 英文 | 说明 |
|---|---|---|
| 标准 Docker 拓扑 | Distributed docker topology | `docker-compose.distributed.yml` + `fms.ps1 -Runtime docker`。 |
| Host 运行时 | Host runtime | 本机起 Rust API 与依赖组件，联调用。 |
| 边缘运行时 | Edge runtime | 精简 compose，资源受限环境。 |
| Vault 渲染文件 | Vault rendered env | Agent 写出的运行时 env；含密钥，不提交。 |

## 不要再当主叙述的说法

- `runtime_state` 旧快照桥
- `global_container` 旧全局容器别名
- 模块级 `lifecycle_manager` 单例当正式读路径
- 「Python `main.py` 是默认路由挂载源」— 默认 HTTP 已是 Rust
- 「AIP 主链」「Python HTTP 主链」「Rust 子集」
- 「OntologyAdvisoryService 分发门面」
- 产品文档里堆任务波次编号、评分口号；那些留在路线图/计划即可
