# Ontology V1 — 飞机中心运行本体

> 状态：实现中（`feat/ontology-definition`）  
> 范围：机号权威、机位占用、登机口分配、周转链接、资源建议、draft 批确认

本文档与代码对齐：`migrations/119_ontology_v1_core.sql`、
`domain/models/ontology_v1*.rs`、`application/services/ontology_service`、
`/api/v2/ontology/*`。

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
domain/ports/ontology_repository.rs
infrastructure/repositories/pg_ontology_repository.rs
application/schemas/ontology_schemas.rs
application/services/ontology_service/
api/routes/ontology.rs
server/di/flight.rs  (装配 OntologyService)
```

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

### 集成测试

```powershell
$env:TEST_DATABASE_URL = "postgres://USER:PASS@localhost:5432/flight_monitor_test"
# 确保已执行 migrations 含 119
cargo test -p fms-application --test ontology_v1_integration -- --ignored --nocapture
```
