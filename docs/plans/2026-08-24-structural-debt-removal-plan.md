# 结构性技术债清除计划

- 日期：2026-08-24
- 前置阅读：`docs/plans/2026-06-29-tech-debt-sweep-master-plan.md`（本计划的直接前身，也是本计划的反面教材）
- 范围：Rust `services/api-server/` 的分层与依赖注入。**不含**前端 CSS、Playwright、迁移基线、Python except——理由见末节。
- 性质：**删除计划**。不是清扫计划，不是加固计划。

---

## 0. 为什么需要第 7 份，以及前 6 份为什么没生效

`2026-06-29-tech-debt-sweep-master-plan.md` 至今约 2 个月。用它自己列出的指标复测：

| 指标 | 计划记录值 | 2026-08-24 实测 | 位移 |
|---|---|---|---|
| `useAiConfigCenter.ts` | 1408 行 | 1403 行 | −5 |
| `DispatchNotifyModal.vue` | 1038 行 | 1037 行 | −1 |
| `AutoCopilotVoicePanel.vue` | 1392 行 | 1146 行 | −246 |
| `AIAssistantFloatPanel.vue` | 791 行 | 473 行 | −318 |
| Python `except Exception` | 274 处 / 86 文件 | 245 处 / 89 文件 | 处数 −29，**文件数 +3** |

前两行是「没动」。最后一行是「摊薄了但扩散了」——总处数下降，涉及文件反而增加，说明宽泛捕获仍在新代码里继续生长，只是每个文件里少了几处。

真正的诊断不在数字里，而在那份计划自己的措辞：

- 它把 application 层基础设施债的**解决方式**写成「由 `application_boundary_inventory.rs` **固定清单**」；
- 它把 Ruff 裸 except 那条注明「仅约束**无 noqa 的**宽泛 except，**不**等于清零 274 处」——问题被准确认识，然后守门被照原样交付。

所以前 6 份计划的验收标准实际上是**「守门测试存在」**，而不是「坏状态不再可表示」。守门测试确实都存在，也都在 CI 里跑绿（`.github/workflows/ci.yml:91` "Architecture boundary guard"）。绿灯之下，结构一寸未动。

`application_boundary_inventory.rs` 是这个失败模式的标本。它的 `DEBT_PATTERNS` 只有 4 个：

```rust
const DEBT_PATTERNS: &[&str] = &[
    "sqlx::query", "sqlx::query_as", "sqlx::query_scalar",
    "fms_infrastructure::repositories",
];
```

漏掉了 `PgPool`、`Transaction<`、`Postgres`。补上之后，application 的**生产**代码里有 **30 个文件**越过数据端口直接摸 Postgres 类型——而 `production_application_source_does_not_bypass_domain_data_ports` 这个测试今天是绿的。守门不是在守边界，是在证明「我们统计过这些债」。

### 本计划的唯一规则

> **每一步的完成定义（DoD）必须是「某个东西不再存在」。**

可以作为交付物的：删掉的依赖行、删掉的文件、删掉的类型参数、删掉的字段、从运行期错误变成编译错误。

**不可以**作为任何一步交付物的：新的守门测试、新的基线清单、新的文档、新的常量、新的 `#[allow]`、新的 `noqa`。全篇只有 P0 一处新增断言，且它的目的是让 CI **变红**。

如果某一步做完后 `git diff --stat` 的净变化是「+测试 +文档」，那一步就是失败的，不论测试写得多好。

---

## P0：先让守门测试说真话（约 1 小时，1 个提交）

**做什么**：把 `PgPool`、`Transaction<`、`Postgres` 加进 `DEBT_PATTERNS`（`services/api-server/crates/application/tests/application_boundary_inventory.rs:4`）。

**预期结果**：CI 立刻变红，`production_application_source_does_not_bypass_domain_data_ports` 报出 30 个文件。

**这不是「加守门」**：断言本来就在，只是瞎的。这一步不引入新检查，只是让既有检查停止说谎。这也是全篇唯一一处新增断言。

**关键约束**：**不准**把这 30 个文件写进 `expected` 基线数组。那正是前 6 份计划做过的事。红灯保持红，直到 P1–P3 把它清成 0；`application_services_boundary_debt_inventory_matches_baseline` 连同它的 `expected` 数组在 P3 结束时**整体删除**——留着它就是留着下一次「固定清单」的工具。

