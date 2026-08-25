# 结构性技术债清除计划

- 日期：2026-08-24
- 前置阅读：`docs/plans/2026-06-29-tech-debt-sweep-master-plan.md`（本计划的直接前身，也是本计划的反面教材）
- 范围：Rust `services/api-server/` 的分层与依赖注入。**不含**前端 CSS、Playwright、迁移基线、Python except——理由见末节。
- 性质：**删除计划**。不是清扫计划，不是加固计划。

---

## 执行状态（截至 2026-08-25）

| 阶段 | 状态 | 提交 / PR | 净变化 |
|---|---|---|---|
| P0 让守门说真话 | ✅ | `f6f79b8` | 断言从 3 个文件改为真实的 31 个 |
| P1 删 DispatchService 的 `Option` | ✅ | PR #4 `b188015` | 26 个 `Option<Arc<dyn …>>` → 0 |
| P2 删 `NullRepository` | ✅ | PR #5 `38eb757`、`26b1144` | domain 公开 API 少 1687 行；净 −532 |
| P3 删 `application → infrastructure` | 🔶 进行中（**清单未归零，仍是红的**） | `60b68b5` … `01eeb1c`（PR #8） | 见下 |
| P4 ts-rs 二择一 | ⬜ 未开始 | — | — |

P3 已落地的三步：

1. `60b68b5` 让守门看穿别名 trait（见失败判据第 1 条的豁免记录）。
2. `42e7ff5` **`fms-infrastructure` 依赖行已从 `[dependencies]` 删除**。实际只有 2 处生产引用：
   `PgCdcAdmin`（提到 domain 成为 `CdcAdminPort`）与 `serialize_json_pretty`（就是
   `serde_json::to_string_pretty` 加一个没人读的指标，直接换掉）。
3. `e5f57bf` **HRTB 已删除**——18 个 `for<'tx>` 归零，代价是 23 处事务签名钉成 `'static`。

4. `ab40062` **打样成功**：`UnitOfWork` 端口成立，3 个文件已彻底无 sqlx，守门清单 32 → **29**。
5. `d143802`、`4d5004e`、`2c84c83` **删掉只包了一条语句的「事务」**：单条语句的 `begin`/`commit`
   不构成事务边界，失败路径上的 `rollback` 是空操作，删掉后语义不变。
6. `486c05f` 第一个真正的 `UnitOfWork` 转换，连同它需要的接缝端口。
7. `9e6cfe1` 运行时服务的两个事务提进窄协作者（`DispatchTimelineWriter`，端口只有两个方法）。
   那个服务有 18 处 `web::Data` 注入，所以是服务保持非泛型、只把带事务的两个单元提出去。
8. `01eeb1c` 删掉 `AnomalyService` 的事务转发层（见下面的测量）。清单 → **22**。

P3 剩余（**未完成**，下列每一项都还在）：

- `sqlx` 仍是 `crates/application/Cargo.toml:22` 的生产依赖——这是 P3 的 DoD，尚未达成；
- 守门清单仍有 **22** 个文件持有 sqlx 类型；
- 13 个事务仍直接从 `PgPool` 开：`ontology_service`（8）、`flight_service`（4）、
  `domain_action_executor`（1）。

清单降到 0 **且**那行依赖被删掉之前，P3 不算做完；上面这 8 步都只是把范围缩小了。

### 打样结论（`ab40062`，供后续逐个推进时照抄）

- **`Tx` 必须是关联类型**，不是 trait 的泛型参数。既有的 `XxxTransactionalRepository<Tx>` 本来就对 `Tx` 泛型，
  所以**一个仓储端口都不用改**。
- **泛型要在 crate 边界停住。** 把 `U` 一路穿到 `fms-api` 会落到十来个
  `web::Data<Arc<SchedulerRuntimeService>>` 处理器签名上，逼 `fms-api` 为了拼出类型而把
  `fms-infrastructure` 升为生产依赖——那是把 P3 的成果换成隔一个 crate 的同一种违规。
  做法是在接缝上放一个窄端口（`DomainEventRelay`，两个方法），泛型到此为止。
  副作用是好的：`fms-api` 从点名具体应用服务变成只认端口。
