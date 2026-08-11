# 技术债识别与清扫主计划

> 编制日期：2026-06-29
> **核实日期：2026-06-29**（对下列结论均做过仓库内 grep / 读文件 / 行数统计）
> **Rust 架构守门更新：2026-07-07**（`fms-api` / routes / application inventory 已补 Rust 测试守门）
> 方法：对照 `docs/plans/2026-06-26-td-*`、`docs/architecture/TECH_DEBT_DASHBOARD.md`、`docs/plans/2026-06-23-*` 与当前代码树。

**目标：** 在已有分项计划基础上，形成「已完成 / 部分完成 / 未完成 / 文档过期」清单，并给出可执行的清扫波次与验收标准。**凡标注「已清扫」的项，均可在「附录 A」找到核对方式。**

> **架构评分与跨波次改进总览（2026-07-11）：**
> [`docs/architecture/ARCHITECTURE_IMPROVEMENT_ROADMAP.md`](../architecture/ARCHITECTURE_IMPROVEMENT_ROADMAP.md)
> （含 Wave 0 止血、迁移自举、AI ACL、安全 fail-closed 等本 TD 清单之外的架构项。）

---

## 附录 A：核实方法摘要（2026-06-29）

| 断言 | 核对命令/方式 | 结果 |
|------|----------------|------|
| TD-15 无内联 SQL | `scheduler_runtime_service.rs` 中无 `sqlx::query` | 0 匹配 |
| FlightSync 仓储 | 存在 `flight_sync_repository.rs`、`pg_flight_sync_repository.rs`，DI 在 `di/observability.rs` | 存在 |
| TD-20 TS `any` | `frontend/vue-app/src` 下 `: any` / `as any` / `<any>` | **0 处**（仅 `vite-env.d.ts` 中 `DefineComponent<{}, {}, any>`） |
| TD-14 DI 死属性 | `container.py` 无 `dead:`；`test_di_container_no_dead_attrs.py` | 无标记；测试文件存在 |
| TD-09 DI 拆分 | `server/src/di/mod.rs` + `ai.rs` `dispatch.rs` `flight.rs` 等 | 无根级 `di.rs` |
| TD-12 配置加密 | `postgres_config_store.py` 注入 `ConfigEncryptor`；`test_postgres_config_store_uses_encryptor.py` | 无内联 Fernet 方法 |
| TD-05 legacy 指标 | `no_legacy_metric_fields_test.rs` | 存在 |
| TD-07 wildcard 垫片 | 无 `application/services/ai/nl_query_service.py`（单文件垫片） | 仅有 `nl_query_service/service.py` 包 |
| Python `except Exception` | `services/ai-sidecar` 下计数 | **274** 处 / **86** 文件 |
| Ruff 裸 except 守门 | `test_no_bare_except.py`（BLE001 且无 noqa） | 仅约束**无 noqa 的**宽泛 except，**不**等于清零 274 处 |
| 看板审计资产 | `audit_route_repository_dependencies.py`、`route_repository_dependency_baseline.json`、`test_route_repository_dependency_audit.py` | **均不存在** |
| `dispatch_routes.py` | 全仓库 | **仅文档提及**，无源码 |
| `routes` 层 SQL | `crates/api/src/routes/**/*.rs` 中 `sqlx::query` | 由 `services/api-server/crates/api/tests/layer_boundary_guard.rs` 守门 |
| `api` crate production infra 依赖 | `fms-infrastructure`、`sqlx`、`redis` | 由 `layer_boundary_guard.rs` 守门；生产依赖已移除 |
| Application 基础设施债务 | `sqlx::query*` / `fms_infrastructure::repositories` | 由 `services/api-server/crates/application/tests/application_boundary_inventory.rs` 固定清单 |
| 巨型前端（行数） | 见下表「TD-11 核实」 | 见第二节表格 |

### TD-11 核实（行数，非 2026-06-26 计划中的旧 KB 估值）

