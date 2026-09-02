# 术语表

文档基线：**2026-08-28**。同一概念只保留一套说法。

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
| 待审批动作 | Pending action | 工具执行后等人审的对象，表 `ai_pending_actions`。**本体写不走它**——聊天卡批 pending 是假执行，禁止对 `ontology.*` 伪造 EXECUTED。 |
| 业务事项 | Business case | 运行中的业务工作项；含 append、workflow、表单。 |
| 派工工单 | Dispatch order | 派工主实体；重排、协同、时间线围绕它展开。 |
| 方向航班 | Flight | 一班进港 **或** 一班出港。`direction` 只能是 `inbound` \| `outbound`，禁止 `both`。不是监控表的一行。 |
| 周转链 | TurnaroundLink | 同机保障链：`inbound_flight_id` ↔ `outbound_flight_id`。不是旅客衔接，也不是 Flight 的另一种写法。 |
| 监控行 | Flight monitor row | 热路径读模型表 `flight_monitor_rows`。一格一行，进出港是列。`row_id` 写入后不因建链/拆链改变。**不是**本体对象。 |
| 运行资源本体 | Ops ontology | `Aircraft` / 占用 / 口 / 周转链；HTTP `/api/v2/ontology`。 |
| 动作本体 | flight-ops.v1 | 同一对象上的只读 / 建议 / 受控写；HTTP `/api/v2/ai/ontology`。 |
| 本体动作 | Object.action | `flight-ops.v1` 里的动作（如 `Flight.add_note`、`StandOccupation.allocate`）；治理字段（启用/风险/审批）来自「代码底 + 动作 overlay」。 |
| 工具 / 适配器 | Tool / adapter | `ontology.lookup`/`propose_action` 等执行入口；有**固定角色**（内部只读 / proposal_only），**不是** `flight-ops.v1` 动作，不进合同、不作为独立动作登记。 |
| 本体动作服务 | Ontology action services | 每条只读或建议动作一个应用服务，由 HTTP 选型。 |
| 受控写执行器 | DomainActionExecutor | 审批后的写动作落到既有领域服务。 |
| 提案 | Action proposal | `ai_action_proposals` 一行；**唯一受控写落点**。批「提案」才真写，`approve + execute` 走执行器，**不等于** pending-action。 |
| 动作覆盖层 | Action overlay | 对代码 schema 已知 `(object, action)` 键的启用/风险/审批覆盖；`load_governed_schema()` 是唯一能得到完整动作 schema 的入口。 |
| 字段覆盖层 | Field overlay | `ontology_field_overlays`：为代码合同**已知对象**补充字段元数据。不能造对象、造写真动作、改核心字段类型。与动作 overlay 不是同一张表。 |
| 元数据码表 | Metadata catalog | `metadata_catalogs` + entries。机型、ICAO 等级等取值集合，不是一等本体对象。 |
| 任务锚点 | TaskType.anchor | 派工挂靠：`inbound` \| `outbound` \| `link`。与计算计划时刻的 `generation_anchor_type` 不同。 |
| 实例扩展属性 | attributes | 目录/运行表 JSONB。key 必须是已启用 overlay 字段；未知 key 或类型不对 → 400。人员扩展落 `personnel_runtime.attributes`。 |
| 业务外键 | Application-level FK | 迁移 `120` 后无物理 FK。`catalog_ref` / `object_ref` 由应用层解析；目标停用/改码若仍被引用 → 409。 |
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
- 「一个 Flight 同时表示进出港」作为现行身份；`direction=both`
- 把 `flight_monitor_rows` 当成本体对象
- 把 `FlightLeg` 当作 `flight-ops.v1` 合同对象
- 把字段 overlay 和动作 overlay 混成一个词
- 产品文档里堆任务波次编号、评分口号；那些留在路线图/计划即可
