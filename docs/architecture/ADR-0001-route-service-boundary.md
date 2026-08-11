# ADR-0001: API 路由层禁止直接依赖 Repository

## 状态
已接受（2026-03-04）

## 背景
当前代码库在多个业务域存在 `route -> repository` 直接耦合，导致：

- 领域规则旁路，无法统一校验与审计。
- 路由层承担事务与状态机职责，测试与演进成本上升。
- DDD/CQRS 迁移过程无法增量推进。

## 决策
1. 新增路由必须依赖应用服务（Command/Query Service），不得直接注入 repository。
2. 对历史存量违规采用“基线冻结 + 增量收敛”策略：
   - 允许存量违规保留在基线文件。
   - 禁止新增违规（CI/测试守卫）。
3. Flight 相关路由维持零「路由直连仓储」红线（历史 Python 为 `flight_routes.py`；现为 Rust `crates/api/src/routes/flights/**`）。

## 影响
1. 新增开发需要先补服务层接口，再开放路由能力。
2. 存量路由迁移以业务域为单位，逐冲刺替换。
3. 架构边界应通过看板、守门测试与（可选）Rust 静态审计持续验证。

## 实施

**当前（Rust API，2026-06-29）：**

- 决策与现状看板：`docs/architecture/TECH_DEBT_DASHBOARD.md`
- 手工已核对：`crates/api/src/routes` 生产代码无 `sqlx::query`（宜后续固化为 CI 测试）
- 可选后续：Rust 静态审计「`fms-api` routes 不直接依赖 infrastructure repositories」

**历史（Python API，已退役）：**

以下资产已从仓库移除，仅作 ADR 历史记录：

- ~~审计脚本：`scripts/tools/audit_route_repository_dependencies.py`~~
- ~~基线文件：`docs/architecture/route_repository_dependency_baseline.json`~~
- ~~测试守卫：`tests/application/test_route_repository_dependency_audit.py`~~