| 文件 | 行数 | 相对 P1 目标（SFC &lt;300 / composable &lt;500） |
|------|------|--------------------------------------------------|
| `AiConfigCenter.vue` | **207** | SFC 已达标 |
| `useAiConfigCenter.ts` | **1408** | **未达标**（逻辑仍集中） |
| `DispatchBoard.vue` | **156** | SFC 已达标 |
| `dispatch_board/composables/useDispatchBoardPageActions.ts` | **523** | 略超 500 |
| `FlightDetail.vue` | **146** | SFC 已达标 |
| `FlowableModeler.vue` | **124** | SFC 已达标 |
| `useFlightData.ts` | **367** | 已 &lt;500 |
| `useDispatchBoardData.ts` | **303** | 已 &lt;500 |
| `FlightMonitorPage.vue` | **279** | SFC 已达标 |
| `AutoCopilotVoicePanel.vue` | **1392** | **未在旧 TD-11 表内，实际仍巨型** |
| `DispatchNotifyModal.vue` | **1038** | 同上 |
| `AIAssistantFloatPanel.vue` | **791** | 同上 |

### TD-10 核实（生产路由 `*.rs`，排除 `tests.rs`）

仍 **≥400 行** 的示例：`flowable/generate_process_draft_from_file.rs` **445**、`auth/login.rs` **439**、`dispatch/create_order.rs` **438**、`ai_copilot.rs` **427**、`auth/shared.rs` **428**、`flowable/shared.rs` **464**、`legacy_compat.rs` **278**。
**无**文件达到旧计划写的「15 个超 500 行」那种密度，但 **400+ 行路由仍有多份**，TD-10 **部分完成**（模块化目录已存在，单文件仍偏大）。

### ConfigManager 引用（2026-06-29）

| 模块 | 状态 |
|------|------|
| `openai_client.py` | 仍 import `ConfigManager`，保留 `config_manager` 参数与 `_load_config_from_manager`（sidecar 计划称调用方多传 `config=`，路径可能死代码，**需用引用扫描确认后删除**） |
| `app_config_integration.py` | **活跃**：默认 `ConfigManager()`，alert/email 等依赖 |
| `prompt_cache.py`、`responses_session_state.py`、`todo_graph_pilot_ops_service.py` | 可选 `config_manager: Any` 参数 |

**结论：** 不能写「仅 openai 一处」；**文件栈不能整体删**，只能按计划做 **openai 解耦 + 缩小死栈**，`app_config_integration` 仍在用 `ConfigManager`。

---

## 一、历史技术债地图（按域）

| 域 | 编号/主题 | 原始计划 | 核实结论（2026-06-29） |
|----|-----------|----------|-------------------------|
| Rust 分层 | TD-15 | P0 | **已清扫并守门**（见附录 A；2026-07-07 补 Rust guard tests） |
| 前端类型 | TD-20 | P0 | **已清扫**：业务 TS/Vue **无** `: any` / `as any`；`shared-api-types.ts` 存在 |
| Python DI | TD-14 | P1 | **已清扫** + `test_di_container_no_dead_attrs.py` |
| Python 异常 | TD-21 | P1 | **未完成**：274 处；守门测试范围见附录 A |
| Rust 组合根 | TD-09 | P1 | **已清扫**：concrete repository wiring 位于 `server/src/di/*` |
| Rust 路由瘦身 | TD-10 | P1 | **部分完成**：无 routes 直连 SQL；若干 handler 仍 400+ 行 |
| 前端巨型文件 | TD-11 | P1 | **部分完成**：多个页面 SFC 已拆小；**composable/部分组件仍超大**（见上表） |
| 配置去重 | TD-12 | P2 | **已清扫** + 单测 |
| 遗留指标 | TD-05 | P3 | **已清扫** |
| Wildcard 重导出 | TD-07 | P3 | **已清扫**（垫片文件已删，包路径保留） |
| 兼容路由 | `legacy_compat.rs` | P3 | **代码保留但生产未挂载**：`configure_pre_dispatch` / `configure_post_dispatch` 未被 `server/src/web.rs` 调用 |
| 架构看板 | Python 路由→Repository | 看板 2026-06-15 | **文档与资产均过期**：指标指向已移除的 Python 路由；审计脚本/基线/测试**已不在仓库** |
| 补丁式修复 | patch-style 计划 | 2026-06-23 | **未在本轮逐条复测**；文档内「已修复」表仍作参考，P0 编号需按 file:line 再验证 |
| Sidecar ConfigManager | config-debt 计划 | 2026-06-23 | **部分完成**：Postgres 存储已收敛；**ConfigManager 栈仍被 integration/openai 等使用** |
| 前端 legacy / 双栈 | `src/legacy/*`，vue-app + ai-react | 迁移文档 | **进行中**：`legacy/` 与 parity 测试仍在 |
| 测试质量 #136/#137 | 看板 | 变异/混沌脚本 | **存在**：`run_api_mutation_pilot.ps1`、`Invoke-LocalChaosExperiment.ps1`；**未**声称已接入 PR CI（与看板一致） |