- **别在 `impl<U: UnitOfWork>` 里留不用 `self` 也不用 `U` 的函数**，否则调用方推不出 `U`（E0283）。
- 不要加 `rollback`：`sqlx::Transaction` drop 即回滚，加了就是死代码。

### 测量：`*_in_tx` 这一面从不跨越 api 缝（2026-08-25）

在动 dispatch 之前把剩余范围重新量了一遍。结论改变了后续几步的形状，所以记在这里，
免得下一轮又按「每个服务都要配一个窄端口」去估工。

- application 层每一个 `*_in_tx` 方法的调用方**只有** `domain_action_executor/service.rs`
  一处（`dispatch_service` 内部的 `sync_assignment_members_in_tx` 是同服务自调，不跨界）。
- **api 层零调用**：没有任何处理器调用过 `*_in_tx`。
- 执行器自身有**零个 `web::Data` 注入点**，只被 `ai_action_proposal_service` 和
  `ai_runtime_service/rollback_service` 两个应用服务持有。

也就是说整个 `*_in_tx` 面是执行器与七个服务之间的私有协议，它从不跨越 crate 缝。
因此这一面的改造**不需要缝端口，也不需要动 api**；只在执行器与那两个持有者之间放一个
窄端口就够。

第二个测量：这些方法体里**没有一处裸 SQL**，它们只是把 `tx` 转发给本来就对 `Tx` 泛型的
仓储端口。真正把 `Tx` 钉成 `Transaction<'static, Postgres>` 的是方法体读到的**别名类型
字段**（`Arc<dyn SqlxFooTransactionalRepository>`），不是 SQL。所以每一步的着力点是那个
字段，不是方法签名——只改签名不动字段是编译不过的（字段无法对方法的泛型参数泛型）。

据此分成两种形状，按方法体里有没有真实领域逻辑来选：

- **纯转发 → 直接删。** `AnomalyService` 的三个方法就是这种：端口早已泛型，中间这层只是
  把具体类型重新钉回去，删掉它**减少**别名引用，而不是把别名搬个地方。
- **带领域逻辑 → 提进泛型协作者**（`FlightTimelineWriter` 那个形状）。`todo_service.rs`、
  `notification_service/service.rs`、`business_case_service/service.rs` 的 sqlx 面完全一致，
  而且**只有**这些：一行 `use sqlx::{Postgres, Transaction}`、一行别名 import、一个 `tx_repo`
  字段、一个建造器、N 个 `_in_tx` 方法。把方法体提进同文件的泛型写入方，文件即彻底干净。
- 单次收益最高的是 **dispatch**：4 个方法读的 `self.order.order_tx_repo` / `member_tx_repo`
  声明在 `dispatch_service/mod.rs`，而那个文件的 sqlx 面恰好就是一行 import 加这两个字段。
  一次提取同时清掉 `mod.rs`、`order_lifecycle.rs`、`helpers_validation.rs` 三个清单条目。
- `domain_action_executor` **必须放最后**：`execute_in_tx` 把同一个句柄扇给 12 个 `*_in_tx`。

扫描口径提醒：清单检测器是**朴素子串扫描**，匹配的是类型名而不是字段类型，也不处理注释。
`flight_runtime_service/timeline.rs` 曾经持有两个事务却从未进过清单，因为它通过 `self.pool`
拿池子。这是扫描的局限；**不要**为此扩充模式表——那就是新造一份清单，本计划明令禁止。

### 顺带发现：P1 删掉的模式在别处活得很好