**DoD**：CI 红，且 `expected` 数组没有变长。

> P0 之后 CI 会持续红。这是刻意的：它把「结构未修」从一份没人读的文档，变成每次推送都能看见的状态。如果团队受不了长期红灯，就把这个测试标 `#[ignore]` 并在 P1 开工时解开——但**不要**通过扩基线来消红。

---

## P1：删掉 DispatchService 的 26 个 `Option`（收益最高，且是机械的）

### 现状（已核实）

`crates/application/src/services/dispatch_service/mod.rs:108` 起，5 个 Dependencies 结构，**28 个字段中 26 个是 `Option<Arc<dyn ...>>`**，配 13 个 `with_*` builder。模块共 12,073 行。

生产环境的构造点**只有一个**：`crates/server/src/di/dispatch.rs:119-156`。那里 **13 个 `with_*` 全部被无条件调用**，一条链下来没有任何 `if`、没有任何 feature gate：

```rust
DispatchService::new(repos.dispatch_order_repo.clone())
    .with_transactional_repos(...)            .with_dispatch_repos(...)
    .with_generation_repos(...)               .with_publication_preparation_repos(...)
    .with_member_repos(...)                   .with_issue_reporting(...)
    .with_collaboration_repo(...)             .with_alert_repo(...)
    .with_notification_service(...)           .with_dispatch_chat_service(...)
    .with_resource_availability_service(...)  .with_todo_repo(...)
    .with_overrun_warning_service(...)
```

**结论：26 个 `Option` 在生产中恒为 `Some`。** 它们制造的 21 种「依赖没接线」错误消息——中英混杂、各写各的：`task_type_repo not injected`、`异常仓储未配置`、`todo service unavailable`、`DispatchService order transactional repository is not configured`、`安全检查清单服务不可用`……**在生产中全部不可达**。

这套机制服务的全部对象是 5 个测试构造点：

- `api/src/routes/ai_proposals/tests.rs:566`
- `application/src/services/ai_action_proposal_service/tests.rs:432`
- `application/src/services/domain_action_executor/tests.rs:93`
- `application/src/services/dispatch_service/tests.rs:342` 和 `:371`

也就是：**26 个可选字段 + 21 种错误路径 + 一部分 NullRepository，全部只为了让 5 个测试少写几行接线。**

### 做什么

1. 26 个字段去掉 `Option`，改为构造时必填。13 个 `with_*` builder 删除，`new` 改为接一个 `DispatchServiceDependencies`（5 个已有的子结构直接复用，全部字段非 `Option`）。
2. `di/dispatch.rs` 的链式调用改为一次结构体字面量——它本来就在传全部 26 个值，改动是纯形状变换。
3. 5 个测试构造点：建一个 `DispatchServiceDependencies` 的测试夹具，缺的依赖用**该测试真正需要的** fake / in-memory 实现填。这一步会暴露「这些测试到底依赖了什么」——目前 `None` 把它藏起来了。
4. 删除所有因 `Option` 而生的取值失败分支。`dispatch_service/` 下 136 处 `as_ref()` 与 81 处 `ok_or_else` 中，凡属「依赖未接线」类的连同错误消息一起删。

### DoD（全部是删除）

- `grep -c 'Option<Arc<dyn' dispatch_service/mod.rs` == **0**
- `grep -c 'pub fn with_' dispatch_service/mod.rs` == **0**
- 上列 21 种「未配置 / not injected / 不可用」消息在 `dispatch_service/` 下 grep 为 **0**
- `mod.rs` 行数下降；`di/dispatch.rs` 行数下降
- 忘接依赖从「运行期 500」变成「**编译不过**」

### 风险

低。生产行为不变（26 个值本来都在传）。唯一真实工作量在第 3 步的测试夹具，且那部分暴露的是既有的测试模糊性，不是新增复杂度。

---

## P2：删掉 `NullRepository`

### 现状

`crates/domain/src/ports/mod.rs` 共 **1737 行**，其中约 **1680 行**是内联 `mod null_repository_impls`，为约 34 个 trait 提供返回 `Ok(None)` 或 `Err(Internal("NullRepository"))` 的桩实现。真正的端口声明只有 46 行 `pub mod` + 1 行 `pub use`。

