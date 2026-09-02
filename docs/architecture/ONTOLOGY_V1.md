# Ontology V1 — 飞机中心运行本体

Ontology 是一套对象和动作：资源对象由运行台写入，AI 通过同一套对象上的
只读 / 建议 / 受控写动作访问。两个 HTTP 面共用这一模型，不是两套本体。

- 运行资源：`/api/v2/ontology` → `OntologyService`
- AI 协议：`/api/v2/ai/ontology` → 命名动作服务 + `DomainActionExecutor`

`flight-ops.v1` 是定义层唯一类型目录：对象、字段、关系、动作、启用/风险/审批。
运行时加载永远走 `load_governed_schema()`（代码底 + `aip_ontology_actions` overlay）。

字段扩展使用同一治理边界下的 `ontology_field_overlays`：对象/动作仍只能来自代码合同，
overlay 只能为已知对象补充字段元数据（类型、码表/对象引用、可见性和表单 widget），
不能修改代码核心字段类型。需要同时读取两类 overlay 时使用
`load_governed_schema_with_fields()`；字段目录 API 位于
`/api/v2/dispatch/resources/ontology-field-overlays`，写操作要求 `dispatch:manage`。
受控写真写只经提案队列 + `DomainActionExecutor`。`Flight.change_stand` /
`Stand.reserve` / `Todo.create` 已废止，fail-closed。金样见
`docs/fixtures/flight_ops_v1_ontology_schema.json`。

---

## 1. 设计原则

1. **飞机中心**：资源占用主体是 `registration`（原样存储、全局唯一），不是机位标量。
2. **任务与飞机分离**：方向航班（Flight）可换机；换机后周转链按同机健康性维护。
3. **冲突软约束**：机位时段重叠只告警，不硬拦。
4. **分权**：AOC / TOC / GROUND 岗位权限模板（见 migration 119）。
5. **占用回写展示列**：机位/口/转盘生效后回写航班 `stand` / `gate` / `terminal` / `baggage_carousel` 作只读展示，不是计划真相。`ResourceAdjustmentSuggestion` 接受 = 对应 allocate，不是一等对象。

---

## 2. 核心对象

| 对象 | 主键 | 说明 |
|------|------|------|
| Terminal | `terminal_id` | 航站楼目录；成员事实在 `terminal_stands` / `terminal_gates` / `terminal_carousels` |
| Stand | `stand_id` | 机位目录；必须挂启用的 Terminal |
| Gate | `gate_id` | 登机口目录；必须挂启用的 Terminal |
| BaggageCarousel | `carousel_id` | 行李转盘目录；必须挂启用的 Terminal |
| StandOccupation | `occupation_id` | 飞机（registration）+ 时段 + 机位；重叠仅软告警 |
| GateAssignment | `assignment_id` | 航班（flight_id）+ 时段 + 登机口；重叠仅软告警 |
| CarouselAssignment | `assignment_id` | 航班（flight_id）+ 时段 + 转盘；无数量/重叠约束 |
| Aircraft | `registration` | 飞机；首次占用时 upsert |
| TurnaroundLink | `id` | 入港 Flight ↔ 出港 Flight 的保障周转链；`active` \| `broken`。不是旅客衔接，也不是 Flight |
| Flight | `flight_id` | 一班进港 **或** 一班出港。`direction` 只能 `inbound` \| `outbound`，禁止 `both`。`stand`/`gate`/`terminal`/`baggage_carousel` 由占用回写 |
| Department | `department_id` | 科室目录 |
| Team | `team_id` | 班组名册；挂科室，不是工单 assignee |
| Equipment | `equipment_id` | 保障设备；挂科室 + EquipmentType |
| EquipmentType | `equipment_type_id` | 设备种类；`requires_driver` 表达司机需求 |
| Personnel | `user_id` | 仅个人账号的作业身份；岗位账号不生成 Personnel |
| Qualification | `qualification_id` | 科室资质目录；发放在人员管理页 |
| TaskType | `task_type_id` | 作业类型目录；`anchor` 为 `inbound` \| `outbound` \| `link` |
| DispatchOrder | `dispatch_order_id` | 按 `TaskType.anchor` 挂进港 Flight、出港 Flight 或周转链；按命名槽指派人员/设备 |
| Anomaly | `anomaly_id` | 运行信号；主体是 `subject_type` + `subject_id`（`flight_id` 可空） |
| BusinessCase | `business_case_id` | 事项/案件；流程是属性而非独立 Workflow 对象 |

监控行 `flight_monitor_rows` **不是**本体对象。热列表一格一行、进出港是列；详情再按 `inbound_flight_id` / `outbound_flight_id` 读写真。`row_id` 写入后不因建链/拆链改变。

码表（`metadata_catalogs`）也不是一等对象；`EquipmentType` / `Qualification` / `TaskType` 仍是合同对象，不搬进通用码表。

---

## 3. 不变量（摘录）

