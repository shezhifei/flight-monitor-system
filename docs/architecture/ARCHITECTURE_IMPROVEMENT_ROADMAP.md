# 架构改进路线图（Architecture Improvement Roadmap）

| 字段 | 值 |
|------|-----|
| 状态 | Active |
| 编制日期 | 2026-07-11 |
| 文档基线 | 2026-08-11（与产品文档同步；历史 Wave 记录保留） |
| 基线评分 | **8.1 / 10**（模块化单体 + AI 侧车，存量债可控） |
| 目标评分 | **≈ 9.0 / 10**（边界清晰、可重建、安全默认 fail-closed、AI 不旁路域写） |
| 事实源对齐 | `docs/SOURCE_OF_TRUTH.md`、`docs/architecture/*`、`services/api-server/`、`services/ai-sidecar/` |
| 相关计划 | [技术债清扫主计划](../plans/2026-06-29-tech-debt-sweep-master-plan.md)、[TECH_DEBT_DASHBOARD](TECH_DEBT_DASHBOARD.md) |
| 相关 ADR | [0001](ADR-0001-route-service-boundary.md) · [0002](ADR-0002-flight-core-write-boundary.md) · [0003](ADR-0003-domain-event-outbox-cdc-relay.md) · [0004](ADR-0004-python-ai-worker-extraction.md) |
| Rust 分层守门 | `services/api-server/crates/api/tests/layer_boundary_guard.rs` |
| Application 债务清单 | `services/api-server/crates/application/tests/application_boundary_inventory.rs` |

> 本文是**架构级改进计划**，不是当前运行时事实源。实施时以源码与 `SOURCE_OF_TRUTH.md` 为准；完成条目后应回写本页状态与 `TECH_DEBT_DASHBOARD.md`。

---

## 1. 背景与问题陈述

### 1.1 当前架构原则

```text
Browser / Vue MPA
  -> Caddy or edge Nginx
  -> Rust API (Actix-web, services/api-server/)
  -> PostgreSQL / Redis / RocketMQ gateway / Flowable
  -> Python AI sidecar (tool execution, NL Query, LLM eval)
```

约束摘要：

1. **Rust** 是默认 HTTP / SSE / 鉴权 / 业务读写主链。
2. **Python** 只做 AI 侧车（及可选 worker/runtime），不新增 Python HTTP 主链。
3. **启动时显式装配依赖**；不靠导入期副作用单例。
4. **写侧只落库 + outbox**；缓存、SSE、投影为外围消费方（ADR-0002 / ADR-0003）。
5. **分层依赖方向**见 [DEPENDENCY_DIRECTION.md](DEPENDENCY_DIRECTION.md)；由 Rust 守门测试冻结。

### 1.2 评分快照（2026-07-11）

| 维度 | 分 | 主要短板 |
|------|:--:|----------|
| 分层与依赖方向 | 9.0 | Application 层 SQL 存量债未清零 |
| 领域边界与写模型 | 8.5 | 非航班域尚未完全对齐 outbox 副作用模式 |
| 事件驱动与一致性 | 8.5 | background / outbox 开关可见性仍依赖运维纪律 |
| AI 架构边界 | 7.5 | 契约漂移风险；ADR-0004 已 Accepted (W2-3)；侧车面偏大 |
| 安全架构 | 8.0 | 部分加固依赖环境变量才 fail-closed |
| 可观测与运维 | 7.5 | SLO 告警落地不完整；chaos/mutation 未进主 CI |
| 前端架构 | 7.0 | legacy 双轨；部分巨型 composable/组件 |
| 可演进与治理 | 8.5 | 守门宜强制进 CI；看板与 inventory 需趋势化 |
| 复杂度与认知负载 | 6.5 | 能力面广；新人与排障成本高 |
| 部署与环境一致性 | 6.5 | 干净库迁移自举 / bootstrap 路径历史缺口 |
| **综合** | **8.1** | — |

### 1.3 改进原则

1. **先止血，再收敛，再降复杂度** — 崩溃 / 假成功 / 不可复现优先于“优雅重构”。
2. **不拆微服务** — 默认保持模块化单体；仅做目录/crate 边界清晰化。
3. **债务只减不增** — `application_boundary_inventory` 与路由分层守门为硬约束。
4. **与现有计划对齐** — 不另起炉灶；本路线图汇总并排期，分项仍可引用 `docs/plans/2026-06-*`。
5. **验收可测** — 每项有明确退出标准（测试命令、行为断言或文档路径）。
6. **明确不做什么** — 见第 8 节 Anti-goals，防止范围膨胀。

---

## 2. 目标定义：何为「架构 9 分」

以下 8 条**全部满足**时，可自评综合分 ≈ **9.0**：

| # | 成功标准 | 验证方式 |
|---|----------|----------|
| S1 | **空库可重建**：官方路径 migrate（或 bootstrap→migrate）一次成功，无 gitignored 秘密脚本依赖 | CI 或脚本在空 PostgreSQL 上跑通 |
| S2 | **边界硬门禁**：分层 + inventory 在主 CI 强制；inventory 连续 ≥3 个月只减不增 | CI 日志 + 看板趋势 |
| S3 | **写一致性**：航班写路径完整 outbox 副作用；至少一个其它写域（建议 dispatch order 或 business case）对齐同模式 | 集成测试 / 事件流文档 |
| S4 | **AI 不旁路**：侧车不直写航班核心表；工具执行期 ACL fail-closed；主路径契约测试 | 单测 + 审计 grep |
| S5 | **无假成功**：关键并发写与流式失败可区分、可观测 | 单测 + 故障注入 |
| S6 | **双轨有 EOL**：`legacy_compat` / 前端 legacy 已删或文档有硬删除日期 | 代码/文档 |
| S7 | **安全默认严**：生产或环境未知时 fail-closed | 配置单测 / 启动行为 |
| S8 | **SLO 可告警**：可用性与 outbox backlog 有实际告警通道 | Grafana/告警规则或 runbook 链接 |