P1 清掉了 `DispatchService` 的 26 个 `Option<Arc<dyn …>>`，但**同一个模式在 application 生产代码里还剩 82 处**，
分布在约 35 个服务上，配套 37 个 `with_*_repository` 建造器。其中 6 处正好是事务仓储
（`anomaly_service.rs:19`、`notification_service/service.rs:37`、`business_case_service/service.rs:28`、
`todo_service.rs:35`、`flight_service.rs:37,40`、`flight_runtime_service/types.rs:27`、`ai_job_service.rs:228`）。

这**不算** P1 失败——P1 的范围就写明是 `DispatchService` 的 26 个，它确实归零了。
但失败判据第 3 条把三个月后的复测**只**指向 `dispatch_service/mod.rs`，这个范围现在看太窄：
它让同一个反模式在别处的存活完全不进入测量。判据不追溯修改，所以这里只记录事实，
并给出正确的复测口径供下一轮使用：

```powershell
# 现值 82（2026-08-25）。降不下来就说明「删 Option」只是在一个服务里做了一次，不是修好了。
Select-String -Path services\api-server\crates\application\src\**\*.rs -Pattern 'Option<Arc<dyn' | Measure-Object
```

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

> **修正（2026-08-25，`e5f57bf` 已核实）。** 上一段解释了别名 trait 为何能存在，但它默认了一个错误前提：
> **HRTB 本身是必要的**。它不必要。`sqlx::Pool::begin()` 的返回类型是
> `Transaction<'static, DB>`（`sqlx-core-0.8.0/src/pool/mod.rs:378`），而 `Transaction<'c, DB>` 的
> `'c` 是**借来的**连接的生存期（`MaybePoolConnection<'c, DB>`，`transaction.rs:71-77`）。
> 从池取出的事务自己拥有 `PoolConnection`，所以 `'static` 是它的**真实类型**，不是放宽或变通。
> 也就是说这 18 个 `for<'tx>` 从第一天起就在为一个不存在的多态性付费——
> 别名 trait 是给自造的问题做的解法。
>
> 18 个 `for<'tx>` 已全部删除。代价是 23 处事务签名要显式写 `'static`：签名里带全新省略生存期的方法
> 会在传给要求 `'static` 的被调方时报 `E0521 borrowed data escapes outside of method`，
> 沿调用链一路上溯到 `pool.begin()` 才收敛（`order_lifecycle.rs` 3 处起，共 13 个文件）。
>
> `sqlx_transactional_repositories.rs` **暂时保留**。它已不承重，但在 `UnitOfWork` 落地之前删掉它，
> 只会把 `Transaction<'static, Postgres>` 从 1 个文件喷到 10 个以上文件里——那是让债务变分散，不是变少。

### 做什么

在 domain 引入一个不含任何数据库类型的工作单元端口，让应用层持有 `Arc<dyn UnitOfWork>`；事务句柄以关联类型 / 不透明类型出现，`Transaction<'tx, Postgres>` 只在 infrastructure 内部具体化。`sqlx_transactional_repositories.rs` 从 application **删除**——若仍需别名 trait，它在 infrastructure 里重新出现，那里 `Postgres` 是合法词汇。

30 个文件按批迁移。`ai_runtime_service/in_memory_repos.rs`、`in_memory_ai_proposal_repository.rs` 这类命名带 in-memory 却引 sqlx 的，先单独看一眼——可能是纯粹的错放，能直接删依赖。