```rust
/// Null object implementations for all dispatch repository traits.
/// Used as default generic type parameters for optional dependencies.
pub struct NullRepository;
```

注释写明了它的存在理由：**给可选依赖当默认类型参数**。P1 消灭了最大的可选依赖消费者。剩余 36 处使用分布在 8 个文件：

| 文件 | 处数 |
|---|---|
| `dispatch_resource_service/service.rs` | 8 |
| `notification_service/service.rs` | 6 |
| `dispatch_schedule_service.rs` | 6 |
| `dispatch_frontend_replan_service/mod.rs` | 5 |
| `business_case_service/service.rs` | 4 |
| `dispatch_service/mod.rs` | 3 |
| `resource_availability_service.rs` | 2 |
| `business_case_service/schemas.rs` | 2 |

`domain/Cargo.toml` 本身是干净的（无 sqlx）——所以这一步纯粹是删代码，不涉及依赖调整。

### 做什么

按 P1 的同一手法逐个服务处理：删掉默认类型参数 → 调用方必须传真实实现 → 删掉桩。`dispatch_frontend_replan_service`（`di/dispatch.rs:158` 起）的构造链形态与 `DispatchService` 相同，可直接套用 P1 的做法。

**顺序说明**：P1 只解决 36 处中的 3 处，不会机械地把 P2 变简单。P1 先行的理由是它是最大单体、构造点唯一、风险最低——先在那里把手法验证一遍，再推广到其余 7 个文件。

### DoD

- `grep -rn 'NullRepository' --include='*.rs' services/api-server/` == **0 处**
- `ports/mod.rs` 从 1737 行降到约 **60 行**
- `pub struct NullRepository` 与 `mod null_repository_impls` 从代码库消失

---

## P3：删掉 `application → infrastructure` 这条边

### 现状

这是**清单层面**的违规，不是代码风格问题。`crates/application/Cargo.toml` 直接依赖：

- `fms-infrastructure` ← 分层反向
- `sqlx`（postgres features）← 应用层不该知道数据库品牌

承重结构是 `crates/application/src/sqlx_transactional_repositories.rs:21-111` 的 **10 个别名 trait**：

```rust
pub trait SqlxDispatchOrderTransactionalRepository:
    for<'tx> DispatchOrderTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync {}
```

（另有 Flight / Todo / Notification / Anomaly / BusinessCase / DispatchOrderMember / FlightTimeline / Ontology / DomainEventOutbox 九个同形变体。）

它们之所以存在，是因为 HRTB（`for<'tx>`）与 `dyn` 不兼容——不能写 `Arc<dyn for<'tx> Repo<Transaction<'tx, Postgres>>>`。用一个带具体技术名的别名 trait 把 HRTB 包起来，就能 `dyn` 了。代价是 **`Postgres` 这个类型被钉进了应用层的公开签名**，并顺着 `Arc<dyn Sqlx*>` 传到 `di/dispatch.rs`。

### 做什么

在 domain 引入一个不含任何数据库类型的工作单元端口，让应用层持有 `Arc<dyn UnitOfWork>`；事务句柄以关联类型 / 不透明类型出现，`Transaction<'tx, Postgres>` 只在 infrastructure 内部具体化。`sqlx_transactional_repositories.rs` 从 application **删除**——若仍需别名 trait，它在 infrastructure 里重新出现，那里 `Postgres` 是合法词汇。

30 个文件按批迁移。`ai_runtime_service/in_memory_repos.rs`、`in_memory_ai_proposal_repository.rs` 这类命名带 in-memory 却引 sqlx 的，先单独看一眼——可能是纯粹的错放，能直接删依赖。

### DoD

- `application/Cargo.toml` 中 **`fms-infrastructure` 依赖行被删除**
- `application/Cargo.toml` 中 **`sqlx` 依赖行被删除**
- `application/src/sqlx_transactional_repositories.rs` **文件删除**
- `production_application_source_does_not_bypass_domain_data_ports` 由 P0 的红转绿——**靠 30 降到 0，不靠改 pattern**
- `application_services_boundary_debt_inventory_matches_baseline` 连同 `expected` 数组**整体删除**（边界已由类型系统保证，清单测试没有存在理由）

### 风险