---

## 3. 总览：波次与时间盒

```text
Wave 0  (Week 1–2)     止血：崩溃、假成功、迁移自举、AI ACL、安全默认
Wave 1  (Week 2–4)     治理：CI 硬门禁、inventory 下降、胖路由、legacy_compat 决策
Wave 2  (Month 2–3)    AI 收敛：契约、侧车只做 AI、ADR-0004、异常类型
Wave 3  (Month 3–6)    降复杂度：legacy 退役、前端拆分、域目录化、SLO/chaos
```

| 波次 | 周期（建议） | 预期综合分 | 主主题 |
|------|--------------|:----------:|--------|
| Wave 0 | 1–2 周 | 8.1 → **8.4–8.6** | 生产诚实性与可部署性 |
| Wave 1 | 2–4 周 | → **8.6–8.7** | 治理硬门禁与分层闭环 |
| Wave 2 | 1–2 月 | → **8.8–8.9** | AI 边界与控制面 |
| Wave 3 | 持续至 6 月 | → **≈ 9.0** | 认知负载与运维成熟度 |

与 [技术债清扫主计划](../plans/2026-06-29-tech-debt-sweep-master-plan.md) 波次的映射：

| 本路线图 | 技术债主计划 | 说明 |
|----------|--------------|------|
| Wave 0 | 补丁 P0 核实 + 审计 Critical/High | 安全与数据诚实优先 |
| Wave 1 | 主计划 Wave 1 + TD-10 部分 | CI 守门 + 路由瘦身启动 |
| Wave 2 | 主计划 Wave 2（TD-21）+ AI 专项 | 侧车异常与 AI 架构 |
| Wave 3 | 主计划 Wave 3–5 | 前端巨型文件、legacy、持续治理 |

---

## 4. Wave 0 — 止血（P0）

**目标：** 进程不因契约漂移 abort；并发写不假成功；干净环境可按文档建库；AI 执行边界 fail-closed。

**不做：** 微服务拆分、大规模前端重构、MQ 替换。

### 4.1 任务清单

#### W0-1 消除 sidecar 反序列化导致的进程级 abort

| 字段 | 内容 |
|------|------|
| 优先级 | P0 |
| 问题 | 畸形 / 版本漂移的 sidecar payload 若走 `.expect()` + `panic = "abort"`，会杀死整个 Rust API 进程 |
| 主要位置 | `services/api-server/crates/api/src/services/streaming_finalizer.rs`；相关 NL Query / AI 流式终结路径；`domain` 中 `AiStructuredOutput` 等严格结构 |
| 做法 | 1. 将 `.expect()` / 不可恢复 panic 改为 `Result` 错误路径（标记 run 失败，如 `proposal_validation_failed`）<br>2. 评估契约字段：版本字段或合理 `#[serde(default)]`（仅对真正可选字段）<br>3. 增加畸形 payload 单测 / 集成测 |
| 验收 | 给定缺字段 JSON：仅当前 run 失败；`fms-rust-api` 进程存活；已有流式成功路径回归通过 |
| 测试 | `cargo test -p fms-api`（相关模块）；必要时加 targeted unit test |
| 状态 | [x] Done (2026-07-11) |

#### W0-2 乐观锁真正生效（禁止静默成功）

| 字段 | 内容 |
|------|------|
| 优先级 | P0 |
| 问题 | 部分仓储 `ON CONFLICT ... WHERE version = ...` 后未检查 `rows_affected()`，并发写表现为成功 |
| 主要位置 | `pg_todo_repository.rs`（save/update）；Flight 的 `update_status` / soft delete 等未 bump `version` 的路径（以当前代码为准） |
| 做法 | 1. `rows_affected() == 0` 返回明确并发冲突错误<br>2. 状态更新 / soft delete 与 version 不变量对齐<br>3. 对照已正确实现的 `pg_flight_repository` 模式统一 |
| 验收 | 并发单测：第二写返回冲突；无“丢更新当成功” |
| 测试 | `cargo test -p fms-infrastructure` / application 相关集成测 |
| 状态 | [x] Done (2026-07-11) |

#### W0-3 数据库迁移干净自举

| 字段 | 内容 |
|------|------|
| 优先级 | P0 |
| 问题 | 历史审计：基础表可能依赖 legacy bootstrap；部分迁移含无效 `schema_migrations` 记账；`CREATE INDEX CONCURRENTLY` 与 sqlx 事务语义冲突 |
| 主要位置 | `migrations/*.sql`；`scripts/fms.ps1`；`docs/DEPLOYMENT.md` / `QUICK_START.md` |
| 做法（二选一，须文档唯一）： | **方案 A（推荐长期）**：编号迁移自洽，补齐缺失的 `CREATE TABLE` 基线，删除无意义 `INSERT INTO schema_migrations`，为 CONCURRENTLY 迁移加 `-- no-transaction`<br>**方案 B（短期可接受）**：正式将 bootstrap SQL 纳入仓库与 `fms.ps1` 强制前置，并在文档写死顺序 |
| 验收 | 空库按 `QUICK_START` / `DEPLOYMENT` 官方路径一次成功；CI 可用同一路径 |
| 文档 | 同步 `SOURCE_OF_TRUTH.md` 迁移最新编号、`DEPLOYMENT.md`、`QUICK_START.md` |
| 状态 | [x] Done (2026-07-12) — 空库 `sqlx migrate run` 0→112 实测通过 |

#### W0-4 AI 执行期 ACL fail-closed