> **修正（2026-08-25，已按当前代码复核）。**
>
> **一、剩余面比「30 个文件」小，且形状不同。** 应用层生产代码里的 sqlx **只有类型，没有 SQL**：
> `sqlx::query*` 一共 47 处，全部落在 2 个 `#[cfg(test)]` 文件
> （`ai_action_proposal_service/tests.rs`、`domain_action_executor/tests.rs`）。
> 生产侧要处理的是 **13 个文件、36 处 `PgPool`** 加上 `Transaction` / `Postgres` 出现在签名里。
> 这正好是 `UnitOfWork` 端口能覆盖的形状——不需要先把 SQL 搬回 infrastructure。
>
> **二、`sqlx` 的 DoD 要分开说，否则它会变成漏洞。** 要删的是 `[dependencies]` 那行（`Cargo.toml:22`）；
> `[dev-dependencies]` 那行（`:66`）**合法保留**——测试知道 Postgres 没有问题，上面那 2 个测试文件就靠它。
> 这里明确写下来，是为了防止将来有人把「dev-dep 还在」当成没做完，或反过来把「只删生产那行」当成钻空子。
>
> **三、打样对象选错了。** 原文说 `DomainEventOutbox` 扇出最小，实测**它最大**：36 处引用 / 15 个文件。
> 真正最小的是 `DispatchOrder` 与 `DispatchOrderMember`，各 6 处 / 3 个文件。实测全表：
>
> | 别名 trait | 引用处 | 文件 |
> |---|---|---|
> | `DomainEventOutbox` | 36 | 15 |
> | `Flight` / `Todo` | 13 | 5 / 7 |
> | `FlightTimeline` | 12 | 5 |
> | `Notification` / `BusinessCase` | 10 | 6 |
> | `Anomaly` / `Ontology` | 7 | 3 |
> | `DispatchOrder` / `DispatchOrderMember` | 6 | 3 |
>
> 打样改用 `DispatchOrder`。这个更正不只是换个名字：按原文在扇出最大的 trait 上打样，
> 会让「端口设计是否成立」和「36 处迁移是否出错」两件事混在同一个 PR 里，
> 一旦失败就分不清该记「设计不成立」还是「这一版改错了」——而 P3 的风险条恰恰要求失败时如实归因。

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

1. `application_boundary_inventory.rs` 的 `expected` 数组**因新增代码**而变长过。

   > **修订（2026-08-25）。** 原文是「数组变长过」，没有例外。P3 触发了一种原文没预见的情况：
   > `DEBT_PATTERNS` 本身漏检，收紧模式后**既有**的隐藏债务浮出水面，数组从 31 变成 32。
   > 这与「新增债务后扩基线消红」是两件事，但原判据不区分，而判据的意义就在于事后不能跟它讲道理。
   > 所以这里不是「解释一下就过」，而是**修订判据并留档**：
   >
   > 因收紧 `DEBT_PATTERNS` 而暴露既有文件，不计入本条；但必须满足三个条件，否则仍算失败：
   > (a) 该提交**只做**「让守门说真话」这一件事，不夹带任何功能或重构改动；
   > (b) 提交信息点名新模式、新增条目、以及它为何是既有债务而非新债务；
   > (c) 在下方《豁免记录》里登记一行。
   >
   > **豁免记录**
   >
   > | 日期 | 提交 | 变化 | 新模式 | 暴露了什么 |
   > |---|---|---|---|---|
   > | 2026-08-25 | `60b68b5` | 31 → 32 | `"Sqlx"` | `dispatch_service/mod.rs:115,117` 两个公开字段类型是 `Arc<dyn Sqlx*TransactionalRepository>`，即被 Postgres 钉死的类型，却在 `Postgres` / `Transaction<` / `PgPool` 上一个模式都不命中——别名 trait 的发明目的正是把这些词从签名里藏掉，于是它同时也绕过了守门测试 |
   >
   > 如果这张表长出第二行、第三行，那本身就是失败信号：说明模式表是靠一次次「补漏」维持的，
   > 而不是一开始就照着债务的真实形状写的。
2. 出现了任何新的 `DEBT_PATTERNS`-式清单、新的基线快照测试、或新的 `#[allow]` / `noqa` 批量豁免。
3. 三个月后（2026-11-24）复测：`grep -c 'Option<Arc<dyn' dispatch_service/mod.rs` 仍 > 0，或 `grep -rn 'NullRepository'` 仍 > 0。
4. 本文档被更新成「已完成 P1，守门测试已就位」这类措辞——守门测试不是任何一步的完成标志。

第 3 条是可机械复测的，也是本节的核心。前 6 份计划之所以能连续失败 6 次而每次都看起来在推进，就是因为**没有任何一份写下过可被证伪的失败条件**。