---

## 二、仍应视为「活跃」的技术债（建议优先清扫）

### P0 — 安全与数据诚实性

1. **补丁计划 P0 项**（R1,R2,P2,P4,P5,F5）— 来源：`docs/plans/2026-06-23-patch-style-fix-remediation.md`
   - **说明：** 本轮未对每条做源码回归，列入 Wave 1 时须 **按编号打开 file:line 核实仍成立**。
   - 验收：故障时明确错误，非假成功/假数据。

2. **`legacy_compat.rs` 退役**（278 行，生产 `web.rs` 未挂载）
   - 验收：确认测试与外部引用后删除，或保留显式 deprecation + 替代路径写入 `docs/API_ROUTE_SNAPSHOT.md`。

### P1 — 可维护性与故障可见性

3. **TD-21** — 274 处 `except Exception`；优先：`todo_agent_executor/executor.py`（15）、`runtime_service/service.py`（14）、`dispatch_command_service/service.py`（11）、`tools/registry/service.py`（9）。
   - 验收：关键路径收窄异常类型；`test_no_bare_except.py` 保持通过（**该测试不等于 274→0**）。

4. **TD-11 剩余** — 重点不再是旧表中的 monolithic SFC，而是：
   - `useAiConfigCenter.ts`（1408）
   - `AutoCopilotVoicePanel.vue`（1392）、`DispatchNotifyModal.vue`（1038）、`AIAssistantFloatPanel.vue`（791）
   - `useDispatchBoardPageActions.ts`（523）
   - 验收：上述文件拆到 composable/子组件 &lt;500 行（SFC &lt;300）；`npm run typecheck && npm run test` 通过。

5. **TD-10** — 优先瘦身 ≥400 行的路由文件（见附录 A 列表）。
   - 验收：业务逻辑下沉 application；`cargo test -p fms-api` 通过。

6. **ConfigManager** — 先做 **`openai_client.py` 死路径删除**（若扫描确认无 `config_manager=` 调用），**勿**删除 `app_config_integration` 依赖的 `config_manager.py` 栈。

### P2 — 文档与治理

7. **`TECH_DEBT_DASHBOARD.md`** — 已改为 Rust 边界说明与当前 guardrail 资产；后续需保持与 `layer_boundary_guard.rs` / `application_boundary_inventory.rs` 同步。

8. **前端 legacy + ai-react 双栈** — parity 测试仍在；按 `FRONTEND_MIGRATION.md` 收敛。

9. **`libs/vendor/rocketmq-rust`** — 体积/clone 成本，**非**业务逻辑债；MQ 升级时再动。

---

## 三、建议清扫波次（2026 Q3–Q4）

| 波次 | 周期 | 内容 | 退出标准 |
|------|------|------|----------|
| **Wave 1** | 1–2 周 | 补丁 P0 **逐条核实** + `legacy_compat` 依赖调查 + Rust 架构守门接入 CI | 核实记录；Rust guard tests 在 CI 中显式运行 |
| **Wave 2** | 2–3 周 | TD-21 热点 4 文件 + `openai_client` ConfigManager 解耦（引用扫描先行） | sidecar pytest 绿 |
| **Wave 3** | 3–4 周 | TD-11：**useAiConfigCenter** + 航班监控 3 个大组件 | 行数门槛 + vitest 绿 |
| **Wave 4** | 2 周 | TD-10：2–3 个最大路由文件 | api 测试绿 |
| **Wave 5** | 持续 | legacy / ai-react 按迁移矩阵缩减 | parity 测试无回退 |