| 字段 | 内容 |
|------|------|
| 优先级 | P0 |
| 问题 | 实体级 `allowed_tools` / `denied_tools` 若仅在 capability 列表塑造阶段过滤，执行期 MCP 路径可能绕过 |
| 主要位置 | `services/ai-sidecar/src/infrastructure/ai/tools/tool_executor.py`；`capability_resolver.py`；`runtime_service` |
| 做法 | 1. `_execute_mcp_tool`（及等价路径）连接前强制 binding + allow/deny + enabled<br>2. 生产环境无 `capability_resolver` 时硬拒绝服务（禁止“空准备放行一切”）<br>3. 单测覆盖：禁用工具、未绑定工具、resolver 缺失 |
| 验收 | 上述用例 fail-closed；非破坏性误报工具不可执行 |
| 测试 | `.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar`（相关用例） |
| 状态 | [x] Done (2026-07-11) |

#### W0-5 安全相关配置环境未知时 fail-closed

| 字段 | 内容 |
|------|------|
| 优先级 | P0 |
| 问题 | JWT audience / CORS / CSP 等依赖 `APP_ENVIRONMENT` 时，未设或拼错可能静默放宽 |
| 主要位置 | `services/api-server/crates/api/src/middleware/jwt.rs`；`server` / `infrastructure` config；`main` 启动日志 |
| 做法 | 1. 定义明确环境枚举：`development` / `production` / `unknown`<br>2. `unknown` 与 `production` 取最严配置，或拒绝启动（二选一，须文档化）<br>3. 启动时 `tracing` 明确打印生效档位 |
| 验收 | 缺 env 或非法值时行为可预测且偏严；有单测或启动集成断言 |
| 状态 | [x] Done (2026-07-11) |

#### W0-6（可选同波次）Edge 网络隔离核对

| 字段 | 内容 |
|------|------|
| 优先级 | P1（可与 W0 并行） |
| 问题 | edge compose 若未给所有服务挂 `internal` 网络，隔离意图落空 |
| 主要位置 | `deploy/docker/docker-compose.edge.yml`（对照 `docker-compose.distributed.yml`） |
| 做法 | 全服务 `networks: [internal]`；仅需出网者另挂 egress |
| 验收 | compose config 检查；文档说明 edge 网络模型 |
| 状态 | [x] Done (2026-07-11) |

### 4.2 Wave 0 退出标准

- [x] W0-1 验收满足（生产代码已安全 + 畸形 payload 测试已补）
- [x] W0-2～W0-5 验收全部通过
- [x] 相关回归测试绿
- [x] 若改了迁移/启动，文档已同步
- [x] 在本文件「进度跟踪」表更新状态

### 4.3 Wave 0 推荐执行顺序

```text
W0-1 (panic 面)  ──并行──  W0-2 (乐观锁)
         │                      │
         └────────┬─────────────┘
                  ▼
              W0-4 (AI ACL)  ──并行──  W0-5 (安全默认)
                  │
                  ▼
              W0-3 (迁移自举，改动面大，单独 PR)
                  │
                  ▼
              W0-6 (edge 网络，可选)
```

---

## 5. Wave 1 — 治理硬门禁与分层收敛（P1）

**目标：** 架构规则进入主 CI；Application 债务可见且下降；兼容层有决策；胖路由开始瘦身。

### 5.1 任务清单

#### W1-1 分层守门接入主 CI

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 做法 | 在 `.github/workflows/ci.yml`（或等价）显式运行：<br>`cargo test -p fms-api --test layer_boundary_guard`<br>`cargo test -p fms-application --test application_boundary_inventory`<br>对应源码：`services/api-server/crates/api/tests/layer_boundary_guard.rs`、`services/api-server/crates/application/tests/application_boundary_inventory.rs` |
| 验收 | PR 无法新增 inventory 违规项；无法在 routes 生产代码新增 `sqlx::query*` / 直连 infrastructure repository |
| 关联 | [TECH_DEBT_DASHBOARD](TECH_DEBT_DASHBOARD.md) 待办 #1 |
| 状态 | [x] Done (2026-07-11) |

#### W1-2 Application inventory 趋势化

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 做法 | 1. 在看板增加「当前 inventory 条目数 / 文件数」<br>2. 可选：CI 注释或 artifact 输出计数<br>3. 规则：**只减不增**；新增项必须同 PR 清掉等量或更多旧项（建议直接禁止新增） |
| 主要位置 | `services/api-server/crates/application/tests/application_boundary_inventory.rs` |
| 验收 | 看板有数字；连续两个迭代计数下降或持平为零新增 |
| 状态 | [x] Done (2026-07-11) — inventory baseline **3** 文件（均为 tests.rs） |

#### W1-3 按域清 Application SQL 债

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 顺序建议 | 1) 航班写路径与 outbox 相关<br>2) dispatch 命令路径<br>3) business case / notification<br>4) 其余 |
| 做法 | 每批：抽出 port → infrastructure 实现 → DI 装配 → 从 inventory 删除条目 → 测试 |
| 验收 | 关键写路径 0 直接 `sqlx::query*`；inventory 行数下降 |
| 状态 | [x] 完成：批次1+2a+2b完成 (2026-07-11)，inventory 9→3 |

#### W1-4 `legacy_compat.rs` 决策与执行

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 事实 | 代码存在但生产 `web.rs` 未挂载 `configure_pre_dispatch` / `configure_post_dispatch` |
| 做法 | 1. 全仓引用扫描（测试、脚本、外部文档）<br>2. **无引用则删除**；有引用则写 EOL 日期 + 替代路径进 `API_ROUTE_SNAPSHOT.md` |
| 验收 | 删除 PR 或 EOL 文档合并 |
| 状态 | [x] Done (2026-07-11) — 文件已删除 |

#### W1-5 供应链门禁收紧

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 做法 | `cargo deny check` 去掉 `continue-on-error`（或等价软失败）；按需收紧 `deny.toml` 中 wildcards / yanked 策略 |
| 验收 | 违规依赖阻断 PR；allowlist 有注释理由 |
| 状态 | [x] Done (2026-07-11) |

