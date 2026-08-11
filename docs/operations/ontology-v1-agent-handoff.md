# Ontology V1 Agent Handoff

> 更新时间：2026-08-12
> 当前分支：`feat/ontology-definition`
> 交接目标：指导后续代理继续完成 `docs/plans/2026-05-11-ontology-v1-contract.md`，并维护已经完成的飞机中心运行资源子集。

## 1. 先读这些文件

按以下顺序阅读，避免把两个不同范围的 Ontology 混为一谈：

1. [Ontology V1 Contract](../plans/2026-05-11-ontology-v1-contract.md)：原始完整 AI Ontology 契约，定义对象、动作、风险、审批、schema 和验收标准。
2. [Ontology V1 Architecture](../architecture/ONTOLOGY_V1.md)：当前已落地的飞机中心运行资源子集。
3. [OntologyService](../../services/api-server/crates/application/src/services/ontology_service/service.rs)：当前运行资源写路径和自动建链逻辑。
4. [Ontology API routes](../../services/api-server/crates/api/src/routes/ontology.rs)：`/api/v2/ontology` HTTP 边界和权限入口。
5. [Flight Ops schema](../../services/api-server/crates/domain/src/ontology/flight_ops_v1.rs)：当前 AI schema 的静态 fallback，不要把它误认为已经覆盖原始契约的全部动作。
6. [Ontology DB integration tests](../../services/api-server/crates/application/tests/ontology_v1_integration.rs)：数据库验收样例和测试数据清理方式。

## 2. 当前状态

当前实现是“飞机中心 Ontology V1”，不是完整 AI Ontology V1。已经落地的对象和能力如下：

| 范围 | 当前状态 |
|---|---|
| `Aircraft`、机号原样唯一 | 已实现 |
| `StandOccupation` Allocate / Adjust / Release | 已实现 |
| `GateAssignment` Allocate / Adjust / Release | 已实现 |
| `TurnaroundLink` 手工建链 / 拆链 / 自动扫描 | 已实现 |
| `ResourceAdjustmentSuggestion` create / accept / reject / expire | 已实现 |
| Flight draft 批量确认 | 已实现 |
| Flight / Aircraft 双资源视图 | 已实现 |
| 域事件驱动自动建链 | 已实现 |
| Ontology Center 前端、深链、AAR | 已实现 |
| 原始契约中的完整 AI read/advisory/write 动作 | 已实现 flight-ops.v1 子集（6 只读 / 5 建议 / 10 受控写，Phase 1–4） |
| 原始契约要求的完整导出 schema 格式 | 已实现（契约 §7 信封，Phase 0） |

架构文档已经明确记录了这个范围边界。后续代理不能把当前资源台的完成状态直接当成完整 AI 契约完成状态：Phase 0–5 落地的是 `flight-ops.v1` 子集，原始契约中子集之外的对象/动作仍未实现。

## 3. 当前工作树中的修正

本次交接前已经完成以下安全和一致性修正。后续代理不得覆盖这些改动：

- 接受建议时，权限和操作者身份强制来自 JWT；请求体中的 `actor_permissions`、`accepted_by` 不再可信。
- reject、draft confirm、资源释放、建议创建、周转链接创建/拆除的审计身份统一由 JWT 注入。
- Stand/Gate Adjust 的资源更新、航班计划同步、outbox 写入进入同一事务。
- 事务更新如果发现目标已被并发修改，会返回 `ConcurrencyConflict`，不会静默成功。
- AOC/TOC 双岗互斥接入角色分配路径，并使用按用户维度的 PostgreSQL advisory transaction lock。
- Ontology DB 集成测试在缺少数据库时明确失败，不再 silently skip。
- 自动建链集成测试不再通过 `try_auto_link_outbound` 兜底，必须证明扫描器本身创建链接。

这些修改当前尚未提交，工作树中可能还有 `test-results/` 未跟踪目录。后续代理开始前先运行：

```powershell
git status --short --branch
git diff --check
```

不要重置或覆盖其他代理的修改。

## 4. 原始契约剩余工作

原始契约的完整验收标准位于 [Contract §8](../plans/2026-05-11-ontology-v1-contract.md)。推荐按以下阶段推进，每个阶段单独提交、单独测试。

### Phase 0：固定契约和 schema 版本（已完成，ed2eefc）

目标：先解决当前 schema 命名和导出契约不一致，避免后续代理各自发明格式。

任务：