| # | 规则 | 实现位置 |
|---|------|----------|
| 1 | registration 原样唯一 | DB + `upsert_aircraft` |
| 4 | 链接健康需两端同机 | `ontology_v1_rules::enforce_link_health` |
| 5 | draft 不可被正式占用引用 | allocate_stand / accept stand |
| 6 | 进港前站起飞 / 出港登机后禁止换机 | `reassign_gate_violation` |
| 9 | 禁 AOC+TOC 双岗 | 规则纯函数；岗位分配侧 enforce |
| 10 | 地服黑名单：改机号/正式位/正式口 | 权限码 + 路由 |
| 12 | 机位建议仅 AOC 可接受；口建议仅 TOC | `accept_permission_for` |
| 13 | `Flight.direction` 只能 inbound/outbound | `Flight::validate_direction_contract`；迁移 `155` |
| 14 | 热列表/搜索/计数只读 `flight_monitor_rows` | `FlightService::list_flights` / `search`；仓储 SQL 不得 JOIN `flights`/`flight_legs` |
| 15 | overlay 不能改代码核心字段类型 | `load_governed_schema_with_fields()`；`attribute_validation` |

---

## 4. HTTP API（`/api/v2/ontology`）

与 `/api/v2/ai/ontology`（AI schema 快照）分离。

| 方法 | 路径 | 权限 |
|------|------|------|
| POST | `/aircraft/reassign` | `ontology.aircraft.reassign` |
| POST | `/stands/occupations` | `ontology.stand.manage` |
| PATCH | `/stands/occupations/{id}` | `ontology.stand.manage` |
| POST | `/stands/occupations/{id}/release` | `ontology.stand.manage` |
| POST | `/gates/assignments` | `ontology.gate.manage` |
| PATCH | `/gates/assignments/{id}` | `ontology.gate.manage` |
| POST | `/gates/assignments/{id}/release` | `ontology.gate.manage` |
| POST | `/turnaround-links` | reassign / stand.manage / plan.confirm |
| POST | `/turnaround-links/{id}/break` | 同上 |
| POST | `/turnaround-links/auto-scan` | `ontology.plan.confirm` |
| GET/POST | `/suggestions` | read / create |
| POST | `/suggestions/{id}/accept\|reject` | accept_* / reject |
| POST | `/flights/confirm-drafts` | `ontology.plan.confirm` |
| GET | `/flights/{id}/resources` | `ontology.read` |
| GET | `/aircraft/{reg}/resources` | `ontology.read` |

---

## 5. 自动建链

- **触发**：`POST .../turnaround-links/auto-scan`，或后台扫描器。
- **环境变量**：
  - `ONTOLOGY_AUTOLINK_SCANNER_ENABLED=true` 开启后台扫描
  - `ONTOLOGY_AUTOLINK_SCAN_INTERVAL_SECONDS`（默认 300）
  - `ONTOLOGY_AUTOLINK_WINDOW_MINUTES`（默认 360）
  - `ONTOLOGY_AUTOLINK_SCAN_LIMIT`（默认 100）
- **规则**：同机号；出港未起飞且无 active 出港链；进港已落地且时间窗内；进港端亦无 active 入链。

---

## 6. 代码地图

```
migrations/119_ontology_v1_core.sql
migrations/144_create_metadata_catalogs.sql
migrations/145_create_ontology_field_overlays.sql
migrations/147_create_flight_monitor_rows.sql
migrations/151_add_task_type_anchor.sql
migrations/152_create_ontology_attribute_references.sql
migrations/155_split_flight_identity.sql
domain/models/ontology_v1.rs
domain/models/ontology_v1_rules.rs
domain/ontology/flight_ops_v1.rs
domain/ports/ontology_repository.rs
infrastructure/repositories/pg_ontology_repository.rs
application/schemas/ontology_schemas.rs
application/services/ontology_service/          运行资源写
application/services/ontology_actions/          AI 只读 / 建议（每动作一个服务）
application/services/domain_action_executor/    受控写 → 领域服务
application/services/metadata_catalog_service.rs
application/services/field_overlay_service.rs
application/services/flight_monitor_row_service.rs
api/routes/ontology.rs                          /api/v2/ontology
api/routes/ai_ontology.rs                       /api/v2/ai/ontology
api/routes/dispatch_resources/metadata_catalogs.rs
api/routes/dispatch_resources/field_overlays.rs
api/routes/flights/monitor_rows.rs
server/di/flight.rs                             装配 OntologyService + OntologyActionServices
```

只读动作服务：`FlightContextService`、`FlightSearchService`、`DispatchStatusService`、
`AnomalyOpenListService`、`StandAvailabilityService`、`BriefingService`。

建议动作服务：`StandRecommendationService`、`DispatchReplanAdvisorService`、
`AnomalyEscalationAdvisorService`、`DelayAdvisorService`。
`notification.suggest_broadcast` 已随 Notification 退出合同，不再作为信封动作。

机位/口/转盘受控写是占用三对象上的 `allocate` / `adjust` / `release`。`Flight.change_stand` 与 `Stand.reserve` 不在 schema 中，执行器 fail-closed。