#### W1-6 胖路由瘦身（TD-10）

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 目标文件（以技术债主计划附录 A 为准，实施前复测行数） | `flowable/generate_process_draft_from_file.rs`、`auth/login.rs`、`dispatch/create_order.rs`、`ai_copilot.rs`、`auth/shared.rs`、`flowable/shared.rs` 等 ≥400 行 handler |
| 做法 | handler 只保留：鉴权、入参校验、调用 application service、映射错误；业务逻辑下沉 `fms-application` |
| 验收 | 上述优先文件 &lt;300 行（或拆分子模块后单文件达标）；`cargo test -p fms-api` 通过 |
| 状态 | [x] Done (2026-07-11) |

#### W1-7 补丁式修复 P0 逐条复测

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 来源 | `docs/plans/2026-06-23-patch-style-fix-remediation.md` |
| 做法 | 按编号打开 file:line 核实是否仍成立；仍成立则写失败测试后修复 |
| 验收 | 核实记录（可附本文件附录或 PR 描述）；打开的项全部关闭或标注 WONTFIX 理由 |
| 状态 | [x] Done (2026-07-11) |

### 5.2 Wave 1 退出标准

- [x] W1-1 主 CI 强制分层守门
- [x] inventory 趋势已建立且无新增违规
- [x] `legacy_compat` 已删除或有 EOL
- [x] 至少完成 2 个胖路由文件瘦身
- [x] 供应链 deny 不再软失败（或有书面豁免）

### 5.3 日常开发规则（Wave 1 起强制）

1. 新增 API：`routes` → `application` service → `domain` port → `infrastructure` repository。
2. **禁止**在 `fms-api` 生产代码写 SQL 或直连 repository。
3. **禁止**新增 application 内联 SQL（除非走 port 化迁移的同一 PR）。
4. 新领域事件：与业务写**同事务**写入 `domain_event_outbox`；禁止写路径直接 SSE/缓存广播。
5. 密钥继续走 Vault；禁止回写长期密钥到 `.env` / compose。

---

## 6. Wave 2 — AI 架构收敛（P1/P2）

**目标：** 侧车 = 推理与工具执行；Rust = 鉴权、业务写、任务状态、实时推送。ADR-0004 已从 Accepted (W2-3)。

### 6.1 目标架构

```text
短交互（可选保留）:
  Client -> Rust (auth) -> HTTP proxy -> AI sidecar stream -> Rust finalize -> Client

长任务（ADR-0004 目标形态）:
  Client -> Rust API
         -> 校验 + 创建 ai_jobs (Postgres)
         -> 入队 (Redis / MQ)
         -> 202 { job_id }
         -> Python worker/sidecar 消费
         -> 写回 job 状态 / run events
         -> Rust SSE / 查询接口推送结果
```

### 6.2 任务清单

#### W2-1 AI 调用契约版本化

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 做法 | 1. 固定 run 请求、流式事件、proposal、structured output 的 JSON Schema / 版本字段<br>2. Rust 反序列化与 Python 产出共用契约测试（契约漂移 CI 失败）<br>3. 破坏性变更必须 bump 版本，双端兼容窗口有文档 |
| 验收 | 主路径契约测试在 CI；W0-1 类故障由契约层提前拦截 |
| 状态 | [x] 完成（2026-07-11）。跨语言字段清单 `contract_field_manifest.json` + 穷举 fixture `runtime_contract.json` 为双端单一真相；Rust `test_shared_fixture_round_trips_without_field_drift` 与 Python `test_shared_fixture.py` 对清单做字段集断言，任一端加减字段即 CI 失败。修复 Rust 丢弃 `token_usage` 的数据丢失。三条版本轴与破坏性变更规则见 `docs/architecture/AI_CONTRACT_VERSIONING.md`。漂移门禁只在测试层，不动 W0-1 运行时优雅降级。 |

#### W2-2 侧车「只做 AI」

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 问题 | `services/ai-sidecar` 中可能残留 flight/dispatch 等非纯 AI 应用服务 |
| 做法 | 1. 盘点并分类：纯 AI / 只读查询辅助 / 应删除或改调 Rust<br>2. **禁止**侧车直写 `flights` 核心表与航班状态真相<br>3. 需要写域时走 Rust DomainAction / 内部 API（Service Identity） |
| 验收 | grep / 审计：侧车无核心写 SQL；文档更新 SOURCE_OF_TRUTH 与 AI 运维页 |
| 对齐 | ADR-0002 |
| 状态 | [x] Done (2026-07-12) |

#### W2-3 异步 Job 路径落地（ADR-0004）

| 字段 | 内容 |
|------|------|
| 优先级 | P2（可与 W2-1/2 并行设计） |
| 做法 | 1. 选定首批长任务类型（如 NL 重查询、批量 copilot、eval）走 job 模型<br>2. 统一 job 状态机与超时/重试/死信<br>3. 修订 ADR-0004 状态为 Accepted，并写清与现有 HTTP 代理的边界 |
| 主要资产 | `migrations/*ai_jobs*`、`ai_jobs` 路由/服务、`docs/operations/ai-job-lifecycle.md`（若存在） |
| 验收 | 至少一类长任务端到端 job 化；前端可经 job_id / SSE 拿到结果 |
| 状态 | [ ] 未开始 |

#### W2-4 执行控制面以 Rust 为源真相

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 范围 | proposal / pending action / compensation / checkpoint / run events |
| 做法 | 状态落库与授权决策在 Rust；侧车上报事件与工具结果；避免双写两套状态机 |
| 验收 | 控制面读写路径文档化；无「仅侧车内存为真相」的关键路径 |
| 状态 | [ ] 未开始 |

#### W2-5 宽泛异常收敛（TD-21）