- 统一 `ontology_version`、`correlation_id`、`object_type`、`object_id`、`action_name`、`arguments`、`risk_level`、`required_permissions`、`approval_policy`。
- 明确 `flight-ops.v1` 与飞机中心资源 Ontology 的版本命名，不能混用裸 `v1.0`、`flight-ops.v1` 和数据库自定义版本。
- 将 `/api/v2/ai/ontology/schema` 的响应补齐为契约要求的稳定结构，或正式更新契约文档为当前结构；两者必须选一个，不能只改文档标题。
- 确定 `exported_at`、顶层 `actions`、`risk_policies`、`constraints` 的来源和序列化规则。

验收：固定 schema fixture，Rust 和 Python sidecar 都能解析同一份 JSON；增加 schema contract test。

### Phase 1：只读动作（已完成，02fb721）

需要落地并接入 schema 的动作：

| 动作 | 推荐复用入口 | 关键要求 |
|---|---|---|
| `flight.get_context` | `FlightRepository.find_by_id` | 返回 `evidence` 和 ontology version |
| `flight.search` | `FlightRepository.search` / `FlightService.search_flights` | limit 最大 200，带 query evidence |
| `dispatch.get_status` | `DispatchOrderRepository.find_by_id` | 返回团队、设备、冲突摘要 |
| `anomaly.list_open` | `AnomalyRepository.find_by_status` | 支持 severity、flight、limit 和汇总 |
| `stand.check_availability` | Stand/Flight repositories | 只读计算冲突和替代建议 |
| `report.generate_briefing` | 现有 Dashboard/Workbench 查询服务 | 明确数据缺口和 confidence |

只读动作不创建 pending action，不允许 Python 或 LLM 直接 SQL。每个 response 都必须带证据来源和检索时间。

### Phase 2：建议动作（已完成，695c784）

计划中明确标记为缺失、需要新增 advisor/service 的动作：

| 动作 | 建议服务 | 输出 |
|---|---|---|
| `flight.suggest_stand_adjustment` | `StandRecommendationService` | proposal、before/after preview、constraint results |
| `dispatch.suggest_replan` | `DispatchReplanAdvisorService` | proposal、资源变化、分数变化、冲突 |
| `anomaly.suggest_escalation` | `AnomalyEscalationAdvisorService` | proposal、升级类型、目标通知/事项 |
| `flight.suggest_delay_action` | `DelayAdvisorService` | 延误处置 proposal 和相关派工动作 |
| `notification.suggest_broadcast` | Notification advisor / prepare method | 只生成建议，不发送副作用 |

建议动作不能直接写业务表。统一经过现有 proposal/pending-action/approval 管线，并补齐 `risk_level`、`approval_policy`、`constraint_results`、`before_snapshot`、`after_preview`。

### Phase 3：受控写动作（已完成，13ccee2）

需要逐项确认已有 Rust service 是否能承载 Ontology 契约；不能只在 schema 中声明 execution mapping：

| 动作 | 目标服务 |
|---|---|
| `flight.update_stand` | `FlightService` / `FlightRepository.update_partial` |
| `flight.update_delay` | `FlightService` / `FlightRepository.update_partial` |
| `dispatch.update_status` | `DispatchService` / dispatch repository |
| `dispatch.reassign` | `DispatchService.reassign` |
| `dispatch.publish` | `DispatchService.publish` |
| `anomaly.acknowledge` | `AnomalyService.acknowledge` |
| `anomaly.resolve` | `AnomalyService.resolve` |
| `notification.send` | `NotificationService` |
| `label.add` | 新增 `LabelService.add_to_flight` |
| `workflow.start` | Flowable service/gateway |

每个写动作都必须满足：

- Rust application service 是唯一业务写入口。
- 执行前重新校验权限、对象版本、约束、审批状态和幂等键。
- 写动作携带 `before_snapshot` 和 `after_preview`。
- 事务内写业务数据和 outbox；失败时不能留下半提交状态。
- 写入 correlation ID、ontology version 和 actor 审计信息。

### Phase 4：AI proposal / execution wiring（已完成，57afc9c）

目标是让 schema 中的动作真的能被 AI proposal 和执行管线消费：

- 将 Ontology action 名称映射到 `DomainActionExecutor` 或现有 action proposal service。
- 禁止 AI proposal 绕过资源权限和 DTO 校验。
- 继续保持 `ReassignAircraft` 等高风险动作的显式阻断/审批策略。
- 为每个 write action 增加拒绝、版本冲突、重复执行、过期 proposal 和无权限用例。
- Python sidecar 只消费 schema 和受控 API，不复制 Rust 业务规则。

### Phase 5：完整验收和文档回写（已完成，见本节末尾验收记录）

只有满足以下条件，才能把原始契约的验收清单从 `[ ]` 改成 `[x]`：

