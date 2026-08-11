# 技术债看板（架构边界）

更新：**2026-08-11**（与文档基线对齐；指标以仓库内测试与代码为准）。

主 API：**Rust** `services/api-server`（`fms-api` / `fms-application` / `fms-infrastructure`）。  
AI：**Python** `services/ai-sidecar`，经 HTTP 与 Rust 协作。不再使用已移除的 `*_routes.py` 单体路由。

相关文档：

- [架构改进路线图](ARCHITECTURE_IMPROVEMENT_ROADMAP.md)
- [技术债清扫主计划](../plans/2026-06-29-tech-debt-sweep-master-plan.md)（本地/白名单计划稿）

---

## 当前指标

| 指标 | 状态 | 说明 |
|------|------|------|
| 路由层内联 SQL | **测试守门** | `services/api-server/crates/api/tests/layer_boundary_guard.rs` 禁止 routes 生产代码新增 `sqlx::query*` |
| 路由层直连 Repository | **测试守门** | 同上，禁止 routes 生产代码新增 `fms_infrastructure::repositories::*` |
| `api` crate 依赖 infrastructure | **已清理并守门** | 断言生产依赖不含 `fms-infrastructure`、`sqlx`、`redis`；具体 wiring 在 `fms-server` |
| Application 直连 SQL | **存量债（3 个 tests.rs）** | `application_boundary_inventory.rs` 固定清单，条目只允许减少；剩余为 `#[cfg(test)]` 测试桩 |
| Flight 域路由红线 | **意图保持** | [ADR-0001](ADR-0001-route-service-boundary.md)：路由走应用服务，不绕过领域规则 |
| 超大路由文件 | **部分债** | `auth/login`、`dispatch/create_order` 已拆分；仍有 ≥400 行文件（如部分 flowable 生成逻辑） |
| `legacy_compat.rs` | **已删除** | 生产 `web.rs` 从未挂载；路由由 `dispatch.rs` 等接管 |
| Sidecar 写核心域表 | **运行时无违规** | 侧车 SQL 写限于控制面表；业务写走 Rust / 内部 API。旧 AIP handler / 归档 gateway 不得假装成功（有测试） |

### 仍有效的治理规则

1. 新能力：`routes` → application service → domain port → infrastructure repository。
2. 胖路由：按域拆 handler，业务下沉 `fms-application`。
3. 兼容层：legacy 前端/静态页要有退役条件，避免永久双轨。

### 守门资产

| 用途 | 路径 |
|------|------|
| 架构决策 | `docs/architecture/ADR-0001-route-service-boundary.md` |
| 技术债主计划 | `docs/plans/2026-06-29-tech-debt-sweep-master-plan.md` |
| API 分层守门 | `services/api-server/crates/api/tests/layer_boundary_guard.rs` |
| Application 债务清单 | `services/api-server/crates/application/tests/application_boundary_inventory.rs` |
| Todo 遗留字段 | `services/api-server/crates/infrastructure/tests/no_legacy_metric_fields_test.rs` |
| 文档陈旧引用 | `tests/tools/test_docs_no_stale_references.py` |
| 架构文档一致性 | `tests/tools/test_architecture_docs_consistency.py` |
| 侧车配置加密 | `services/ai-sidecar/tests/sidecar/test_postgres_config_store_uses_encryptor.py` |
| 侧车 DI 死属性 | `services/ai-sidecar/tests/sidecar/test_di_container_no_dead_attrs.py` |

### 待办（简）

1. ~~分层守门接入 CI~~ 已完成（`layer_boundary_guard` + `application_boundary_inventory`）。
2. Application inventory 继续降到 0（当前 3 个测试桩文件）。
3. ~~`legacy_compat`~~ 已删除。
4. ~~空库 migrate 自举 / CONCURRENTLY 拆分~~ 已完成。
5. 侧车控制面源真相、配置管理收敛、legacy 退役与前端拆分等见路线图。

细节与验收：`ARCHITECTURE_IMPROVEMENT_ROADMAP.md`。

---

## 历史归档（Python API 时代，2026-06-15 以前）

> 仅历史记录，不用于当前发布门禁。源码中已无 `dispatch_routes.py` 等文件。

| 历史指标 | 数值 |
|----------|------|
| 路由直连 Repository 存量 | 63 |
| Flight 路由直连 Repository | 0（红线） |

| 历史分布文件 | 违规数 |
|--------------|--------|
| `dispatch_routes.py` | 29 |
| `dispatch_order_routes.py` | 26 |
| `auth_routes.py` | 6 |
| `workflow_dispatch_routes.py` | 2 |

**已移除、勿再引用：**

- `scripts/tools/audit_route_repository_dependencies.py`
- `docs/architecture/route_repository_dependency_baseline.json`
- `tests/application/test_route_repository_dependency_audit.py`