| 字段 | 内容 |
|------|------|
| 优先级 | P1 |
| 基线 | 技术债主计划：侧车约 274 处 `except Exception`（实施前复测） |
| 优先文件 | `todo_agent_executor`、`runtime_service/service.py`、`dispatch_command_service`（若仍在侧车）、`tools` registry / executor |
| 做法 | 收窄异常类型；关键路径失败可观测（结构化日志 + 错误码）；保持 `test_no_bare_except.py` 通过 |
| 验收 | 优先文件显著下降；不要求一波次清零 274 |
| 状态 | [ ] 未开始 |

#### W2-6 ConfigManager 死路径清理

| 字段 | 内容 |
|------|------|
| 优先级 | P2 |
| 来源 | `docs/plans/2026-06-23-ai-sidecar-config-debt-remediation.md` |
| 做法 | 全仓扫描 `OpenAIClient(` / `config_manager=`；确认死路径后删 `openai_client` 上无用分支；**保留** `app_config_integration` 所需 ConfigManager 栈 |
| 验收 | 引用扫描记录 + 相关 pytest 绿 |
| 状态 | [ ] 未开始 |

### 6.3 Wave 2 退出标准

- [ ] 契约测试在 CI
- [ ] 侧车不写航班核心真相
- [x] ADR-0004 状态更新（Accepted 或明确修订说明）
- [ ] 控制面源真相文档化
- [ ] TD-21 热点文件完成一轮收敛

---

## 7. Wave 3 — 降复杂度与运维成熟度（P2/P3）

**目标：** 复杂度/认知负载 6.5 → 7.5+；前端 7.0 → 8.0；可观测 7.5 → 8.5。

### 7.1 退役双轨

| ID | 项 | 做法 | 验收 | 状态 |
|----|----|------|------|------|
| W3-1 | 前端 legacy | 按 `docs/FRONTEND_MIGRATION.md` / parity 矩阵收敛；canonical 100% 后删 `frontend/vue-app/src/legacy/*` 与兼容 HTML 路由 | parity 无回退；死代码删除 | [ ] |
| W3-2 | 未挂载死路由 | 审计未注册路由（如历史 `ai_proxy` 等），删除或显式 feature flag | 无「看似可调用实则死代码」的安全误解 | [ ] |
| W3-3 | 文档历史污染 | 计划/runbook 标 HISTORICAL；主导航不引用过期 Python HTTP 主链 | `test_docs_no_stale_references` 绿 | [ ] |

### 7.2 前端可维护性（TD-11）

| ID | 目标文件（实施前复测行数） | 门槛 | 状态 |
|----|---------------------------|------|------|
| W3-4 | `useAiConfigCenter.ts` | composable &lt;500 | [ ] |
| W3-5 | `AutoCopilotVoicePanel.vue` | SFC &lt;300 或拆子组件 | [ ] |
| W3-6 | `DispatchNotifyModal.vue` | 同上 | [ ] |
| W3-7 | `AIAssistantFloatPanel.vue` | 同上 | [ ] |
| W3-8 | `useDispatchBoardPageActions.ts` | composable &lt;500 | [ ] |

验收：`cd frontend/vue-app && npm run typecheck && npm run test`。

### 7.3 有界上下文目录化（仍同进程部署）

| ID | 项 | 做法 | 验收 | 状态 |
|----|----|------|------|------|
| W3-9 | Application 服务按域目录 | `application/src/services/{flight,dispatch,business_case,ai,auth,...}` 归并（可渐进） | 新代码只进域目录；README/架构图更新 | [ ] |
| W3-10 | （可选）按域拆 crate | `fms-flight` / `fms-dispatch` 等，**仍单进程装配** | 依赖方向测试仍绿；无跨域反向依赖 | [ ] |

### 7.4 其它写域对齐 Outbox 模式

| ID | 项 | 做法 | 验收 | 状态 |
|----|----|------|------|------|
| W3-11 | Dispatch / Business Case 写后效应 | 对齐 ADR-0002：写侧只落库+outbox；SSE/缓存由 subscriber 驱动 | 至少一域文档 + 集成测 | [ ] |

### 7.5 可观测、弹性、质量试点升级

| ID | 项 | 做法 | 验收 | 状态 |
|----|----|------|------|------|
| W3-12 | SLO 告警 | 将 `docs/observability/SLO.md` 接到 Prometheus 规则 / Grafana 告警（可用性、写 p99、outbox backlog） | 告警规则入库或 runbook 可操作 | [x] Done (2026-07-11) |
| W3-13 | SSE Lagged 恢复 | 慢客户端 Lagged 后可重订阅，避免永久丢 topic | 单测或集成行为 | [ ] |
| W3-14 | Chaos nightly | 本地 chaos 脚本稳定后进 nightly（不挡 PR） | 流水线配置 | [x] Done (2026-07-11) |
| W3-15 | Mutation 扩展 | domain 试点稳定后，对逃逸变异补领域测试；忌刷分 | 报告归档 `.tmp/mutation` 策略不变 | [x] Done (2026-07-11) |
| W3-16 | 关键 E2E | 登录→航班列表→派工 主路径：先 nightly 阻断，再评估 PR 阻断 | Playwright 配置调整有文档 | [ ] |

### 7.6 Wave 3 退出标准

- [ ] legacy 双轨有 EOL 或已删除
- [ ] TD-11 优先文件达标
- [ ] 域目录化至少完成 application 层约定
- [ ] SLO 告警可操作
- [ ] 第 2 节 8 条成功标准自评勾选完成

---

## 8. Anti-goals（明确不做）

| 不做 | 原因 |
|------|------|
| 立刻拆多微服务 | 运维与分布式事务成本陡增；当前模块化单体更合适 |
| 把 LLM 运行时塞回 Rust 进程 | 生态与迭代速度不匹配 |
| 新增 Python HTTP 业务主链 | 与当前架构原则冲突，形成双主链 |
| 为覆盖率铺空测试 | 优先契约、乐观锁、边界守门、关键写路径 |
| 无限期保留 legacy 双轨 | 永久支付复杂度税 |
| 替换 RocketMQ / Flowable「为换而换」 | 无明确 SLA/故障证据前不引入第二套中间件 |
| 把 `libs/vendor/rocketmq-rust` 当业务债清扫 | 体积问题，随 MQ 升级另议 |