- 所有 read/advisory/controlled-write 动作都有代码入口和测试。
- 所有动作都有可解析 JSON schema、风险等级、审批策略和权限。
- 所有写 proposal 都有约束结果、before/after snapshot、correlation ID、ontology version。
- schema endpoint 与契约文档字段完全一致。
- Python sidecar 的 schema mirror/live smoke 使用真实版本字段。
- DB、Rust unit、API route、frontend/E2E 测试全部通过。
- 文档中的状态只描述实际验证过的范围。

Phase 5 验收记录（2026-08-12，数据库 `flight_monitor_test`）：

- Rust：`fms-application ontology` 36、`fms-domain ontology` 20、`fms-api ontology` 3+1、
  `fms-application ai_action_proposal` 16+3（staging smoke）、`fms-api ai_proposals` 9+7、
  `fms-application domain_action_executor -- --ignored` 37 全部通过；`cargo check -p fms-server` 通过。
- DB 集成：`.\scripts\dev\run_ontology_v1_db_tests.ps1` 6/6 通过。
- Python sidecar：`test_ontology_schema_fixture` + `test_schema_mirror_url_guard` 8 通过；
  schema mirror 与 envelope 均使用真实版本字段 `flight-ops.v1`。
- 前端：`npm run typecheck` 通过；定向单测 19/19；`e2e/ontology_center.spec.ts` 12/12。
- 契约 §8 验收清单已按上述证据打勾，验收范围限定为 flight-ops.v1 子集。

## 5. 当前飞机中心实现的维护规则

后续代理扩展现有 `/api/v2/ontology` 时，遵守以下边界：

1. `Aircraft.registration` 原样存储，不能自动大写、去空格或重新格式化。
2. `StandOccupation` 和 `GateAssignment` 是时段关系对象，不能退回到只写 `flights.stand` / `flights.gate` 的标量模型。
3. 机位时段重叠是 warning，不得变成未经产品确认的硬拒绝。
4. draft 航班不可被正式机位占用引用。
5. 周转链接 active 的前提是两端当前机号一致；换机后必须拆链或重新建链。
6. AOC、TOC、GROUND 权限必须在路由和 service 两层校验；不能只依赖前端按钮隐藏。
7. HTTP 请求体中的权限、角色、操作者身份只能作为显示/兼容字段，不能作为授权依据。
8. Adjust 类操作必须复用事务仓储，不要重新引入“先更新资源、后更新航班”的双提交路径。
9. 自动扫描测试必须验证扫描器本身的结果，不能用手工 helper 兜底。

## 6. 测试命令

### Rust 单元和编译

```powershell
cd C:\flight-monitor-system\services\api-server
cargo check -p fms-server
cargo test -p fms-application ontology_service --lib
cargo test -p fms-domain ontology_v1 --lib
cargo test -p fms-api ontology --lib
```

### PostgreSQL 集成测试

推荐使用项目脚本。脚本会拒绝 `flight_monitor_dev`，读取根目录 `.env`，检查 migration 119 的关系，并在数据库不可用时失败：

```powershell
cd C:\flight-monitor-system
.\scripts\dev\run_ontology_v1_db_tests.ps1
```

不要直接运行 ignored integration test 期待它自动跳过环境缺失；当前测试已改为缺少 `TEST_DATABASE_URL` 时明确失败。

### 前端

```powershell
cd C:\flight-monitor-system\frontend\vue-app
npm run typecheck
npm run test -- src/pages/ontology_center/ontologyApi.test.ts src/shared/page-routes.test.ts src/legacy/pageParityMatrix.test.ts
npm run test:e2e -- e2e/ontology_center.spec.ts
```

## 7. 交接协议

后续代理接手时，必须在回复或提交说明中记录：

- 当前实现的是哪一个 Phase，不要笼统写“Ontology 完成”。
- 修改了哪些 schema、service、repository、route 和测试。
- 哪些动作仍未实现，哪些只是 execution mapping 或文档计划。
- 实际运行的测试命令和结果；不能只引用历史的“6/6 通过”。
- 如果发现契约和实现冲突，先更新契约决策记录，再写代码。

建议每个 Phase 使用独立提交，提交信息示例：

```text
feat(ontology): implement flight.search read action
test(ontology): add dispatch status contract coverage
docs(ontology): update v1 schema export contract
```

## 8. 当前交接结论

Ontology V1 计划的 Phase 0–5 已全部完成并提交（ed2eefc / 02fb721 / 695c784 / 13ccee2 / 57afc9c）：
`flight-ops.v1` 子集的 6 个只读、5 个建议、10 个受控写动作均有代码入口、测试、schema、
风险/审批/权限定义；AI proposal 与执行管线已接入 `DomainActionExecutor`，风险与权限以
ontology schema 为单一事实来源；契约 §8 验收清单已打勾。后续工作的主线不再是补齐本子集，
而是按需扩展原始契约中子集之外的对象/动作，或维护现有资源台边界（见第 5 节）。
