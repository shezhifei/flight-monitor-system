# Ontology V1 — 当前范围

飞机中心运行资源与 `flight-ops.v1` AI 动作共用一套对象模型。
运行台走 `/api/v2/ontology`，AI 走 `/api/v2/ai/ontology`。

## 1. 源码入口

1. [Ontology V1 Architecture](../architecture/ONTOLOGY_V1.md)：对象、HTTP、动作服务。
2. [Ontology V1 Contract](../plans/2026-05-11-ontology-v1-contract.md)：历史完整动作清单与规则。
3. [OntologyService](../../services/api-server/crates/application/src/services/ontology_service/service.rs)：运行资源写路径和自动建链。
4. [Ontology actions](../../services/api-server/crates/application/src/services/ontology_actions/)：每条只读/建议动作一个服务。
5. [DomainActionExecutor](../../services/api-server/crates/application/src/services/domain_action_executor/service.rs)：受控写落到既有领域服务。
6. [Ontology API routes](../../services/api-server/crates/api/src/routes/ontology.rs)：`/api/v2/ontology`。
7. [AI ontology routes](../../services/api-server/crates/api/src/routes/ai_ontology.rs)：schema 与动作执行。
8. [Flight Ops schema](../../services/api-server/crates/domain/src/ontology/flight_ops_v1.rs)：静态 schema。
9. [Ontology DB integration tests](../../services/api-server/crates/application/tests/ontology_v1_integration.rs)。

## 2. 当前能力

| 范围 | 状态 |
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
| `flight-ops.v1` 只读 / 建议 / 受控写 | 已实现 |
| schema 信封 | 已实现 |

授权与一致性约定：

- 权限和操作者身份来自 JWT；请求体中的 `actor_permissions`、`accepted_by` 不可信。
- Stand/Gate Adjust 的资源更新、航班计划同步、outbox 写入在同一事务中。
- 并发修改返回 `ConcurrencyConflict`，不会静默成功。
- AOC/TOC 双岗互斥走角色分配路径，并使用按用户维度的 PostgreSQL advisory lock。
- Ontology DB 集成测试在缺少数据库时明确失败。

## 3. 扩展

动作表见 [ONTOLOGY_V1 §6](../architecture/ONTOLOGY_V1.md)。新增动作时增加命名服务，HTTP 加一支选型，不要再加 `execute()` 门面。

整仓剩余补丁与约束见 [agent-handoff](agent-handoff.md)。
历史草稿契约不要当活法：[ontology-v1-contract](../plans/2026-05-11-ontology-v1-contract.md)。

## 4. 维护规则

扩展 `/api/v2/ontology` 时遵守：

1. `Aircraft.registration` 原样存储，不能自动大写、去空格或重新格式化。
2. `StandOccupation` 和 `GateAssignment` 是时段关系对象，不能退回到只写 `flights.stand` / `flights.gate` 的标量模型。
3. 机位时段重叠是 warning，不得变成未经产品确认的硬拒绝。
4. draft 航班不可被正式机位占用引用。
5. 周转链接 active 的前提是两端当前机号一致；换机后必须拆链或重新建链。
6. AOC、TOC、GROUND 权限必须在路由和 service 两层校验；不能只依赖前端按钮隐藏。
7. HTTP 请求体中的权限、角色、操作者身份只能作为显示字段，不能作为授权依据。
8. Adjust 类操作必须复用事务仓储，不要拆成“先更新资源、后更新航班”的双提交。
9. 自动扫描测试必须验证扫描器本身的结果，不能用手工 helper 兜底。

## 5. 测试命令

### Rust 单元和编译

```powershell
cd C:\flight-monitor-system\services\api-server
cargo check -p fms-server
cargo test -p fms-application ontology_actions --lib
cargo test -p fms-application ontology_service --lib
cargo test -p fms-domain ontology_v1 --lib
cargo test -p fms-api ontology --lib
```

### PostgreSQL 集成测试

脚本会拒绝 `flight_monitor_dev`，读取根目录 `.env`，检查 migration 119 的关系，并在数据库不可用时失败：

```powershell
cd C:\flight-monitor-system
.\scripts\dev\run_ontology_v1_db_tests.ps1
```

缺少 `TEST_DATABASE_URL` 时集成测试明确失败，不会静默跳过。

### 前端

```powershell
cd C:\flight-monitor-system\frontend\vue-app
npm run typecheck
npm run test -- src/pages/ontology_center/ontologyApi.test.ts src/shared/page-routes.test.ts src/legacy/pageParityMatrix.test.ts
npm run test:e2e -- e2e/ontology_center.spec.ts
```