---

## 9. 依赖与风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| 迁移自举改动破坏现网升级路径 | 高 | 先在空库 CI 验证；现网只跑增量；保留回滚说明 |
| AI 契约硬化导致旧前端短暂不兼容 | 中 | 版本字段 + 兼容窗口；先服务端容忍再收紧 |
| inventory 清债范围过大拖垮迭代 | 中 | 按域小 PR；看板只看趋势不追求一周清零 |
| ADR-0004 全面 job 化改动面大 | 中 | 先一类长任务试点，短交互保留代理 |
| CI 变慢 | 低 | 守门测试保持纯静态/无外部依赖；E2E/chaos 放 nightly |

---

## 10. 度量与看板

### 10.1 建议跟踪指标

| 指标 | 来源 | 目标趋势 |
|------|------|----------|
| Application inventory 违规数 | `application_boundary_inventory.rs` | ↓ 至 0 |
| ≥400 行生产路由文件数 | 行数统计 | ↓ |
| 前端超标 SFC/composable 数 | 行数统计 | ↓ |
| 侧车 `except Exception` 热点文件数 | grep | ↓ |
| 干净库 migrate 是否通过 | CI | 稳定绿 |
| Outbox backlog | 指标 `SLO.md` | &lt; 100 |
| API 5xx 比率 | Prometheus | 满足 SLO |

### 10.2 文档同步义务

完成涉及下列变更时，按 `docs/DOCUMENTATION_WORKFLOW.md` 同步：

- 启动 / 迁移 → `QUICK_START.md`、`DEPLOYMENT.md`、`SOURCE_OF_TRUTH.md`
- 路由 → `API_ROUTE_SNAPSHOT.md`
- 边界 / 事件 → 本页 + 对应 ADR + `TECH_DEBT_DASHBOARD.md`
- AI 协议 → operations 下 AI 相关 runbook

---

## 11. 推荐首批 5 个可开 PR 的任务

按 ROI 排序，适合作为落地启动包：

| 顺序 | 任务 | 波次 | 建议 PR 标题风格 |
|:----:|------|:----:|------------------|
| 1 | W0-1 消除 streaming finalizer abort 面 | 0 | `fix(api): handle sidecar payload errors without process abort` |
| 2 | W0-2 乐观锁 rows_affected / version | 0 | `fix(infra): enforce optimistic lock conflicts on todo/flight writes` |
| 3 | W0-4 MCP 执行期 ACL + 无 resolver 拒服 | 0 | `fix(ai-sidecar): fail-closed MCP tool ACL and resolver requirement` |
| 4 | W0-5 环境未知安全默认 fail-closed | 0 | `fix(api): fail-closed security defaults when environment unknown` |
| 5 | W0-3 迁移干净自举（独立大 PR） | 0 | `fix(db): make numbered migrations bootstrap a clean database` |

并行可开：

- W1-1 分层守门进 CI（几乎不改业务代码）

---

## 12. 进度跟踪

> 实施过程中更新本表；完成波次时在「状态」列写 `Done (YYYY-MM-DD)`。