| 动作 | 服务 |
|---|---|
| `flight.get_context` | `FlightContextService` |
| `flight.search` | `FlightSearchService` |
| `dispatch.get_status` | `DispatchStatusService` |
| `anomaly.list_open` | `AnomalyOpenListService` |
| `stand.check_availability` | `StandAvailabilityService` |
| `report.generate_briefing` | `BriefingService` |
| `flight.suggest_stand_adjustment` | `StandRecommendationService` |
| `dispatch.suggest_replan` | `DispatchReplanAdvisorService` |
| `anomaly.suggest_escalation` | `AnomalyEscalationAdvisorService` |
| `flight.suggest_delay_action` | `DelayAdvisorService` |
| `Flight.add_note` / `Flight.update_delay` / 占用 allocate·release / 派工槽位 / 其它受控写 | `DomainActionExecutor` → 既有领域服务 |

---

## 7. 完成度与运维

| 项 | 状态 |
|----|------|
| 定义层 migration/models/repos | ✅ |
| Reassign / draft / 双视图 | ✅ |
| 机位/口 Allocate·Adjust·Release | ✅ |
| 建议 create/accept/reject + 接受落正式资源 | ✅ |
| 周转链接手工/自动/扫描器 | ✅ |
| 域事件驱动建链 | ✅（status/resource/leg 更新） |
| 集成测试 | ✅（`ontology_v1_integration`，需 `TEST_DATABASE_URL`） |
| 前端资源台 | ✅ `/frontend/ontology_center.html`（工作区模块「本体」） |
| 前端 AAR（Adjust/Release） | ✅ 占用/分配列表 + PATCH/release |
| 航班详情深链 | ✅ `?flight=` / `?registration=` / `?tab=` + 详情「本体资源」 |
| Playwright e2e | ✅ `e2e/ontology_center.spec.ts` |

### 集成测试

```powershell
# 推荐：从 .env 读凭证，目标库 flight_monitor_test（不会打到 dev）
.\scripts\dev\run_ontology_v1_db_tests.ps1

# 或手动：
# $env:TEST_DATABASE_URL = "postgres://USER:PASS@localhost:5432/flight_monitor_test"
# 确保已执行 migrations/119_ontology_v1_core.sql
# cargo test -p fms-application --test ontology_v1_integration -- --ignored --nocapture
```

本地验收（宿主机 psql，`flight_monitor_test` + migration 119）：**6/6 通过**
（reassign / allocate+accept / auto-link / stand·gate AAR / draft+reject / link create·break）。

---

## 8. AI 治理（工具 ≠ 动作；提案 ≠ pending）

`flight-ops.v1` 只含**本体动作**（`Object.action`，如 `Flight.add_note`、`StandOccupation.allocate`、
`DispatchOrder.assign_slot`）。工具（`ontology.lookup` / `ontology.propose_action`）是执行**适配器**，有
固定角色（内部只读 / proposal_only），**不是**本体动作，不进合同、不作为独立动作登记。不要把
`MCP send`、`Notification.send` 等塞进 `flight-ops.v1`。

受控写的唯一落点是**提案**：`ontology.propose_action` 只生成 `ai_action_proposals`（或 structured output
的 `proposals[]`），必须经 `/api/v2/ai/proposals/{id}/approve` + `/execute` 走 `DomainActionExecutor` 才真写
业务表 + outbox。**pending-action 假执行**（`ai_pending_actions` / 聊天卡）对 `ontology.*` 不落业务写；
「批了 pending 就算已占用 / 已代签」是错的。

治理字段（启用 / 风险 / 审批）只来自一份真相：`build_flight_ops_v1_schema()`（代码底）叠加
`aip_ontology_actions` 覆盖。`load_governed_schema()` 是唯一能拿到完整 `OntologySchema` 的入口；
配置中心通过 `PUT/DELETE /api/v2/ai/ontology/actions/overlay`（需 `ai:manage`，只认代码 schema 已知键）
改启用 / 风险 / 审批，`generate` 与导出读同一函数，故配置中心改了审批、新提案立即同源生效。

### 8.1 字段 overlay 与任务锚点

对象字段可通过 `ontology_field_overlays` 扩展，但不能覆盖代码合同字段的类型；运行时 schema、AI
校验和资源表单都只读取启用中的 overlay。`catalog_ref` 字段由码表服务提供选项，`object_ref` 由对象
目录提供候选，停用字段不会进入写入校验。实例值进该对象行的 `attributes` JSONB；未知 key 或类型不对 → 400。
`object_ref` 是业务外键（无物理 FK）：目标必须存在且未停用；停用/改码若仍被引用 → 409。

码表种子含封闭有序的 `icao_size`（A–F）和开放的 `aircraft_type`（报文可 ingest upsert）。
本期不扩 `anomaly_rules`，不把机型超限或组合机位占用写进规则引擎。

`TaskType.anchor` 是作业绑定的业务锚点，取值严格为 `inbound`、`outbound` 或 `link`，与用于计算计划
时间的 `generation_anchor_type` 不同。生成派工单时，`leg_scope` 必须与任务类型 anchor 一致；旧分类
（arrival/departure/turnaround）仅在迁移和创建请求缺省时映射到上述三值。