高，工作量最大，且**唯一一处需要真做设计**的地方。建议先在 1 个 trait（`DomainEventOutbox`，扇出最小）上打样，确认 `dyn`-able 的工作单元端口成立，再推其余 9 个。若打样失败，如实记录失败原因并**停在这里**——P1 / P2 的收益已经落地，不要为了凑完整度硬推。

---

## P4：ts-rs 二择一（独立，可随时做）

### 现状（已核实）

`use ts_rs::TS` 在 21 个 schema 文件中出现 **1 次**（`auth_schemas.rs:7`）。全仓（含 `.github/`）搜不到 `TS_RS_EXPORT_DIR`、`export_to`、任何 build script 配置。

也就是说：`#[ts(export)]` 生成的 `.ts` 落在 `target/` 里，**没有任何人读**。契约仍然在 Rust / TypeScript / Dart 三处手抄。这不是「ts-rs 用得少」，是**它的输出 100% 被丢弃**，同时留下「我们有契约代码生成」的错觉。

### 做什么——必须选一个，不许维持现状

- **A（便宜、诚实）**：删掉 `ts-rs` 依赖、`auth_schemas.rs` 的 `use ts_rs::TS` 与所有 `#[ts(export)]`。手抄契约的问题依然存在，但至少不再假装已解决。
- **B（有价值、更大）**：在**同一批改动内**打通端到端——配置导出目录、生成产物纳入版本管理、CI 校验产物与源码一致、**并删除被取代的手写 TS / Dart 镜像**。

B 的 DoD 必须包含「删除手写镜像」。只配好生成而不删手抄件，等于把 P4 变成第 7 个守门，直接算失败。

**DoD**：`grep -rn 'ts_rs' services/api-server/` 为 0（选 A）；或手写镜像文件被删除且 CI 校验生成产物（选 B）。

---

## 不在本计划内（明确排除）

| 项 | 为什么排除 |
|---|---|
| 前端三层 token / CSS 优先级战争 | 需要先定「哪一层是唯一真源」的产品决策，不是纯技术删除。见 `docs/architecture/SIGNAL_SURFACE.md` |
| Playwright 视觉对比基准指向已删除的参考实现 | 纯删除，风险最低，但与本计划的分层主线无关，可独立随时做 |
| 迁移基线漂移 / `IF NOT EXISTS` 重复建 | 涉及生产库状态核对，必须单独排期，不能与重构混在一个窗口 |
| Python 245 处 `except Exception` | 逐处需要判断真实异常类型，无法结构化删除；本计划的规则套不上去 |
| vendored 代码里的嵌套 `CLAUDE.md` | 纯删除，一分钟的事，随手做掉即可，不值得占一个阶段 |
| `frb_generated.*`（12,658 行提交产物） | 生成物提交是 flutter_rust_bridge 的既定用法，不是债 |

排除不等于否认。上表每一项都真实存在，只是不适合和分层重构共用一个执行窗口。

---

## 执行顺序

```
P0 ──> P1 ──> P2 ──> P3
(让红灯出现) (最大单体) (推广手法) (真做设计)

P4 ── 独立，任意时刻
```

P0 是 P1–P3 的前提：没有红灯，就没有「何时算完」的客观信号。P4 与主线无耦合。

建议节奏：P0 单独一个提交；P1 一个 PR；P2 按文件拆 3–4 个 PR；P3 先打样 PR，再逐 trait。**不要把 P1–P3 合成一个大 PR**——那会让 review 失效，而 review 失效是前 6 份计划得以「绿灯通过」的另一半原因。

---

## 如何判断这份计划也失败了

以下任一条成立，即视为第 7 次失败，且**不要写第 8 份**——改为直接停止投入并把结论记录下来：

1. `application_boundary_inventory.rs` 的 `expected` 数组变长过。
2. 出现了任何新的 `DEBT_PATTERNS`-式清单、新的基线快照测试、或新的 `#[allow]` / `noqa` 批量豁免。
3. 三个月后（2026-11-24）复测：`grep -c 'Option<Arc<dyn' dispatch_service/mod.rs` 仍 > 0，或 `grep -rn 'NullRepository'` 仍 > 0。
4. 本文档被更新成「已完成 P1，守门测试已就位」这类措辞——守门测试不是任何一步的完成标志。

第 3 条是可机械复测的，也是本节的核心。前 6 份计划之所以能连续失败 6 次而每次都看起来在推进，就是因为**没有任何一份写下过可被证伪的失败条件**。