| ID | 标题 | 波次 | 状态 | 完成日期 | PR / 备注 |
|----|------|:----:|------|----------|-----------|
| W0-1 | 反序列化 abort 面 | 0 | Done (2026-07-11) | 2026-07-11 | 核实：生产代码已安全（extract_terminal 用 `if let Ok`，finalize_stream_terminal 用 `match` 将 AiStructuredOutput 反序列化失败转为 proposal_validation_failed，无 .expect() panic 面）。补 5 个畸形 payload 测试锁定行为（sse_stream_parser ×2、streaming_finalizer ×3）。lib 本身编译通过；lib test 整体编译被预存错误阻断（jwt.rs 测试引用不存在的 build_jwt_validation_full / Algorithm / validate_iss — 属 W0-5 范畴；ai_execution_readiness/tests.rs 构造函数签名变更 — 独立问题）。 |
| W0-2 | 乐观锁 | 0 | Done (2026-07-11) | 2026-07-11 | 核实：`PgTodoRepository` / `PgFlightRepository` 的 `save_in_tx` / `partial_update` 均检查 `rows_affected() == 0` 并返回 `ConcurrencyConflict`；并发写不再静默成功。`cargo test -p fms-infrastructure` 相关单测通过（74 passed）。 |
| W0-3 | 迁移自举 | 0 | Done (2026-07-12) | 2026-07-12 | **硬验收通过**：空库 `sqlx migrate run --source migrations` 0→112 全绿（113 条 success）。根因修复：sqlx 将整份 migration 作为一次 multi-statement 执行，PG 隐式事务导致**每个文件仅允许一条** `CREATE INDEX CONCURRENTLY`（仅 `-- no-transaction` 不够）。拆分 077/079/098 多余索引至 107–112；095 补 `CREATE TABLE IF NOT EXISTS ai_entities`（原先仅在 setup_postgresql.sql）。注意：已应用旧 077/079/095/098 内容的库可能触发 checksum 变更，需 `sqlx migrate info` 核对并用 repair/补跑 107–112（IF NOT EXISTS 安全）。最新迁移：`112_add_dispatch_order_logs_order_created_index.sql`。 |
| W0-4 | AI 执行期 ACL | 0 | Done (2026-07-11) | 2026-07-11 | 在 `_execute_mcp_tool` 中强制 entity binding + allow/deny + enabled 检查；生产环境无 `capability_resolver` 时硬拒绝服务。`services/ai-sidecar/tests/sidecar/test_tool_executor_entity_acl.py` 15 个测试全部通过。 |
| W0-5 | 安全 fail-closed | 0 | Done (2026-07-11) | 2026-07-11 | 新增 `RuntimeEnvironment` 枚举，`APP_ENVIRONMENT` 未设置、拼错或未知值均默认 `production`；启动日志打印生效档位。`fms-server` 单测 5 个全部通过；同时修复了 jwt.rs 与 ai_execution_readiness/tests.rs 的预存编译错误。 |
| W0-6 | Edge 网络 | 0 | Done (2026-07-11) | 2026-07-11 | 核对：`deploy/docker/docker-compose.edge.yml` 中 postgres / redis / config-seed / rust-api / ai-sidecar / nginx 全部挂载 `networks: [internal]`，且 `networks.internal.internal: true`，隔离意图已落地。 |
| W1-1 | CI 分层守门 | 1 | Done (2026-07-11) | 2026-07-11 | 核实：`.github/workflows/ci.yml` 第 59-62 行已包含 `cargo test -p fms-api --test layer_boundary_guard` 和 `cargo test -p fms-application --test application_boundary_inventory`。两个守门测试本地全部通过。 |
| W1-2 | inventory 趋势 | 1 | Done (2026-07-11) | 2026-07-11 | `application_boundary_inventory.rs` 固定清单强制「只减不增」；当前 baseline **3** 文件（均为 `tests.rs`）。看板与 CI 守门生效。 |
| W1-3 | Application SQL 清债 | 1 | Done (2026-07-11) | 2026-07-11 | 批次1：新增 `DatabaseMetadataPort` + `PgDatabaseMetadataAdapter`，替换 2 个服务中的 `sqlx::query_scalar`，inventory 9→7。批次2a：新增 `DomainEventOutboxTransactionalRepository<Tx>` port + impl + `SqlxDomainEventOutboxTransactionalRepository` 别名 trait；`domain_event_outbox_delivery.rs` 改用 port 替代 `Arc<PgDomainEventOutboxRepository>`；`flight_domain_events.rs` 通过 infra re-export 移除 `fms_infrastructure::repositories` 路径；inventory 7→5。批次2b：`domain_event_outbox_delivery.rs` 新增 `claim_pending` 方法委托 port；`domain_event_relay_service.rs` 删除冗余 `outbox_repo` 字段，参数改为 `Arc<dyn SqlxDomainEventOutboxTransactionalRepository>`，`lock_pending_rows` 委托 `delivery.claim_pending`；`domain_event_cdc_relay_service.rs` 参数改为 port trait；inventory 5→3。无需新建 `CdcAdmin` port 或事务管理 port——两文件债务根因仅为 `fms_infrastructure::repositories` import 路径。`fms-application` lib：445 passed；`fms-domain` lib：179 passed；`fms-api` lib：348 passed；`application_boundary_inventory`：1 passed；`fms-server` cargo check OK。剩余 3 文件为 `#[cfg(test)]` 模块内的测试桩或 service tests 子模块，不属结构性债务。 |
| W1-4 | legacy_compat | 1 | Done (2026-07-11) | 2026-07-11 | 全仓扫描确认 `legacy_compat.rs` 是死代码：`configure_pre_dispatch` / `configure_post_dispatch` 从未在 `web.rs` 或 `main.rs` 挂载，路由已由 `dispatch.rs` 接管。已删除 `legacy_compat.rs`、从 `mod.rs` 移除模块声明、从 `test_architecture_docs_consistency.py` 移除 legacy_compat 检查、更新 `API_ROUTE_SNAPSHOT.md` 和 `TECH_DEBT_DASHBOARD.md`。layer_boundary_guard 4 passed；architecture docs 4 passed。 |
| W1-5 | cargo deny 强制 | 1 | Done (2026-07-11) | 2026-07-11 | 核实：CI 中 `cargo deny check` 无 `continue-on-error`；`deny.toml` 已配置 `multiple-versions = "deny"` 和 `wildcards = "deny"`。唯一 `continue-on-error: true` 是 E2E job（第 261 行），与 cargo deny 无关。 |
| W1-6 | 胖路由 TD-10 | 1 | Done (2026-07-11) | 2026-07-11 | 完成两个胖路由文件瘦身：①`auth/login.rs`（438行→11行模块根 + 4 子模块：session.rs 173 / user_management.rs 156 / online_status.rs 77 / permissions.rs 35）；②`dispatch/create_order.rs`（437行→11行模块根 + 4 子模块：lifecycle.rs 263 / safety.rs 89 / replan_ops.rs 47 / queries.rs 41）。所有子模块单文件 <300 行，handler 仅保留鉴权/入参校验/调用 service/映射错误。`cargo test -p fms-api`：348 lib + 4 boundary guard 全通过；`application_boundary_inventory` 通过；`test_architecture_docs_consistency.py` 通过。 |
| W1-7 | 补丁 P0 复测 | 1 | Done (2026-07-11) | 2026-07-11 | 逐条核实 `docs/plans/2026-06-23-patch-style-fix-remediation.md` 中 P0 清单 Task 1-6，全部已修复：①Task 1（R1）MQ Gateway `is_authorized` 返回 `!auth.require_auth`，prod 下 token 缺失返回 false（`mq-gateway/src/http.rs:234-240`）；②Task 2（R2）`resolve_workflow_internal_token_for_environment` prod 下 Err，不再 `unwrap_or_default()`，有测试覆盖（`server/src/config.rs:574-598, 924-942`）；③Task 3（P2）`effective_fail_closed = config_fail_closed or is_production_environment()`，未发现能力时 raise（`capability_resolver.py:555-571`）；④Task 4（P4）`_stub_flight_state`/`_stub_equipment_state` 零调用点，`_get_equipment_state` 返回 None 而非 stub（`data_access.py:164-167`）；⑤Task 5（P5）`_handle_todo_create` 的 `ImportError` 分支改为 `raise RuntimeError`，不再返回 `success: True`（`action_handlers.py:576-578`）；⑥Task 6（F5）`sanitizeHtml` 无 DOMPurify 时 `return _escapeHtml(html)`（`markdown-sanitize-config.js:46-47`）。 |
| W2-1 | AI 契约 | 2 | Done (2026-07-11) | 2026-07-11 | 字段清单+穷举 fixture 双端漂移门禁；修复 token_usage 丢失；见 AI_CONTRACT_VERSIONING.md |
| W2-2 | 侧车只做 AI | 2 | Done (2026-08-13) | 2026-08-13 | Sidecar 不直写核心域真相表；业务写只生成 proposal，并由 Rust `DomainActionExecutor` 执行。已删除未挂载的 AIP dual-mode 平行栈、AIP ontology CRUD/loader、Python 业务写 action handlers、legacy dispatch command 工具、旧 DI/AIPlugin 组合根和相应死实现测试。现行入口只装配 `infrastructure/ai/ai_container.py`，本体只保留 Rust schema mirror。 |
| W2-3 | ADR-0004 job 路径 | 2 | Done (2026-07-12) | 2026-07-12 | ADR-0004 从 Proposed 升级为 Accepted。实现采用 Postgres leasing（SKIP LOCKED）替代 ADR 原设计的 Redis 队列，避免新增基础设施依赖。两层独立 lease：Rust 层 lease `ai_jobs` 表（控制面），Python 层 lease `ai_runtime_commands` 表（执行面）。SSE 经 outbox→CDC→SSE 路径发布。Python worker 通过 ServiceIdentity JWT 认证调用 Rust internal API（POST /internal/ai/v1/jobs/lease、heartbeat、runs、events、complete、fail）。NL 查询支持 async_mode 字段，异步模式返回 202 + job_id。Rust 侧新增 `ai_job_timeout_reaper_service`（spawn_tracked + interval scan）回收超时 job。Python 侧新增 `AiJobWorker` + `ServiceIdentityIssuer` + composition root（degrade-closed）。测试：45 个新测试全通过（JWT issuer round-trip + path mismatch + config + worker degrade-closed + 409 handling + aclose）。 |
| W2-4 | 控制面源真相 | 2 | 未开始 | | |
| W2-5 | TD-21 异常 | 2 | 未开始 | | |
| W2-6 | ConfigManager 死路径 | 2 | 未开始 | | |
| W3-1～W3-11 | 见第 7 节 | 3 | 未开始 | | |
| W3-12 | SLO 告警 | 3 | Done (2026-07-11) | 2026-07-11 | 新增版本化 Prometheus 规则，覆盖 API 可用性、写 p99、outbox backlog；observability compose 显式挂载；新增 `docs/observability/ALERT_RESPONSE.md` 与配置契约测试。 |
| W3-13 | SSE Lagged 恢复 | 3 | 进行中 | | 生产代码已有按 topic 重订阅逻辑，但仍缺 Lagged 后继续收取新消息的行为测试，未提前标 Done。 |
| W3-14 | Chaos nightly | 3 | Done (2026-07-11) | 2026-07-11 | `.github/workflows/nightly.yml` 已配置独立 non-blocking chaos job，运行本地 chaos 脚本并归档报告。 |
| W3-15 | Mutation 扩展 | 3 | Done (2026-07-11) | 2026-07-11 | nightly mutation pilot 强制要求工具并归档 `mutants.out` 报告；不进入 PR 阻断链。 |
| W3-16 | 关键 E2E | 3 | 未开始 | | |