---

## 四、日常守门（仓库内**已存在**）

| 守门项 | 路径 |
|--------|------|
| 文档拓扑陈旧引用 | `tests/tools/test_docs_no_stale_references.py` |
| 架构文档一致性 | `tests/tools/test_architecture_docs_consistency.py` |
| Rust API 分层守门 | `services/api-server/crates/api/tests/layer_boundary_guard.rs` |
| Rust application 债务清单 | `services/api-server/crates/application/tests/application_boundary_inventory.rs` |
| Todo agent legacy 字段 | `services/api-server/crates/infrastructure/tests/no_legacy_metric_fields_test.rs` |
| 前端 worker 死拷贝等 | `frontend/vue-app/src/workers/__tests__/no_dead_worker_copy.test.ts` |
| Python 无 noqa 的 BLE001 | `services/ai-sidecar/tests/sidecar/test_no_bare_except.py` |
| DI 无 dead 属性 | `services/ai-sidecar/tests/sidecar/test_di_container_no_dead_attrs.py` |
| Postgres 配置用 Encryptor | `services/ai-sidecar/tests/sidecar/test_postgres_config_store_uses_encryptor.py` |
| 无 wildcard 垫片文件 | `services/ai-sidecar/tests/sidecar/test_no_wildcard_reexport.py` |
| 变异/混沌试点 | `scripts/dev/run_api_mutation_pilot.ps1`、`scripts/chaos/Invoke-LocalChaosExperiment.ps1`（本地，非 PR 必跑） |

**建议新增（Wave 1）：**

- CI：显式运行 `cargo test -p fms-api --test layer_boundary_guard` 与 `cargo test -p fms-application --test application_boundary_inventory`。
- 前端：对 `no-explicit-any` 的 eslint 规则（当前业务代码已基本无 `any`）。

**已失效、勿再引用：**

- `scripts/tools/audit_route_repository_dependencies.py`
- `docs/architecture/route_repository_dependency_baseline.json`
- `tests/application/test_route_repository_dependency_audit.py`

---

## 五、执行方式（分项计划仍有效部分）

| 计划文件 | 备注 |
|----------|------|
| `2026-06-26-td-p0-critical-fixes.md` | TD-15/20 **主体已完成**；Task 5–8 前端 any 可视为**已完成** |
| `2026-06-26-td-p1-python-cleanup.md` | TD-14 **已完成**；TD-21 **继续** |
| `2026-06-26-td-p1-rust-organization.md` | TD-09 **已完成**；TD-10 **按附录 A 更新目标文件** |
| `2026-06-26-td-p1-frontend-organization.md` | **按附录 A 更新优先级**（非旧 140KB 路径） |
| `2026-06-26-td-p2-architecture-quality.md` | TD-12 **已完成**；TD-13/16/17/18/23 仍可按需执行 |
| `2026-06-26-td-p3-legacy-cleanup.md` | TD-05/07 **已完成**；compat 路由 **未完成** |
| `2026-06-23-patch-style-fix-remediation.md` | P0/P1 **须逐条复测** |
| `2026-06-23-ai-sidecar-config-debt-remediation.md` | openai 解耦 **待做**；integration 栈 **保留** |

---

## 六、建议下一步

1. Wave 1：从 patch 计划 P0 表选 1 条，对照当前 file:line 写失败测试。
2. 对 `OpenAIClient(` 全仓库引用扫描，再动 `config_manager` 参数。
3. 将 Rust 架构守门命令接入 CI，并保持 `TECH_DEBT_DASHBOARD.md` 与守门测试同步。

如需从某一 Wave 直接改代码，指定波次即可按上表子计划执行。
