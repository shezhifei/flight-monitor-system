# Ontology V1 — 飞机中心运行本体

Ontology 是一套对象和动作：资源对象由运行台写入，AI 通过同一套对象上的
只读 / 建议 / 受控写动作访问。两个 HTTP 面共用这一模型，不是两套本体。

- 运行资源：`/api/v2/ontology` → `OntologyService`
- AI 协议：`/api/v2/ai/ontology` → 命名动作服务 + `DomainActionExecutor`

`flight-ops.v1` 当前动作面：6 只读 / 5 建议 / 受控写（机位写作为
`Flight.change_stand`）。历史完整动作清单见
[ontology-v1-contract](../plans/2026-05-11-ontology-v1-contract.md)。

---

## 1. 设计原则

1. **飞机中心**：资源占用主体是 `registration`（原样存储、全局唯一），不是机位标量。
2. **任务与飞机分离**：航段（Flight）可换机；换机后周转链接按同机健康性维护。
3. **冲突软约束**：机位时段重叠只告警，不硬拦。
4. **分权**：AOC / TOC / GROUND 岗位权限模板（见 migration 119）。
5. **建议接受即执行**：`ResourceAdjustmentSuggestion` 接受后回写 Flight 计划字段。

---

## 2. 核心对象

| 对象 | 主键 | 说明 |
|------|------|------|
| Aircraft | `registration` | 飞机；首次写入 upsert |
| Flight | `flight_id` | 增加 `direction` / `flight_kind` / `is_draft` / `divert` |
| StandOccupation | `id` | 飞机+时段+机位；`normal` \| `moving` |
| GateAssignment | `id` | 飞机+时段+登机口 |
| TurnaroundLink | `id` | 进港航段 ↔ 出港航段；`active` \| `broken` |
| ResourceAdjustmentSuggestion | `id` | 机位/口建议；pending → accepted_executed \| rejected \| expired |

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
domain/models/ontology_v1.rs
domain/models/ontology_v1_rules.rs
domain/ontology/flight_ops_v1.rs
domain/ports/ontology_repository.rs
infrastructure/repositories/pg_ontology_repository.rs
application/schemas/ontology_schemas.rs
application/services/ontology_service/          运行资源写
application/services/ontology_actions/          AI 只读 / 建议（每动作一个服务）
application/services/domain_action_executor/    受控写 → 领域服务
api/routes/ontology.rs                          /api/v2/ontology
api/routes/ai_ontology.rs                       /api/v2/ai/ontology
server/di/flight.rs                             装配 OntologyService + OntologyActionServices
```

只读动作服务：`FlightContextService`、`FlightSearchService`、`DispatchStatusService`、
`AnomalyOpenListService`、`StandAvailabilityService`、`BriefingService`。

建议动作服务：`StandRecommendationService`、`DispatchReplanAdvisorService`、
`AnomalyEscalationAdvisorService`、`DelayAdvisorService`、
`NotificationBroadcastAdvisorService`。

机位受控写只有 `Flight.change_stand`。旧 `Flight.update_stand` 不在 schema 中，执行器也拒绝该动作名。

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
| `notification.suggest_broadcast` | `NotificationBroadcastAdvisorService` |
| `Flight.change_stand` / `Flight.update_delay` / 其它受控写 | `DomainActionExecutor` → 既有领域服务 |

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