---

## 13. 相关文档索引

| 文档 | 用途 |
|------|------|
| [SOURCE_OF_TRUTH.md](../SOURCE_OF_TRUTH.md) | 运行时事实源 |
| [DEPENDENCY_DIRECTION.md](DEPENDENCY_DIRECTION.md) | 分层依赖方向 |
| [TECH_DEBT_DASHBOARD.md](TECH_DEBT_DASHBOARD.md) | 边界债当前指标 |
| [ADR-0001](ADR-0001-route-service-boundary.md) | 路由禁止直连仓储 |
| [ADR-0002](ADR-0002-flight-core-write-boundary.md) | 航班写链路 |
| [ADR-0003](ADR-0003-domain-event-outbox-cdc-relay.md) | Outbox CDC |
| [ADR-0004](ADR-0004-python-ai-worker-extraction.md) | AI Worker 抽取（Accepted） |
| [技术债清扫主计划](../plans/2026-06-29-tech-debt-sweep-master-plan.md) | TD 项与核实附录 |
| [SLO.md](../observability/SLO.md) | 服务级别目标 |
| [SYSTEM_AUDIT_REPORT_2026-06-21.md](../SYSTEM_AUDIT_REPORT_2026-06-21.md) | 独立审计问题清单 |
| [DOCUMENTATION_WORKFLOW.md](../DOCUMENTATION_WORKFLOW.md) | 文档同步流程 |

---

## 14. 修订记录

| 日期 | 变更 |
|------|------|
| 2026-07-11 | 初版：评分基线 8.1、Wave 0–3 详细计划、成功标准、Anti-goals、首批 5 PR |
| 2026-07-11 | W0-1 Done：核实生产代码已消除 sidecar 反序列化 panic 面；补 5 个畸形 payload 测试；发现 lib test 预存编译错误（jwt.rs / ai_execution_readiness tests.rs，属 W0-5 及独立问题）|
| 2026-07-11 | Wave 3 继续：W3-12 Prometheus SLO 规则与 runbook 落库；核实 W3-14 chaos nightly、W3-15 mutation nightly 已满足验收；W3-13 因缺恢复行为测试保持进行中。 |
| 2026-07-12 | W0-3 阻断收尾：拆分多语句 CONCURRENTLY（107–112）、095 补 ai_entities；空库 migrate 0→112 实测通过；同步 §4/§5 状态与 SOURCE_OF_TRUTH/QUICK_START/DEPLOYMENT 最新迁移编号。 |
