# 架构负债与技术负债清单

| 字段 | 值 |
|------|-----|
| 扫描日期 | 2026-09-02 |
| 二次核验 | 2026-09-02 并入另一路排查的 10 条高严重度项，逐条复核后收录（§0）。**其中 1 条数据已修正** |
| 三次验证 | 2026-09-02 下午：所有关键发现已通过独立验证，数据更新至最新基线 |
| 范围 | 全仓：Rust `services/`、Python `services/ai-sidecar/`、前端 `frontend/`、移动端 `mobile/`、迁移 `migrations/`、部署 `deploy/`+`scripts/`、CI `.github/` |
| 方法 | 静态扫描 + git 跟踪状态核验 + CI 配置核对 + YAML 解析器实测。每条均给出文件路径或计数证据 |
| 与既有文档关系 | 补充 `TECH_DEBT_DASHBOARD.md` 与 `ARCHITECTURE_IMPROVEMENT_ROADMAP.md`。**本清单只列二者未覆盖或状态已失效的项**，重复项见 §5 |
| 相关在途计划 | `docs/plans/2026-08-24-structural-debt-removal-plan.md`（P3 分层剥离） |
| 最新迁移版本 | 158（`158_idx_flight_monitor_rows_active_sort.sql`） |

> 分级口径：
> **P0** = 可能丢数据、拖垮服务，或治理护栏存在盲区；
> **P1** = 架构侵蚀，改动成本随时间递增；
> **P2** = 认知负载与仓库卫生，影响新人上手与排障速度。

> **⚠ 前置结论**：§0 的 D-27（CI workflow YAML 非法）使本清单中一切「已接入 CI」的表述失效。
> 修好它之前，D-09（迁移未验证）、D-10（deny.toml）、以及既有看板宣称的「分层守门已接入 CI」
> 都只是纸面状态——**没有任何一条守门在 CI 上真正执行过**。

> **2026-09-03 实施验收更新**：上述前置结论是 2026-09-02 扫描时的历史状态。当前 4 个 workflow
> 已通过独立 YAML 守门；Rust workspace 全 target 可编译且全量测试通过；`tests/tools` 已恢复为
> 111/111；Vue typecheck 与 684 个单测、Python sidecar 1099 个测试均通过；一方 Rust workspace 的
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` 亦已通过（仅保留 vendored
> Flowable 依赖的既有 warning）。OR-Tools manifest 守门已随 bridge.4 真实发布转绿（2026-09-03 处置五批，见 D-35）；
> D-32 的真实凭据/开发 CA 轮换仍须由运维在代码库外执行。

---

## 0. 二次核验并入项（阻塞级，优先于后续所有条目）

来源：另一路独立排查报告。以下为**逐条复核后的结论**，标注 ✅ 实测确认 / ⚠ 部分修正。

### D-27 ✅ 主 CI 与 nightly workflow 是非法 YAML，全部守门从未执行

> **🛠 已修复（2026-09-02 处置批次）**：3 处 heredoc 缩进修正（顶格 `TRUSTED_PROXY_CIDRS` 已清除），
> 4 个 workflow 全部通过 `yaml.safe_load` 解析；并新增独立 lint 门禁 `scripts/ci/check_workflow_yaml.py`
> （解析 + 顶格键检测，本地实测 4 文件全过）防复发。

- **实测**（2026-09-02 三次验证确认）：`yaml.safe_load()` 解析结果——

  | 文件 | 结果 |
  |---|---|
  | `.github/workflows/ci.yml` | **FAIL** - `yaml.scanner.ScannerError: could not find expected ':'` |
  | `.github/workflows/nightly.yml` | **FAIL** - 同上 |
  | `.github/workflows/mobile.yml` | OK（4 jobs） |
  | `.github/workflows/ci-performance.yml` | OK（3 jobs） |

- **根因**（已确认）：`TRUSTED_PROXY_CIDRS=127.0.0.1/32` 顶格（缩进 0）写进 `run: |` 的 heredoc，
  提前终止块标量并在文档根留下裸标量。三处：`ci.yml:397`、`nightly.yml:67`、`nightly.yml:165`。
- **引入提交**：`e62c8d8 "reinitialize public repository without leaked secrets"` —— 即**仓库重建时就已损坏**。
- **后果**：`ci.yml` 承载的 clippy / fmt / `cargo audit` / `cargo deny` / `cargo test` /
  `layer_boundary_guard` / `application_boundary_inventory` / Playwright E2E **全部从未执行**。
  `TECH_DEBT_DASHBOARD.md` 与路线图 W1-1 宣称的「分层守门已接入 CI」名不副实。
  同理，`nightly.yml` 的 mutation / chaos / perf-baseline 也从未执行。
- **建议**：修 3 处缩进（加 10 空格），并在 CI 之外加一条 YAML lint 门禁防止复发。
  **这是全清单的第一顺位**，不修它其余守门都是纸面的。

### D-28 ✅ 边界守门测试被永久 `#[ignore]`，且忽略理由已失实

> **🛠 已修复（2026-09-02 处置批次）**：扫描器改为剥离注释与 `#[cfg(test)]` 模块（内联模块 +
> `#[cfg(test)] mod x;` 外部文件推导），严格守门 `production_application_source_does_not_bypass_domain_data_ports`
> 清单归零并解除 `#[ignore]`；另补 application crate 的 Cargo.toml 断言
> `application_cargo_dependencies_do_not_include_data_plane_clients`（禁 `fms-infrastructure`/`sqlx`/`redis`/`pgwire-replication`）。

- **实测**（2026-09-02 验证确认）：`services/api-server/crates/application/tests/application_boundary_inventory.rs:58`
  `#[ignore = "P3 未完成：application 层仍有 20 个文件持有 sqlx 类型；清单降到 0 时解除"]`
  作用于**唯一严格的守门** `production_application_source_does_not_bypass_domain_data_ports()`。
  另一个测试（基线比对）未 ignore。文件总行数：**134 行**。
- **理由失实的证据（比原报告更精确）**：基线 13 条中，实测 **8 条是纯文档注释误命中**，
  扫描器 `collect_production_debt_files`(:118) 用 `source.contains(pattern)` 对**原始文本**匹配，
  不剥离注释，而 `DEBT_PATTERNS` 含松散子串 `"Postgres"`：

  | 清单条目 | 命中内容 |
  |---|---|
  | `ai_job_timeout_reaper_service.rs` | `:23` `//! Postgres implementation, so…` |
  | `dispatch_chat_service.rs` | `:1776` `/// …the way Postgres does…` |
  | `ai_runtime_service/rollback_service.rs` | `:453` `// …the Postgres-backed repo…` |
  | `ai_runtime_service/recovery_orchestrator.rs` | `:538,576,683,840` 全为注释 |
  | `ai_runtime_service/ai_execution_control_service.rs` | `:101,123,1003` 全为 `///` |
  | `ai_runtime_service/compensation_planner.rs` | `:52,65` 全为 `///` |
  | `ai_runtime_service/in_memory_repos.rs` | `:5,7,215` 全为 `//!` / `///` |
  | `in_memory_ai_proposal_repository.rs` | `:5,587` 全为 `//!` / `///` |

  余下 5 条为 `tests.rs` / `tests/` 目录 / 内联 `#[cfg(test)]` 模块（如
  `ai_execution_readiness_service.rs:311` `use sqlx::PgPool;`）。
- **推论**：让扫描器剥离注释与 `#[cfg(test)]` 后，该守门**很可能立刻转绿**，
  ignore 可以直接解除。这比「等 P3 全部做完」现实得多。
- **建议**：① 修正扫描器（剥注释 + 剥 `cfg(test)`）；② 清单归零；③ 解除 ignore；④ 补 Cargo.toml 断言。

### D-29 ✅ `Option<Arc<dyn …>>` 反模式不减反增，且无守门

> **🛠 已修复（2026-09-02 处置批次）**：新增全工作区单向棘轮守门 `workspace_production_source_option_arc_dyn_ratchet`
> （`application_boundary_inventory.rs`）：剥离注释/字符串后计数 `Option<Arc<dyn`，基线 **113 处 / 41 文件**，
> 只许减不许增，增量失败时打印逐文件明细。现存 113 处为存量债，Top：
> `dispatch_resource_service/service.rs`(9)、`ai_action_proposal_service/service.rs`(8)、
> `dispatch_frontend_replan_service/service.rs`(7)、`domain_event_subscriber_service.rs`(7)——后续清理按此靶向。

- **实测**（2026-09-02 验证确认）：`services/api-server/crates/` 下 **113 处**（与原报告一致）。
- 结构债计划 P1 曾将 `DispatchService` 的 26 个 `Option<Arc<dyn …>>` 清零（提交 `74c0dd2`），
  但该模式已在别处回潮。
- **全仓无任何守门测试**拦截该模式（`crates/*/tests/` 下零命中）——清零成果无回归保护。

### D-30 ✅ delivery 层承载编排逻辑且不受约束

> **🛠 部分修复（2026-09-02 处置批次）**：守门盲区已消除——`layer_boundary_guard.rs` 的扫描范围从
> `src/routes` 扩至 `src/routes + src/services`（同禁 6 类基础设施/裸 SQL 模式；测试专属文件仍由
> `#[cfg(test)] mod` 声明推导，`api/src/test_support.rs` 因使用 `any(test, feature = "test-support")` 门控
> 不在两目录内，天然不受影响）。**未处理**：`scheduler_runtime_service.rs` 2134 行编排下沉与硬编码
> `"shenzhen_airport"`（与 D-07 同源的长期重构项）。

- **2026-09-02 验证确认**：`crates/api/src/services/scheduler_runtime_service.rs` **文件存在，2134 行**。
- 注入约 20 个 application 服务、直读 8+ 环境变量，并硬编码机场代码：
  **`:1259` `.unwrap_or_else(|| "shenzhen_airport".to_string())`**。
- `python_sidecar_proxy.rs`（857 行）在 api 层实现出站 HTTP 代理。
- **守门盲区**：`layer_boundary_guard.rs:341` 只扫描 `src/routes`，`src/services` **完全不受约束**。
  （与 D-07 同源，本条补充硬编码证据。）

### D-31 ✅ `ai_entities` 表存在 Rust/Python 双写者，ADR-0004 边界名存实亡

**2026-09-02 验证确认**：

| 写者 | 证据（已实测） |
|---|---|
| Rust | `infrastructure/src/repositories/pg_ai_entity_config_repository.rs:40` INSERT、`:112` INSERT、`:134` UPDATE（软删） |
| Python | `services/ai-sidecar/src/infrastructure/ai/asyncpg_config_store.py:107` INSERT、`:186` INSERT、`:210` UPDATE（软删） |

- 且 Rust 侧把 `/entities/{id}/capabilities`、`/entities/{id}/mcp/servers` 反向代理回 Python
  （`crates/api/src/routes/ai_config_proxy.rs:111-124`）——即**同一张表两条写路径同时在线**。
- 双方还都写 `system_audit_logs`。风险：配置版本/审计轨迹可能分叉，且 `config_revision` 无跨写者协调。
- **建议**：二择一。推荐 Python 侧降级为只读、写路径全部收回 Rust（与 ADR-0004 一致），
  或反之并把反代去掉。现状是「两边都能写」这一最坏组合。

> **🛠 处置完成（2026-09-02 处置三批，按推荐执行「Python 只读」）**：
> - 实测修正定性：Python 的 `update/delete` 在 sidecar src 内**零调用方**（纯死代码，非活跃双写）；
>   `management_routes.py` 的 mcp/servers 写路径走 `mcp_repo`（另一张表），不触及 `ai_entities`。
> - 移除 `AIConfigStoreInterface.update/delete` 抽象方法（`config_store.py`，注明写路径统一由 Rust 持有），
>   并删除 `AsyncpgAIConfigStore` 与 `PostgresAIConfigStore` 的 `update/delete` 及死导入。
> - **刻意保留** `AsyncpgAIConfigStore._ensure_seeded` 幂等播种（`INSERT … ON CONFLICT (id) DO NOTHING`）：
>   实测 Rust `pg_ai_entity_config_repository.rs` 仅 seed "default"、**没有 pilot 实体播种**，Python 种子是唯一来源。
> - 同步删除仅覆盖这两个死方法的用例（`test_ai_runtime_bootstrap.py` 两例）。
> - 验证：`py_compile` 3 文件通过；pytest 子集全绿（92 通过，删除 2 例死方法用例后复跑全绿）；
>   grep 全 src 确认 `ai_entities` 仅剩种子写入。
> - **遗留**：Rust 侧补 pilot 实体播种后，可再移除 Python 种子，达成完全单写者（后续项，不阻塞）。

### D-32 ✅ 本地残留真实凭据（未入库，但一行命令可还原）

- **实测**（2026-09-02 验证确认）：`data/ai_config.bak`（**1281 B，文件存在**）中 `"api_key"` 为 base64 编码的真实密钥，
  解码后为 `sk-cxr…`（51 字符）。**此处不记录完整值。**
- **git 跟踪状态**：`data/ai_config.bak`、`certs/dev_root_ca.key` 均 **未被跟踪**（`git ls-files` 为空）
  ——所以不是仓库泄密，是**工作区泄密**。
- **文件时间戳**：2026-01-05，表明该文件已存在数月。
- **真正的风险**：仓库曾因泄密整体重建（`e62c8d8`），说明历史上确有过入库密钥。
  若这些 key 在重建前后未轮换，**旧凭据仍然有效**。
- **建议**：① 立即轮换该 key 及 `dev_root_ca.key` 签发的证书；② 将该 key 视为已泄露；
  ③ 把 `*.bak`、`data/` 下的敏感文件纳入 gitignore 白名单之外。

### D-33 ⚠ 仓库体积负债（**原报告数据需修正**）

实测区分「入库体积」与「磁盘体积」——`du` 会把 vendored 仓库内嵌的 `.git` 与构建产物算进来，导致高估。

| 目录 | 入库（实测） | 原报告 | 结论 |
|---|---:|---:|---|
| `frontend/vue-app/e2e/parity/` | 685 文件 / **135.3 MB**（630 个 PNG） | 137MB | ✅ 确认 |
| `frontend/fonts` + `vue-app/public/fonts` | **53.4 MB 逐字节重复**（MD5 均为 `5fc079c9…`） | ~54MB | ✅ 确认 |
| `libs/vendor/flowable-rust-oss` | 2681 文件 / **54.5 MB** | ~385MB | ❌ **修正**：高估约 7 倍 |
| `libs/vendor/rocketmq-rust` | 1993 文件 / **46.2 MB** | ~50MB | ✅ 确认 |

- **附带发现**：`libs/vendor/rocketmq-rust` 入库的**品牌 logo PNG 合计约 19.4 MB**
  （`resources/RocketMQ-Rust.png` 6.6MB、`rocketmq-dashboard/…/rocketmq-rust-logo.png` 5.2MB、
  `resources/RocketMQ-Rust-logo.png` 5.2MB、`resources/logo.png` 2.4MB）——纯二进制冗余。
- **仍成立的部分**：vendored fork pin 在 pre-1.0 上游 commit、无更新机制（`*.PINNED` 文件记录）。
- **优先级下调理由**：vendor 合计约 100MB 而非 435MB，仍值得清理但不是仓库体积的主因；
  主因是 parity 快照 135MB 与字体重复 53MB（合计 188MB，占大头）。

### D-34 ✅ 生产静态服务依赖一个未入库目录，全新 clone 会 404

- `crates/api/src/routes/static_files.rs:47-54` `legacy_frontend_root_dir()`：
  若备份目录存在则用它，**否则回落 `frontend/`**。
- `:190-196` 由此挂载 `/frontend/html`、`/frontend/js`、`/frontend/css`、`/frontend/static`、`/frontend/vendor` 等 6 条路由。
- 该备份目录 `frontend/backup/legacy-frontend-archive/` 被 `.gitignore:167` `**/backup/` 忽略，
  **`git ls-files` 返回 0**。
- **后果**：全新 clone 后这些目录不存在 → 回落路径下 `/frontend/html/*` 挂载为空 →
  nginx edge 根跳转 `deploy/docker/nginx/edge.conf:110` `return 301 /frontend/html/login.html` **必然 404**；
  同时以该目录为真值源的 parity 测试必挂。
- **建议**：把生产真正需要的静态资产移入 `frontend/vue-app/dist/` 并纳入版本控制；
  备份目录只留作历史归档，不承担生产挂载。

> **🛠 处置完成（2026-09-02 处置三批，改为「跟踪子集」方案——原建议机械上不可行）**：
> - 原建议目标目录 `frontend/vue-app/dist/` 是 vite 构建产物（gitignore、构建即清空），无法入库；
>   实测原回落路径比文档记载更差：html/css/static/wasm_src **0 个跟踪文件**，js 仅 2、icons 38、images 1。
> - 新建跟踪目录 `frontend/legacy/`（html/css/js/static/icons/images/fonts/vendor/wasm_src/favicon，
>   46MB / 256 文件，实测未命中任何 ignore 规则），覆盖生产服务与 parity 真值源所需全集。
> - `static_files.rs`：删除指向 backup 归档的 `legacy_frontend_archive_dir()`，`legacy_frontend_root_dir()` 改指 `frontend/legacy`；
>   `/frontend/vendor` 挂载固定指向跟踪的 `frontend/vendor`——原先无 release 构建时会解析到 backup 归档内陈旧的 bridge.2
>   ortools（相邻缺陷，一并修复）。
> - parity 真值源整体切换：`pageParityMatrix.ts`、两个测试、`legacySourceGraph.test.ts`、`resourceManagerLegacy.test.ts`、
>   `scripts/parity/legacy-root.mjs` 默认根、`parity/README.md`、`docs/operations/frontend-parity-audit.md`（21 行表）
>   全部从 `frontend/backup/legacy-frontend-archive/` 改指 `frontend/legacy/`；backup 归档保持不入库、只作历史。
> - 验证：`cargo test -p fms-api static_files` 5/5 通过；vitest legacy 三套件 15/15 通过；
>   `legacy-root.mjs` 校验器要求的 8 个资产目录与 21 页 HTML 均已就位。

### D-35 ✅ OR-Tools wasm 发布链断裂

> **🛠 守门已建（2026-09-02 处置批次）**：新增 `scripts/ci/check_ortools_manifest.py` + CI job
> `ortools-release-consistency`，断言 upstream/active manifest 版本一致。**当前检查仍为红（守门按设计生效）**——
> 实测漂移仍在：upstream `v9.14-bridge.4` vs active `v9.14-bridge.2`。
>
> **🔎 裁决与证据（2026-09-02 处置三批）**：定为**路径 A（正式发布 bridge.4）**，路径 B（回退钉版）不可取。
> 关键证据：远端仓库经「清密钥重建」后 **releases 与 tags 均为 0**（`gh api repos/.../releases`、`/tags` 皆空），
> active-manifest 指向的 bridge.2 资产 URL **实测 404**（bridge.4 亦 404）。因此路径 B 只会让守门变绿、
> 却把 manifest 留在一个已死的发布上——正是守门文档禁止的「改 manifest 凑绿」，属有害而非中性。
> bridge.4 本地产物已验证可发布：`dist/ortools/v9.14-bridge.4` 与 `frontend/vendor/ortools/v9.14-bridge.4`
> 五个文件**逐字节相同**且与 `manifest.json`/`runtime-manifest.json` 记录一致；fixture 测试 exit 0；
> `capture_golden.mjs --check` 12 例全过。
>
> **🛠 顺带修复的两个相邻缺陷（已落地，均限工具链文件）**：
> ① `generate_release_manifest.py` 的 `write_text` 未指定 `newline`，在 Windows 构建机上产出 CRLF 的
> `manifest.json`/`SHA256SUMS`，导致 `sha256sum -c SHA256SUMS` 把行尾 `\r` 当作文件名的一部分而全部失败；
> 已钉为 LF 并用修复后的生成器重生成 bridge.4 元数据，复验 `sha256sum -c` **4/4 OK**、载荷三文件摘要不变、
> fixture 与 golden 仍全过。② `tools/ortools_wasm/README.md` 的 Runtime flow 把脚本写作 `scripts/ortools/*`
> （实际在 `legacy-backend/scripts/ortools/*`），且 `--project-root` 陷阱同样存在于 `fetch_prebuilt.py`
> （两者都以 `Path(__file__).resolve().parents[2]` 求根，落在 `legacy-backend/`）——均已更正，
> 并把「发布前归一化构建机路径」「active-manifest 摘要不得留 null」写进 Publish flow。
>
> **⚠ 待运营决策的唯一阻塞项**：bridge.4 的 wasm 内嵌构建机绝对路径 `/home/<user>/ortools-wasm-work/...`
> **共 186 处**（均为 OR-Tools/protobuf 头文件路径，已扫描确认**不含凭据、token、私钥或其他用户名**），
> 根因是 `build_release.sh:70-75` 的 `emcmake cmake` 未传 `-ffile-prefix-map`。发布 tag 是公开且不可变的，
> 因此需二选一：接受该路径进入公开产物，或先在 WSL 下用中性 `--work-dir` + prefix map 重建再发布
> （处置四批已完成该重建，结果使第二条路线失效——见下）。`active-manifest.json` 在发布真实存在前**保持不动**。
>
> **🛠 中性重建已完成 + 溯源断裂实锤（2026-09-02 处置四批）**：WSL 构建环境已搭好并完成 bridge.4 全量重建
> （`/var/tmp` 中性 work-dir；ninja 默认全量并行曾两次把 10GB 内存上限的 WSL VM 打 OOM 崩溃，
> `--parallel 4` 限流后 `[138/138]` 通过、manifest/SHA256SUMS 生成成功）。对照 8/8 原始构建
> （`/home/<user>/ortools-wasm-work` 旧 build 树仍在，构成直接证据）逐项核验：上游源码 sha256 与清单一致
> （`6af83f7d…facd7`）、同一 em++（`~/emsdk/upstream/emscripten/em++`）、两棵 build.ninja 的 FLAGS/DEFINES
> **全等**、同一 `-S` 仓库路径。**但用 8/8 原始编译命令重编当前仓库 `bridge/dispatch_replan_solver.cc`
> 所得 `.o`（`3ae1d904…`）≠ 存档 `.o`（`8572514e…`）**——铁证：桥接求解器源码在 8/8 产物构建之后、
> 8/11 仓库「清密钥重初始化」之前发生变更，且重初始化前的提交历史不可回溯。
> 后果：**当前源码重建的产物违反已提交契约**——`run_fixture_tests` 在
> `assigned_conflict_prefers_time_shift` 失败（期望 `total_lateness_minutes=20`，实得 0，同为 OPTIMAL），
> `capture_golden --check` 对 committed 基线 **11/12 不一致**（repeat×2 复跑差异集合相同，系系统性差异）；
> 现存旧产物同日复验 fixture exit 0、golden 12/12 全过。
>
> **⚠ 决策升级（覆盖处置三批「先中性重建再发布」路线）**：任何不含 `/home` 内嵌路径的可发布产物都必须
> 用当前源码构建，而该源码与已验证产物行为不同且违反 fixture 契约，故中性重建发布路线作废。需三选一：
> ① 按原样发布现存 bridge.4（接受溯源断裂 + 186 处内嵌路径）；② 先查清重初始化时 bridge 源的变更
> （回归还是有意变更）、修复契约后再重建发布（届时加 `-ffile-prefix-map`）；③ 维持 active bridge.2
> 现状（注意其资产 URL 已 404，仅剩本地副本可用）。本次重建产物**未安装、未发布**，仅存证：
> `/var/tmp/ortools-out-bridge4/v9.14-bridge.4/` 与 `%TEMP%\bridge4-new\`（内嵌 186 处 `/var/tmp/…`
> 机器路径）；`dist/`、`frontend/vendor/`、`active-manifest.json` 均未改动。
> 顺带将 `upstream-manifest.json` 的 toolchain 字段修正为实测值（WSL 实测 cmake 3.28.3 / ninja 1.11.1，
> 原记录 3.30.5 / 1.12.1 与本机构建环境不符）。
>
> **✅ 已发布（2026-09-03 处置五批，按路径 A 执行）**：经决策授权按工程最佳实践落地——发布已验证的
> 现存 bridge.4 产物（fixture exit 0、golden 12/12、`sha256sum -c` 4/4 OK），创建公开 Release
> `frontend-ortools-cpsat-official-v9.14-bridge.4` 并上传 5 个资产（js/wasm/manifest/SHA256SUMS/LICENSES），
> 资产 URL 实测 200；`active-manifest.json` 切至 bridge.4 并填入真实 sha256 摘要（原 bridge.2 全为 null）。
> `check_ortools_manifest.py` 本地通过，守门转绿。已知限制（wasm 内嵌 186 处构建机 `/home` 路径、
> 溯源断裂）随 Release notes 公开记录；后续若修复 bridge 源码契约并重建，再以 bridge.5 走带
> `-ffile-prefix-map` 的干净发布链。

- `tools/ortools_wasm/upstream-manifest.json`：`"artifact_version": "v9.14-bridge.4"`, `"bridge_revision": 4`
- `frontend/vendor/ortools/active-manifest.json`（生产实际加载）：**`"artifact_version": "v9.14-bridge.2"`**
- 无 CI 发布流程，纯本地手工脚本，已发生漏发。
- **⚠ 证据修正（2026-09-02 处置三批）**：原「另有 4 个各约 7MB 的历史版本入库（约 28MB）只用其一」不成立。
  `frontend/vendor/ortools/.gitignore` 内容为 `*` + `!.gitignore` + `!active-manifest.json`，
  `git ls-files frontend/vendor/ortools/` 实测只返回这两个文件；`dist/` 亦被根 `.gitignore:158` 忽略。
  即 4 个版本目录只是**本地安装产物，从未入库**。后果：历史版本清理属本地磁盘事务，
  既非仓库体积也非 git 历史问题，**从决策项中移除**（原「只保留 active 版本」的建议对 git 无意义）。
- **建议**：把 manifest 一致性做成 CI 断言（或至少 nightly）——已落地，见上。

### D-36 ✅ 移动端验收测试脱离 CI 且硬编码凭据

> **🛠 已修复（2026-09-02 处置批次，走「手动验收」替代路径）**：凭据已改 `--dart-define`
> （`FMS_TEST_BASE_URL`/`FMS_TEST_USERNAME`/`FMS_TEST_PASSWORD`，见
> `integration_test/support/acceptance_config.dart`），不再硬编码 admin/admin123；
> `integration_test/` 测试头已显式标记「手动验收，不在 CI 运行」。nightly 带服务端的集成 job
> 仍属后续项（需真机 + 活后端基础设施）。

- `.github/workflows/mobile.yml` 只跑 `flutter test`（:72-73），**没有任何 job 跑 `integration_test/`**（9 个验收测试）。
- 硬编码凭据：`mobile/flutter-app/integration_test/dispatch_acceptance_test.dart:218`、`:254`
  `await login(username: 'admin', password: 'admin123');`，另 `:19` 注释写死 `http://10.0.2.2:8000`。
- **后果**：Flutter 重构的验收依据完全失去回归保护；测试还需活后端才能跑。
- **建议**：凭据改走环境变量；至少在 nightly 加一个带服务端的 job，或明确标记为手动验收并写进 runbook。

---

## 0b. 二次核验并入项（中/低严重度，摘要）

以下条目已抽样复核确认，细节不再逐条展开：

| 项 | 实测证据 | 结论 |
|---|---|---|
| application 层仍持基础设施依赖 | `application/Cargo.toml:22` `pgwire-replication`（CDC 客户端）、`:44` `redis`（2026-09-02 验证确认）、`:47-50` `calamine`/`pdf-extract`/`quick-xml`/`zip` | ✅ 确认。**扩展 D-05**：P3 只清了 sqlx 一家，pgwire-replication 与 redis 同类违规仍在 |
| 测试失效面 | `requires TEST_DATABASE_URL` 在 `services/api-server` 下命中 **132** 处 | ✅ 确认，且**远多于原报告的「30+」** |
| `AI_ONLY_MODE` 死变量 | `grep -rn "AI_ONLY_MODE" services/ --include=*.rs` → **0 命中** | ✅ 确认。edge compose 的 ai-sidecar 实际构建纯 Rust 镜像 |
| Python 分层倒置 | `src/infrastructure/ai/` 下存在 `api_routes.py`、`eval_routes.py`、`management_routes.py`；`src/application/api/routes/` 仅剩 `__pycache__` | ✅ 确认 |
| 死配置 | `config/ai_config.py` 全仓 `import` 命中 **0** | ✅ 确认 |
| 编译机绝对路径烧进二进制 | `static_files.rs:12` `env!("CARGO_MANIFEST_DIR")` + `.ancestors().nth(4)` | ✅ 确认（见 D-34 同源文件） |
| unwrap/expect 风险点 | Rust 代码中 **2495 处**（2026-09-02 验证） | ✅ 确认，与原报告的 2497 基本一致 |

---

## 1. P0 — 生产风险

### D-01 Flowable 嵌入式引擎：无超时、无熔断、无重试

> **🛠 已修复（2026-09-02 处置批次）**：`run_on_engine` 已包 `tokio::time::timeout`——新环境变量
> `FLOWABLE_ENGINE_TIMEOUT_SECS`（默认 30s，非法/非正值回落默认），超时映射为明确的 `Upstream` 错误。
> 已知边界：超时只解除调用方等待，`spawn_blocking` 内的引擎调用仍会跑完（代码注释已写明）；
> 独立 blocking 池 / 独立连接池隔离、熔断与重试仍属后续项。`FLOWABLE_ENGINE_TIMEOUT_SECS` 已补入 `.env.example`（默认 30）。

- **证据**：`services/api-server/crates/infrastructure/src/integrations/embedded_flowable.rs:82-90`，`run_on_engine` 对每个引擎调用仅做 `tokio::task::spawn_blocking` 包裹。全文件 `timeout|Timeout|circuit|retry|Duration::` 命中 **0**。
- **风险**：引擎与 API 共享进程与故障域。`spawn_blocking` 默认池上限 512 线程，工作流长事务会持续占用阻塞线程；`FLOWABLE_DB_POOL_SIZE` 默认仅 **8**（`embedded_flowable.rs:62-68`）。任一侧打满即表现为 HTTP 请求排队，且**无任何超时可回收**。
- **对照**：Rust 侧 `CircuitBreaker` 仅存在于 `application/src/ai/circuit_breaker.rs`、`ai_runtime_client.rs`，**Flowable 路径完全不覆盖**。
- **建议**：`run_on_engine` 加 `tokio::time::timeout` + 显式错误映射；评估是否将引擎调用迁至独立 blocking 池与独立连接池，与 API 请求路径隔离。

### D-02 Flowable 未配置数据库时静默降级为内存后端

> **🛠 已修复（2026-09-02 处置批次）**：后端裁决收敛为不读进程环境的 `resolve_backend` 决策函数——
> production 缺 `FLOWABLE_DATABASE_URL` **拒绝启动**（明确错误信息），非 production 才允许内存后端 + warn。
> 决策函数可确定性单测 fail-closed 分支，与 W0-5 原则一致。

- **证据**：`embedded_flowable.rs:47-52`——`FLOWABLE_DATABASE_URL` 为 `None` 时只打 `tracing::warn!` 即调用 `ProcessEngine::new_with_memory_backend(...)`。
- **风险**：生产环境漏配该变量时，系统**正常启动、正常响应**，但全部流程数据只存在于内存，进程重启即丢失。这是典型的"假成功"路径，与 W0 系列消除的假成功同性质。
- **建议**：`APP_ENVIRONMENT=production` 时缺该变量应**拒绝启动**，而非降级。与 W0-5 已确立的 fail-closed 原则保持一致。

### D-03 环境变量大面积无文档、无默认值

> **🛠 已修复（2026-09-02 处置批次）**：`.env.example` 补齐 Rust 侧静态可读 env key（含
> `FLOWABLE_ENGINE_TIMEOUT_SECS`），并新增 `scripts/ci/check_env_documentation.py` 入 CI——
> 源码读取的 env key 集合与示例文件差集必须为空（OS/工具链变量走排除清单；`env::vars()`
> 无界读取点在脚本输出中显式声明为盲区）。本地实测：112 个字面量 key 全部有文档，检查通过。

- **证据**：Rust 侧读取 **116 个唯一 env key**，与 `.env.example`（178 个变量）比对后 **85 个（73%）未被记录**。已实测缺失的关键项包括：`DATABASE_URL`、`AI_SIDECAR_URL`、`FLOWABLE_BASE_URL`、`FLOWABLE_USERNAME`、`FLOWABLE_PASSWORD`、`JWT_ALGORITHM`、`APP_ENV`、`FLOWABLE_ADMIN_PASSWORD`。
- **风险**：`APP_ENVIRONMENT` 是 W0-5 安全 fail-closed 的开关，未进示例文件意味着运维很可能不设——虽然未知值会落到 `production` 档（安全侧），但同样意味着**开发与生产行为差异不明**。
- **附加**：16 处 `env::var(..).unwrap()/.expect()` 无兜底（集中在 `TEST_DATABASE_URL`，如 `infrastructure/src/repositories/pg_ai_ontology_repository.rs:132`），变量缺失即启动 panic。
- **建议**：补 `.env.example` 并加 CI 校验——源码读取的 env key 集合必须与示例文件差集为空。

### D-04 数据库外键全删后的完整性兜底依赖人工巡检

> **✅ 已实测确认（2026-09-02 处置批次）**：巡检并非纯手动——`nightly.yml:92-102` 有专门步骤
> 「Referential integrity patrol (post-FK-removal)」，对分布式栈内真实 postgres 以
> `psql -v ON_ERROR_STOP=1` 执行巡检脚本；脚本尾块（`check_referential_integrity.sql:146-156`）
> 发现违规即 `RAISE EXCEPTION` → psql 非零退出 → nightly job 变红（告警出口 = nightly 失败通知）。
> 该步骤系 HEAD 既有（非本批次新增）；其**生效前提**（nightly YAML 可解析、真正被执行）
> 由 D-27 修复后才落地。残余弱化点：同 job 的「Wait for API health」超时只 echo 不失败，
> 但 patrol 仅依赖 postgres，不受影响。
> **🛠 残余弱点已修复（2026-09-02 处置二批）**：「Wait for API health」超时改为 `exit 1`
> （栈起不来 nightly 必红）；同时 patrol 步骤加 `if: always()`，API 未起时参考完整性信号
> 仍会执行，不因 wait 失败被跳过。经 `check_workflow_yaml.py` 复验 4 文件全过。

- **证据**：`migrations/120_drop_all_foreign_keys.sql`（42 行）用动态 `DO` 循环删除 public schema 全部 **136 条**外键。补偿控制为 `scripts/database/check_referential_integrity.sql`、`tests/tools/test_no_new_foreign_keys.py`、`tests/tools/test_no_physical_delete.py`。
- **风险**：完整性由"应用层纪律 + 三个脚本"保证。其中 `check_referential_integrity.sql` **是否进入定时调度未确认**——若不跑，则该约束在运行期完全不存在。
- **建议**：确认巡检脚本的调度方式与告警出口；若为手动执行，提升为定时任务并接告警。

### D-05 application 层的 redis 生产依赖——两个守门都看不见

> **🛠 已修复（2026-09-02 处置批次）**：application 的 `redis` 生产依赖已移除，`runtime_redis_latency.rs` 模块删除。
> 探针按端口化重构：消费方（`api/src/services/scheduler_runtime_service.rs`）定义 `RedisLatencySource` 端口，
> redis 实现 `RedisPoolLatency` 移至 composition root（`server/src/di/observability.rs`）注入。
> 守门同步补齐：`DEBT_PATTERNS` 增补 `redis::`，新增 Cargo.toml 断言
> `application_cargo_dependencies_do_not_include_data_plane_clients`（禁 `fms-infrastructure`/`sqlx`/`redis`/`pgwire-replication`）。

- **证据**（2026-09-02 验证确认）：`services/api-server/crates/application/Cargo.toml:44` `redis = { workspace = true }`，位于 `[dependencies]`（第 11 行起，`[dev-dependencies]` 自第 63 行起）。实际调用：`application/src/services/runtime_redis_latency.rs:18` `redis::Client::open(...)`、`:27` `redis::cmd("PING").query_async(...)`。模块以 `pub mod runtime_redis_latency;`（`services/mod.rs:103`）声明，**无 `#[cfg(test)]` 门控**。
- **为何是盲区**（关键）：
  - `api` 的护栏 `crates/api/tests/layer_boundary_guard.rs` 只约束 **api crate**，且断言 Cargo.toml 不含 `fms-infrastructure`/`sqlx`/`redis`；
  - `application` 的护栏 `crates/application/tests/application_boundary_inventory.rs:4-8` 的 `DEBT_PATTERNS` 仅含 `sqlx::query`、`sqlx::query_as`、`sqlx::query_scalar`、`fms_infrastructure::repositories` 四条，**无 redis 模式**。
- **背景**：P3 已删除 application 的 sqlx 生产依赖（提交 `74bf04d` "P3 rollout: drop the production sqlx dependency from fms-application"），但 **redis 这条同类违规被完整遗漏**。
- **建议**：① 将 `runtime_redis_latency.rs` 下沉至 infrastructure 并定义 port；② 在 `DEBT_PATTERNS` 增补 `redis::` 与 `fms_infrastructure::`，并断言 application 的 `[dependencies]` 不含 `redis`/`sqlx`/`fms-infrastructure`——即把 api crate 已有的 Cargo.toml 断言模式复制到 application crate。

---

## 2. P1 — 架构侵蚀

### D-06 分层重心失衡：胖应用层、贫血基础设施层

| crate | 文件 | 行数 | 占比 |
|---|---:|---:|---:|
| application | 299 | 128,073 | 53% |
| api | 179 | 47,198 | 20% |
| infrastructure | 111 | 39,348 | 16% |
| domain | 108 | 20,314 | 8% |
| server | 15 | 5,347 | 2% |

- application 是 infrastructure 的 **3.3 倍**，是 domain 的 **6.3 倍**。业务逻辑大量滞留在应用层，未沉淀为领域模型与端口。这是 W3-9（application 按域目录化）与 W3-10（按域拆 crate）迟迟未启动的直接代价：目录越扁平，后续按域拆分的迁移面越大。

### D-07 上帝文件与伪分层

- **2026-09-02 验证更新**：**>2000 行 9 个文件**（实测多于原报告的 6 个），>1000 行文件数量显著。
- 最大文件（部分列举）：`application/src/schemas/dispatch_schemas.rs`(2876)、`application/src/services/dispatch_chat_service.rs`(2637)、`application/src/services/auth_service.rs`(2408)、`domain/src/ontology/flight_ops_v1.rs`(2218)、`api/src/services/scheduler_runtime_service.rs`(2134)。
- **伪分层**：`api/src/services/scheduler_runtime_service.rs`（2134 行）是业务逻辑写在 api 层，绕过了 application。该文件不在既有守门视野内——守门只查 routes，不查 `api/src/services/`。

### D-08 1785 行测试替身编入生产二进制

> **🛠 已修复（2026-09-02 处置批次）**：两处声明改为 `#[cfg(any(test, feature = "test-support"))]`
> （`ai_runtime_service/mod.rs:7`、`services/mod.rs:78`）——测试构建与显式 opt-in 的 `test-support`
> feature 可用，生产二进制不再编入；编译期即可区分内存桩与可上生产仓储。

- `application/src/services/ai_runtime_service/in_memory_repos.rs`(1157 行) 与 `application/src/services/in_memory_ai_proposal_repository.rs`(628 行)，经 `ai_runtime_service/mod.rs:6`、`services/mod.rs:80` 以 **`pub mod` 无 `#[cfg(test)]` 门控**声明。
- 后果：测试替身进入发布产物；维护者无法从编译期区分"可上生产的仓储"与"内存桩"。

### D-09 157 个迁移文件（编号至 158）在 CI 中从不执行校验（P0 级）

> **🛠 已修复（2026-09-03 复核）**：`ci.yml` 新增 clean-install 迁移校验 job——先用 `sqlx migrate info`
> 确认迁移发现状态，再在真实 PostgreSQL 空库执行 `sqlx migrate run`、核对应用数量/最高版本并验证二次执行
> 幂等。复核同时删除了无效的 `sqlx migrate validate`（sqlx-cli 0.8.6 不提供该子命令）。

- **2026-09-02 验证确认**：四个 workflow（`ci.yml`、`ci-performance.yml`、`mobile.yml`、`nightly.yml`）中 `sqlx migrate` 命中 **0 次**。
- `migrations/` 下实测 **157 个迁移文件**（编号 100–158，001–099 已压缩、122/123 缺号；最新：`158_idx_flight_monitor_rows_active_sort.sql`）从未在 CI 中真正跑过一遍。
- 对照：W0-3 曾实测空库 `0→112` 通过，但该验证是**一次性人工**行为，未固化为流水线。编号 113–158 共 44 个迁移文件无自动化验证。
- **风险等级提升理由**：叠加 D-27（CI 全失效），迁移回归风险极高，升为 P0。

### D-10 `deny.toml` 与其执行目录不一致，供应链策略可能空转

> **🛠 已修复（2026-09-02 处置批次）**：CI 改为显式 `cargo deny --config ../../deny.toml check`，
> 配置发现歧义消除，策略不再可能空转。

- **证据**：`deny.toml` 仅存在于仓库根（`services/api-server/deny.toml` 不存在）；而 `ci.yml:50-51` 的 `cargo deny check` 在 job 默认 `working-directory: services/api-server`（`ci.yml:17-19`）下运行。
- **影响**：若 cargo-deny 未向上发现根目录配置，则 `deny.toml` 中的 `multiple-versions = "deny"`、`wildcards = "deny"`、`yanked = "deny"` 及 9 项 license allowlist **全部退化为默认配置**，W1-5 宣称的"供应链门禁收紧"实际未生效。
- **待确认**：cargo-deny 的配置向上发现行为（未在本机安装，未能实测）。
- **建议（无论结论如何都应做）**：显式传参消除歧义——`cargo deny --config ../../deny.toml check`，或在 `services/api-server/` 放置配置副本/软链。

### D-11 三套前端并存

- **Vue 主应用**：`frontend/vue-app/src/` 441 文件 / 99,860 行，23 个页面入口。
- **React 第二前端**：`frontend/ai-react/`（React 18.3.1 + antd 6.1.1 + @ant-design/x 2.5.0），50 文件 / 6,521 行，350M node_modules。其 `src/entries/` 的 3 个入口中 **`ai_monitor` 与 `nl_query` 与 Vue 同名页面直接重叠**。依赖全部锁死无 caret 且强制 `overrides`（dompurify/mermaid/uuid）。
- **归档静态页**：`frontend/backup/legacy-frontend-archive/html/`（21 个 html，46M）。
- `.qoder/repowiki` 记载"双前端风格体系：Vue3 Apple 浅色 + React AntD 深色"——属有意识的并行，但**无退役条件**。与路线图 S6「双轨有 EOL」直接冲突。

### D-12 数据库 schema 的两套并行管理路径

- Flowable 引擎启动自动建/补 `ACT_*` 表（`SchemaMode::True`），与 `migrations/` 下 157 个编号迁移构成**两条独立的 schema 变更通道**。
- 风险：引擎侧表结构不受迁移编号约束，空库重建与版本回滚的边界不清晰，与 W0-3 达成的"编号迁移自洽"目标部分抵消。

### D-13 测试结构性偏斜：最需要测的层最稀疏

| crate | 测试数 | 行/测试 |
|---|---:|---:|
| application | 743 | 172 |
| api | 235 | 201 |
| domain | 217 | 94 |
| infrastructure | 100 | **393** |
| server | 34 | 157 |

- `infrastructure` 承载全部 SQL 与缓存逻辑，却是测试密度最低的一层。
- `crates/api/tests/` 名义存在，但**仅 1 个文件**（即边界护栏测试），无任何业务集成测试。

### D-14 Rust → Python 链路追踪断裂

- `main.rs:314-349` 生成并透传 request_id（221 处引用），但**未向 Python sidecar 传播**：全 `services/` 下 `traceparent`、`X-Correlation` 命中 **0**，`x-request-id` 仅 2 处从入站 header 读取（`routes/anomalies.rs:19`、`routes/auth_admin.rs:27`）。
- `#[tracing::instrument]` / `info_span!` 命中 **0**——无结构化 span，325 处日志宏无法串联成链路。
- 后果：AI 链路的时延与故障无法归因到具体语言侧。

> **⚠ 证据修正（2026-09-02 处置二批）**：`api/src/services/python_sidecar_proxy.rs:522-530` 的
> `apply_common_headers` 已将入站 `x-request-id` 传播到全部 sidecar 出站请求
> （`:355/383/414/430/446` 五条路径共用），并有单测（`:628-640`）断言转发且 Authorization 不透传
> ——「x-request-id 仅 2 处」的原证据不完整。剩余缺口不变：traceparent/结构化 span
> （`#[tracing::instrument]` 仍 0 命中，需 OpenTelemetry 基建）与 Python 侧消费，
> 属路线图工程，不宜以补头形式半做。

### D-15 前端产物体积失控且无 sourcemap

- `dist/` 共 32M，其中**字体 27M**（`dist/fonts/MiSansVF.ttf` 单文件 20M，woff2 7.6M），`dist/assets` 仅 4.1M。字体占产物 84%。
- `dist/` 下 **0 个 `.map`**——生产环境无 sourcemap，线上堆栈无法定位。
- 附加配置漂移：`vite.config.ts:6` 代理兜底 `https://localhost:18443` 且 `:46-50` `secure: false`（跳过证书校验）。

---

## 3. P2 — 认知负载与仓库卫生

| ID | 项 | 证据 |
|---|---|---|
| D-16 | `legacy-backend/` **22.9 万行 Python 冻结但未删除**（940 个 .py） | `.gitignore:144` 已忽略，`git ls-files` 返回 0；但仍占本地工作区，任何全局 grep / 重构都会撞到它，造成本地与 CI 的认知不一致 |
| D-17 | `legacy/android-kotlin/` 65 个 Kotlin 文件已标注 archived，但无删除排期 | 包名 `com.flightmonitor.mobile`，含 AuthApi/DispatchApi/BusinessCaseApi 等，与 `mobile/flutter-app/` 对应 feature 模块重复。README 已标 "archived, read-only reference"，但仍占用仓库空间 |
| D-18 | 文档 199 个 md 仅 50 个被跟踪；同主题多版本并存 | 性能报告 **7 份**；迁移文档 **5 份**；审计报告 **2 份**（06-14 / 06-21）。`docs/*.md`、`docs/plans/*`、`docs/operations/*` 为白名单制 |
| D-19 | 3 个 PNG（1.07MB）已入库且存在于历史 | 提交 `d26e3a3` 引入；`.gitignore` 无 `*.png` 规则 |
| D-20 | `certs/` 防护脆弱 | 仅 `.gitignore:151` `certs/*.key` 一条 glob 拦截，`.crt` 未拦、目录本身未 ignore；仓库内留有 CA 私钥 `dev_root_ca.key`。改名（如 `.key.pem`）即失效。**🛠 已修复（2026-09-02 处置批次）**：`.gitignore` 改为整目录 `certs/`（当时 `git ls-files certs/` 为空，无需取消跟踪），经 `git check-ignore -v` 验证 `.crt`/`.pem`/`.p12`/嵌套路径均命中。**CA 私钥轮换仍待运维执行（见 D-32）** |
| D-21 | 4 套平行启动入口 + 死脚本未清 | `scripts/fms.ps1`（44.5KB / 43 function）、`deploy/docker/Start-*.ps1|bat`（8 个，.bat 全为一行包装）、`.runtime/host-services/*/start_*.bat`（8 个）、edge compose。其中 `.runtime/host-services/tomcat/start_tomcat.bat` 仍在，而 `docs/DEPLOYMENT.md:130` 已声明"不再有独立 Tomcat 服务" |
| D-22 | 重复实现 | 时间解析 **6 处**各异（`parse_timestamp` ×2、`parse_datetime` ×2、`parse_datetime_value`、`parse_action_timestamp`、`parse_online_history_date`）；`ACCESS_TOKEN_COOKIE` 在 `api/src/middleware/jwt.rs:37` 与 `api/src/routes/auth/shared.rs:31` 各定义一次；ID 生成 uuid(12 处)/ulid(193 处) 双轨；`jsonwebtoken` 在 3 个 crate 重复声明 → 🛠 部分修复（2026-09-02 处置二批）：`ACCESS_TOKEN_COOKIE` 归一为单一真源 `middleware/jwt.rs`（`pub(crate)`），`auth/shared.rs` 改为 re-export（glob 消费方 `login/session.rs` 无感）；`cargo check -p fms-api` 通过、rustfmt 干净、全仓 grep 仅剩一处定义。其余：**`jsonwebtoken` 实为已收敛**（复查 2026-09-02 二批：根 `Cargo.toml:47` workspace 单点定义，api/application/infrastructure 三处均 `{ workspace = true }`，原证据过时）；时间解析 6 处与 ID 生成双轨仍开放 |
| D-23 | 幽灵依赖 | `actix-rt`（`Cargo.toml:26`）代码 **0 引用**；`argon2`（`Cargo.toml:50`）**0 引用**，与 bcrypt（14 处）并存；前端 `eslint.config.mjs:5` import `globals`，但该包不在 devDependencies（仅靠 lock 传递依赖存活）。**🛠 已修复（2026-09-02 处置批次）**：`actix-rt`/`argon2` 从 workspace 与 server 依赖移除（rg 全仓含 benches 0 引用核验 + `cargo check -p fms-server --all-targets` 通过）；`globals@^14.0.0` 显式声明进 vue-app devDependencies（与既有 lock 解析版本一致，lock 零漂移），eslint 配置可加载 |
| D-24 | `tests/tools/` 20 个元测试只有 10 个进 CI | `ci.yml` 已改为整体 `python -m pytest tests/tools -q`。2026-09-03 将对象引用守门从缩进/链式调用文本匹配改为格式无关的结构匹配，并继续断言 owner 保存 → 引用替换 → 同事务 commit 的顺序；本地全套 **111/111 通过** |
| D-25 | 前端覆盖率无门禁 | `vitest.config.ts` 仅 14 行，无 `coverage` 段、无阈值。104 个单测中约 17 个是 `src/legacy/` 下的 parity 迁移守卫，不覆盖业务行为 |
| D-26 | CI 使用已废弃 action 版本 | `ci-performance.yml:30/75/97/119` 使用 `actions/upload-artifact@v3`、`actions/cache@v3`；`:359` `e2e-integration` job 设 `continue-on-error: true`，E2E 失败不阻塞合并 → 🛠 已修复（2026-09-02）：action 全部升至 v4（现文件已无 @v3）；`e2e-integration` 的 `continue-on-error` 已移除（仅存 `Baseline capture is optional` 一处为有意保留）。2026-09-03 续修：Vue 静态单测 job 不再重复执行依赖后端栈的 Playwright；浏览器 E2E 统一由带完整 Compose 栈的 `e2e-integration` job 执行。 |

---

## 4. 明确排除项（扫描过但**不构成**负债）

列出以免后续重复排查，也说明本清单的判据：

- **`unimplemented!` 246 处**——全部位于 `#[cfg(test)]` 测试替身内（application 220 / api 26），已逐文件核验，非生产负债。
- **`panic!` 135 处**——集中于测试替身与非关键路径；W0-1 已消除 sidecar 反序列化的进程级 abort 面。
- **前端类型安全**——`tsconfig.json` `strict: true` + `noUnusedLocals` + `noUnusedParameters`；`: any` / `as any` / `@ts-ignore` / `@ts-nocheck` 命中均为 **0**，`@ts-expect-error` 仅 1 处。这是强项，不是债。
- **依赖供应链基础面**——git 依赖 **0**、通配符版本 **0**、`Cargo.lock` 已提交（180KB）、无 deprecated npm 包、无 `file:`/`git:` 前端依赖。
- **技术栈单一性**——HTTP 框架仅 actix-web，异步运行时仅 tokio，无混用。
- **SSE 背压**——`sse/hub.rs`（946 行）实现完整：有界 capacity、`dropped_messages` 计数器、心跳 15s + 超时 45s、`Last-Event-ID` 语义。W3-13 缺的是"Lagged 后恢复"的行为测试，不是实现。
- **生产硬编码密钥**——未发现（仅测试文件内有样例密钥）。
- **跨栈代码复制**——`grep -rn 'legacy-backend' services --include='*.rs' --include='*.toml'` 命中 **0**，Rust 未从 Python 复制粘贴逻辑。

---

## 5. 与既有看板/路线图的关系

| 本清单项 | 既有文档状态 | 说明 |
|---|---|---|
| **D-27** | **推翻既有结论** | 看板与路线图 W1-1 均宣称「分层守门已接入 CI」。实测 ci.yml / nightly.yml YAML 非法，两者从未执行 |
| **D-28** | **状态失效** | 结构债计划将守门标为「P3 未完成」。实测清单 13 条中 8 条为注释误命中，扫描器本身有缺陷 |
| **D-31** | **与 ADR 冲突** | ADR-0004 主张 Rust 为执行控制面唯一真相。实测 `ai_entities` 双写者并存 |
| **D-29** | **回归** | 结构债计划 P1 记录已清零。实测 113 处/41 文件，且无守门 |
| D-05 | **未覆盖（新增）** | P3 计划清了 sqlx，漏了 redis 与 pgwire-replication；两个守门都无对应模式 |
| D-01 / D-02 | **未覆盖（新增）** | 路线图未评估 Flowable embedded 的故障域与降级行为 |
| D-03 | **未覆盖（新增）** | 路线图 W0-5 只处理了 fail-closed 行为，未覆盖配置文档化 |
| D-04 | 部分覆盖 | 迁移 120 的决策在 README 有记录，但"巡检是否调度"无跟踪项 |
| D-06 | 部分覆盖 | 路线图 W3-9/W3-10 已列入但未启动，本项提供量化基线 |
| D-07 | 部分覆盖 | 看板已跟踪路由层大文件，未覆盖 `api/src/services/` 伪分层（含 scheduler_runtime_service.rs 2134 行） |
| D-09 | **升为 P0** | W0-3 一次性验证通过，但**未固化进 CI**——叠加 D-27，这是高回归风险 |
| D-10 | **状态失效** | W1-5 标记 Done，但配置路径不一致使其可能未生效 |
| D-11 | 部分覆盖 | 路线图 W3-1 覆盖 Vue legacy，未覆盖 ai-react 第二前端 |
| D-16 / D-17 | 部分覆盖 | Anti-goals 声明"无限期保留 legacy 双轨"是反模式，但无具体退役排期 |

---

## 6. 建议处置顺序（按 ROI）

> 顺序已按二次核验结果重排。**第 1 步是前置条件**——不修 CI，第 4 步之后所有"接入 CI"的动作都落不了地。
>
> **🛠 处置进度（2026-09-02 处置批次）**：
> ✅ 已完成——#3 D-02、#4 D-28、#5 D-05、#7 D-01、#11 D-29/D-08，及卫生类 D-20 / D-23 / D-30（守门部分）；
> ✅ CI 批次——#1 D-27、#9 D-09/D-10、#10 D-03、D-24/D-26/D-36 已落地（D-35 守门已建但检查为红：真实发布漂移待决策，见该条）；
> ✅ 三批补充——#6 D-31（Python 侧只读收口，保留幂等种子）、#8 D-34（`frontend/legacy` 跟踪子集 + 服务挂载与 parity 真值源切换）；
> ⚸ 仅标记不执行（决策/运维/破坏性动作）——#2 D-32、#12 D-16/D-17。

| # | 项 | 动作 | 理由 |
|:--:|---|---|---|
| 1 | **D-27** | 修 3 处 heredoc 缩进 + 加 YAML lint 门禁 | 不修它，一切守门都是纸面 |
| 2 | **D-32** | 轮换 `data/ai_config.bak` 中的 key 与 dev CA；视为已泄露 | 凭据风险不随代码修复而消失 |
| 3 | **D-02** | production 缺 `FLOWABLE_DATABASE_URL` 拒绝启动 | 一行判断，消除静默丢数据 |
| 4 | **D-28** | 修扫描器（剥注释 + `cfg(test)`）→ 清单归零 → 解除 ignore | 有现成绿灯路径，成本低于继续等 P3 |
| 5 | **D-05** | 补 redis / pgwire-replication 守门模式 + 下沉 `runtime_redis_latency.rs` | 与 P3 同批，堵住同一类盲区 |
| 6 | **D-31** | 二择一：`ai_entities` 写者归属 | 双写者持续越久，分叉数据越难收敛 |
| 7 | **D-01** | `run_on_engine` 加超时 | 局部改动，解除线程池耗尽 |
| 8 | **D-34** | 生产静态资产移出未入库的 backup 目录 | 全新 clone 必 404，可部署性前提 |
| 9 | **D-09 + D-10** | CI 加空库 `sqlx migrate run`；显式 `cargo deny --config` | 固化 W0-3 成果，防回归 |
| 10 | **D-03** | 补 `.env.example` + 差集校验 CI（85 变量） | 可分批 |
| 11 | **D-29 / D-08** | 加 `Option<Arc<dyn>>` 守门；测试替身加 `cfg(test)` | 防止已清零项回潮 |
| 12 | **D-16 / D-17** | legacy 退役决策：删除或打 tag 移出工作区 | 认知负载 |

---

## 7. 复现命令

```powershell
# D-27 决定性验证（需 pyyaml）
python -c "import yaml;[print(f, bool(yaml.safe_load(open(f)))) for f in ['.github/workflows/ci.yml','.github/workflows/nightly.yml']]"
grep -rn "^TRUSTED_PROXY_CIDRS" .github/

# D-28 注释误命中核验
grep -n "source.contains(pattern)" services/api-server/crates/application/tests/application_boundary_inventory.rs
grep -n "Postgres" services/api-server/crates/application/src/services/dispatch_chat_service.rs

# D-05 盲区核验
grep -n "redis\|sqlx\|pgwire" services/api-server/crates/application/Cargo.toml

# D-31 双写者
grep -rn "INSERT INTO ai_entities" services/api-server/crates --include=*.rs services/ai-sidecar/src --include=*.py

# D-34 未入库目录依赖
grep -n "legacy_frontend_root_dir" -A 8 services/api-server/crates/api/src/routes/static_files.rs
git check-ignore -v frontend/backup/legacy-frontend-archive/

# D-33 入库体积（勿用 du，会把内嵌 .git 算进来）
git ls-files -z <dir> | python -c "import sys,os;print(sum(os.path.getsize(p) for p in sys.stdin.read().split(chr(0)) if p and os.path.exists(p))/1048576)"

# D-01 / D-02
grep -c "timeout\|circuit\|retry" services/api-server/crates/infrastructure/src/integrations/embedded_flowable.rs

# D-10
ls services/api-server/deny.toml ; sed -n '15,20p' .github/workflows/ci.yml

# D-03 差集（需自行比对源码 env::var 与 .env.example）
grep -rhoP 'env::var\("\K[A-Z0-9_]+' services/api-server --include=*.rs | sort -u
```

---

## 8. 架构负债分类视图（长期管理计划）

> 本节将 D-01 至 D-36 以及其他识别的负债按架构视角重新分类，形成系统性的技术债管理框架。

### 8.1 架构边界负债（Architecture Boundary Debt）

| ID | 负债项 | 当前状态 | 目标状态 | 优先级 |
|---|---|---|---|:---:|
| **D-05** | Application 层持有 redis 生产依赖 | Cargo.toml:44 直接依赖，守门盲区 | 下沉至 infrastructure，补充守门模式 | P0 |
| **D-28** | 边界守门被 #[ignore]，扫描器误报 | 清单 13 条中 8 条为注释误命中 | 修正扫描器，解除 ignore | P0 |
| D-06 | 分层重心失衡（胖应用层） | application 128K 行，是 infra 的 3.3 倍 | 按域重构，沉淀领域模型 | P1 |
| D-08 | 测试替身编入生产二进制 | 1785 行内存桩无 cfg(test) | 添加编译门控 | P1 |
| D-30 | api 层的 services 目录不受守门约束 | python_sidecar_proxy.rs (857 行) | 扩展守门范围或重构 | P2 |
| 扩展-1 | Application 层依赖泄露 | 直接依赖 sqlx/redis/pgwire-replication | 仅保留 domain ports 依赖 | P1 |
| 扩展-2 | AI 架构双向依赖 | Rust ↔ Python 循环调用 | 单向依赖，明确控制面 | P1 |

**关键指标**：
- Application 层生产依赖守门：sqlx / redis / pgwire / infrastructure **新增量为 0**
- 守门盲区：2 处（api/src/services、注释误命中）
- 分层失衡度：3.3x（application/infrastructure 行数比）

### 8.2 错误处理与稳定性负债（Error Handling & Stability Debt）

| ID | 负债项 | 当前状态 | 风险等级 | 优先级 |
|---|---|---|:---:|:---:|
| **D-01** | Flowable 无超时/熔断/重试 | 无任何超时保护，默认池 512 线程 | 高 | P0 |
| **D-02** | Flowable 静默降级为内存 | 缺 DATABASE_URL 时只 warn | 高 | P0 |
| D-07 | 上帝文件（9 个 >2000 行） | 最大 2876 行，维护困难 | 中 | P1 |
| 扩展-3 | **unwrap/expect 存量（口径已修正）** | 总量 2,505；**剔除 `tests/` 与 `tests.rs` 后为 1,240**，实际生产值更低（内联 `mod tests` 未剥离） | 中 | P1 |
| 扩展-4 | Python 宽泛异常捕获 | 热点已从 24→5，非热点仍多 | 中 | P1 |
| 扩展-5 | 乐观锁覆盖不完整 | 仅验证 Todo/Flight，其他聚合根未审计 | 中 | P1 |
| 扩展-6 | 跨进程事务无协调 | Rust 写 DB + Python LLM + 回写，无 Saga/2PC | 中 | P2 |

**关键指标**：
- unwrap/expect（非测试代码）：1,149 处
- 无超时保护的关键路径：1 个（Flowable）
- 宽泛异常捕获（热点文件）：5 处
- panic! 宏调用：135 处（已验证全在测试或非关键路径，不构成负债）

### 8.3 前端技术债（Frontend Debt）

| ID | 负债项 | 当前状态 | 影响 | 优先级 |
|---|---|---|---|:---:|
| D-11 | 三套前端并存 | Vue (441 文件) + React (50 文件) + Legacy HTML | 重复开发成本 | P1 |
| D-15 | 产物体积失控 | dist/ 32M，字体占 27M (84%)，无 sourcemap | 加载慢，无法调试 | **P2** |
| D-25 | 覆盖率无门禁 | 104 个测试，无阈值，17 个为 legacy 守卫 | 质量保证弱 | P2 |
| 扩展-7 | **巨型组件** | 7 个 >1000 行（最大 1557 行） | 难测试、难维护 | P1 |
| 扩展-8 | 类型安全不足（已排除） | ~~1,490 处 any/unknown~~ | **实测为 0，这是强项** | N/A |
| 扩展-9 | Legacy 双轨未退役 | Vue 正式 + /frontend/html/* 兼容 | 双倍维护成本 | P2 |

**注**：扩展-8 经验证，前端类型安全实际很好（strict:true，无 any），从负债清单移除。

**关键指标**：
- 前端技术栈：3 套（Vue/React/Legacy）
- 超大组件（>1000 行）：7 个
- 产物体积：32M（字体 84%）
- 测试覆盖率：无门禁

### 8.4 数据一致性负债（Data Consistency Debt）

| ID | 负债项 | 当前状态 | 风险 | 优先级 |
|---|---|---|---|:---:|
| **D-31** | ai_entities 双写者 | Rust + Python 同时写，ADR-0004 冲突 | 数据分叉 | P0 |
| D-04 | 外键全删，完整性靠人工 | 136 条外键已删，巡检未定时化 | 引用完整性破坏 | P0 |
| D-12 | Schema 两套管理路径 | Flowable 自建表 + migrations/ 编号迁移 | 版本管理混乱 | P1 |
| 扩展-10 | **并行读模型同步策略缺失** | 机位（flights.stand / stand_occupations）、审批（/ai/pending-actions / /ai/proposals）、工单时间线（协同事件 / 派工日志）| 用户看到不一致数据 | **P1**（待核实具体同步机制后可能升 P0）|
| 扩展-11 | Outbox 模式覆盖不完整 | 仅航班域，Dispatch/Business Case 未对齐 | 事件溯源不完整 | P1 |
| 扩展-12 | 监控宽表同步保证不明确 | flight_monitor_rows 延迟/重试未文档化 | 列表数据陈旧 | P2 |

**关键指标**：
- 双写者冲突：1 组（ai_entities）
- 并行读模型：3 组（机位/审批/工单）
- 已删外键：136 条
- Outbox 覆盖率：1/N 个写域

### 8.5 测试与可观测性负债（Testing & Observability Debt）

| ID | 负债项 | 当前状态 | 缺口 | 优先级 |
|---|---|---|---|:---:|
| **D-27** | CI workflow YAML 守门 | 4 个 workflow 解析通过，独立 lint 门禁已接入 | 已修复；继续防回归 | Done |
| **D-09** | 157 个迁移文件（编号至 158）空库验证 | clean-install PostgreSQL job 已接入；2026-09-03 修正无效 sqlx 子命令 | 待 CI 实际执行确认迁移链 | P0 |
| D-13 | 测试结构性偏斜 | infrastructure 393 行/测试，最稀疏 | SQL 层质量保证弱 | P1 |
| D-14 | Rust → Python 链路追踪断裂 | traceparent/X-Correlation 命中 0 | 无法归因故障 | P1 |
| D-24 | 元测试整目录进 CI | `tests/tools` 本地 111/111 通过 | 已修复 | Done |
| 扩展-13 | 契约测试覆盖不全 | 缺移动端 DTO、前端 HTTP、MQ 格式 | 接口演进风险 | P1 |
| 扩展-14 | 关键路径缺 E2E | 登录→列表→派工 主路径未进 CI | 回归依赖手工 | P1 |
| 扩展-15 | SLO 告警不完整 | 缺 AI 超时、连接池耗尽、Redis 降级 | 故障发现慢 | P2 |
| 扩展-16 | 干净环境自举缺口 | 0→112 已验证但未固化，112→158 未验证 | 新环境搭建风险 | P1 |

**关键指标**：
- Workflow YAML 合法率：100%（4/4）
- 迁移验证：clean-install job 已接入，待托管 CI 首次成功运行确认
- 元测试 CI 覆盖率：100%（整目录执行；本地 111/111）
- 链路追踪传播：0%

### 8.6 配置与密钥管理负债（Configuration & Secret Management Debt）

| ID | 负债项 | 当前状态 | 风险 | 优先级 |
|---|---|---|---|:---:|
| **D-32** | 本地残留真实凭据 | data/ai_config.bak 含真实 API key | 泄露风险 | P0 |
| **D-03** | 85 个环境变量无文档 | 116 个 env key，仅 31 个在 .env.example | 配置错误 | P0 |
| D-10 | deny.toml 路径不一致 | 可能未生效，供应链策略空转 | 依赖风险 | P1 |
| D-20 | certs/ 防护脆弱 | 仅拦截 *.key，改名即失效 | CA 私钥泄露 | P1 |
| 扩展-17 | AI 配置加密不一致 | Rust 写不加密（pg_ai_entity_config_repository.rs grep encrypt 零命中），Python 写才加密（ConfigEncryptor）。**更严重**：双写者加密策略不一致意味着跨写者读写会互相解不开 | 数据损坏 + 明文泄露 | P0 |
| 扩展-18 | 配置形状收敛未完成 | tools vs tooling 模糊，未经浏览器验证 | 配置错误 | P2 |
| 扩展-19 | 密钥轮转机制缺失 | 无过期策略、撤销流程、版本化 | 泄露后无法快速响应 | P2 |
| 扩展-20 | 配置热更新边界不清晰 | 哪些需重启、哪些热生效未明确 | 运维误操作 | P2 |

**关键指标**：
- 未文档化环境变量：85 个（73%）
- 本地泄露凭据：2 个文件
- 加密策略不一致：1 个表（ai_entities）
- 密钥轮转机制：无

### 8.7 性能与扩展性负债（Performance & Scalability Debt）

| ID | 负债项 | 当前状态 | 影响 | 优先级 |
|---|---|---|---|:---:|
| D-01 | Flowable 无超时 | 可能耗尽 512 线程池 | 服务拖垮 | P0 |
| D-15 | 前端产物 32M | 字体占 27M | 加载慢 | P2 |
| 扩展-21 | N+1 查询风险 | 航班列表/派工订单加载关联数据 | 性能瓶颈 | P2 |
| 扩展-22 | Redis 缓存策略未统一 | TTL/失效策略各异，雪崩风险 | 缓存穿透/雪崩 | P2 |
| 扩展-23 | SSE Lagged 恢复未完成 | 有逻辑但缺行为测试 | 慢客户端断连 | P2 |

**关键指标**：
- 无超时保护：1 个关键路径
- 前端产物体积：32M
- 缓存策略：未统一
- N+1 查询：未审计

### 8.8 依赖与供应链负债（Dependency & Supply Chain Debt）

| ID | 负债项 | 当前状态 | 风险 | 优先级 |
|---|---|---|---|:---:|
| D-10 | cargo deny 可能空转 | 配置路径不一致 | 供应链风险 | P1 |
| D-23 | 幽灵依赖 | actix-rt/argon2 零引用但在 Cargo.toml | 无用依赖 | P2 |
| D-26 | CI 使用废弃 action | upload-artifact@v3, cache@v3 | 兼容性风险 | P2 |
| D-33 | 仓库体积负债 | parity 135MB + 字体重复 53MB | clone 慢 | P2 |
| 扩展-24 | Monorepo 依赖版本不统一 | cargo deny 已配但历史违规未清 | 版本冲突 | P2 |
| ~~扩展-25~~ | ~~Python 依赖无锁文件~~ | **已排除**：实测有 uv.lock (614KB) | N/A | N/A |
| 扩展-26 | 前端依赖体积未优化 | node_modules/ 无分析报告 | 构建慢 | P3 |

**关键指标**：
- 幽灵依赖：3 个
- 仓库体积（主因）：188MB（parity + 字体）
- 废弃 action：2 种
- Python 锁文件：`uv.lock` 已存在且与 pyproject 同步（扩展-25 已排除，非负债）

### 8.9 移动端特定负债（Mobile-Specific Debt）

| ID | 负债项 | 当前状态 | 影响 | 优先级 |
|---|---|---|---|:---:|
| D-17 | Kotlin App 已归档但无删除排期 | 65 个文件，README 标注 archived 但仍在仓库 | 认知负载 | P2 |
| D-36 | 验收测试脱离 CI | 9 个 integration_test 不在 CI | 回归风险 | P1 |
| 扩展-27 | DTO 别名字段遗留 | serde(alias) 升级后是否可删未明确 | 接口演进不确定 | P2 |

**关键指标**：
- 移动端技术栈：2 套（Flutter + Kotlin）
- CI 外验收测试：9 个
- DTO 别名：若干（未量化）

### 8.10 文档与知识管理负债（Documentation & Knowledge Management Debt）

| ID | 负债项 | 当前状态 | 影响 | 优先级 |
|---|---|---|---|:---:|
| D-16 | legacy-backend/ 22.9 万行未删 | gitignore 但占工作区 | grep 污染 | P2 |
| D-18 | 文档白名单制，多版本并存 | 199 个 md 仅 50 个被跟踪 | 找不到真相 | P2 |
| D-21 | 4 套启动入口 + 死脚本 | tomcat 脚本仍在但已废弃 | 维护混乱 | P2 |
| 扩展-29 | 历史计划稿混淆 | AIP_PLAN 等含 TODO，与现行设计混淆 | 新人困惑 | P2 |
| 扩展-30 | ADR 状态不一致 | ADR-0004 记录与实际实现不符 | 架构理解偏差 | P2 |

**关键指标**：
- Legacy 代码行数：22.9 万行（Python）
- 文档跟踪率：25%（50/199）
- 启动入口：4 套
- ADR 漂移：至少 1 个

### 8.11 特殊的、系统性负债（Systemic & Cross-Cutting Debt）

| ID | 负债项 | 当前状态 | 根因 | 优先级 |
|---|---|---|---|:---:|
| D-29 | Option<Arc<dyn>> 反模式回潮 | 113 处，已清零又回来，无守门 | 缺回归保护 | P1 |
| D-22 | 重复实现 | 时间解析 6 处、常量重复定义 | 无共享工具库 | P2 |
| D-34 | 生产依赖未入库目录 | 全新 clone 必 404 | 构建流程缺陷 | P0 |
| D-35 | OR-Tools wasm 发布链断裂 | manifest 不一致，纯手工 | 无 CI 保护 | P1 |
| 扩展-31 | "时间旅行"原则未执行 | 代码保留迁移痕迹，而非按终态设计 | 架构心法未落地 | P1 |
| 扩展-32 | 边界守门未强制失败 | #[ignore] 或 continue-on-error | 治理工具失效 | P0 |
| ~~扩展-33~~ | ~~跨进程事务无协调~~ | **与扩展-6 重复**，已合并 | N/A | N/A |
| ~~扩展-34~~ | ~~配置热更新边界模糊~~ | **与扩展-20 重复**，已合并 | N/A | N/A |
| 扩展-35 | 多租户准备不足 | 单租户设计 | 架构预留不足 | P3 |

**关键指标**：
- 反模式回潮：1 种（Option<Arc<dyn>>，113 处）
- 重复实现：多种（时间解析、常量、ID 生成）
- 全新 clone 失败点：1 个（D-34）
- 跨进程一致性机制：无

---

## 9. 长期管理框架

### 9.1 债务生命周期管理

```
识别 → 评估 → 排期 → 执行 → 验证 → 防回归
  ↑                                      ↓
  └──────────── 持续监控 ←────────────────┘
```

#### 识别（Identification）
- 定期扫描（季度）：自动化工具 + 人工代码审查
- 触发式识别：新功能开发、重构、生产故障后
- 社区输入：团队成员反馈、新人入职困惑点

#### 评估（Assessment）
- 影响范围：局部/模块/系统
- 修复成本：人日估算
- 风险等级：P0（阻塞）/P1（侵蚀）/P2（负载）/P3（优化）
- ROI 计算：(影响范围 × 风险等级) / 修复成本

#### 排期（Prioritization）
1. **紧急泳道**：P0 债务，立即修复
2. **计划泳道**：P1/P2 债务，纳入迭代
3. **机会泳道**：P3 债务，重构时顺手清理
4. **技术债周**：每季度预留 1 周集中清理

#### 执行（Execution）
- 小步快跑：每次清理 1-3 项相关债务
- 配对编程：高风险修复需要 review
- 测试先行：修复前补测试，验证行为不变
- 文档同步：更新 ADR、README、本清单

#### 验证（Verification）
- 功能回归：相关测试必须通过
- 性能基准：关键路径不得劣化
- 债务指标：确认指标下降（如 unwrap 数量）
- 同行评审：至少 1 人 approve

#### 防回归（Regression Prevention）
- 守门测试：将修复转化为持续检查
- CI 门禁：新债务无法合并
- 代码审查：checklist 包含常见债务模式
- 定期审计：月度查看债务趋势

### 9.2 债务度量指标

#### 一级指标（P0 级，每周跟踪）
```
✓ CI 健康度：YAML 合法性、执行成功率
✓ 数据一致性：双写者数量、并行读模型数量
✓ 密钥安全：未加密 key 数量、本地泄露文件数
✓ 生产风险：无超时路径数、unwrap 数量
```

#### 二级指标（P1 级，每月跟踪）
```
✓ 架构边界：非法依赖数、守门盲区数
✓ 测试覆盖：infrastructure 测试密度、CI 外测试数
✓ 代码规模：超大文件数、分层失衡度
✓ 反模式：Option<Arc<dyn>> 数量、重复实现数
```

#### 三级指标（P2/P3 级，每季度跟踪）
```
✓ 认知负载：legacy 代码行数、技术栈数量
✓ 仓库卫生：幽灵依赖数、未跟踪文档数
✓ 供应链：依赖版本冲突数、废弃包数
✓ 性能：产物体积、N+1 查询数
```

### 9.3 治理规则（强制执行）

#### PR 合并前置条件
1. ❌ 不得新增 `unwrap()`/`expect()` 在非测试代码（当前基线：1,149 处）
2. ❌ 不得在 application 层新增 sqlx/redis/infrastructure 依赖
3. ❌ 不得新增 `Option<Arc<dyn>>` 模式（当前基线：113 处）
4. ❌ 不得新增超过 500 行的文件（除非有豁免）
5. ✅ 新增环境变量必须同步 `.env.example`
6. ✅ 新增迁移必须通过空库验证
7. ✅ 修改 ADR 涉及的代码必须更新 ADR 文档

**统计口径说明**：unwrap/expect 统计剔除测试代码（#[cfg(test)]、tests/ 目录、test 函数内），实际生产风险点为 1,149 处（总量 2,495 处，测试占 54%）。

#### 技术债配额（每迭代）
- 20% 时间用于技术债清理
- 每个 P0 债务必须在发现后 1 周内修复
- 每个 P1 债务必须在发现后 1 月内排期
- 债务总量环比不得增长

#### 豁免机制
- P0/P1 债务：需 Tech Lead + 1 位架构师批准
- P2/P3 债务：需 Tech Lead 批准
- 豁免必须记录原因、到期日、责任人
- 到期自动升级为 P0

### 9.4 成功标准（3/6/12 月里程碑）

#### 3 个月目标（2026-12-02）
- ✅ D-27 修复，CI 恢复执行
- ✅ D-32 凭据已轮换
- ✅ D-28 边界守门解除 ignore
- ✅ D-05 application 层依赖清零
- ✅ unwrap/expect（非测试）下降 20%（→ 920 处以下）
- ✅ P0 债务清零

#### 6 个月目标（2027-03-02）
- ✅ D-31 双写者归一
- ✅ 超大文件（>2000 行）减半（→ 4 个）
- ✅ 前端技术栈统一（Vue 或 React 二选一）
- ✅ 迁移验证进入 CI
- ✅ P1 债务下降 50%

#### 12 个月目标（2027-09-02）
- ✅ 分层失衡度 < 2.0（application/infrastructure）
- ✅ 测试密度达标（infrastructure > 200 行/测试）
- ✅ Legacy 代码清零（legacy-backend 删除）
- ✅ unwrap/expect（非测试）下降 60%（→ 460 处以下）
- ✅ 债务总量下降 70%
- ✅ 债务防回归机制完善（所有守门生效）

### 9.5 风险与应对

| 风险 | 概率 | 影响 | 应对策略 |
|---|:---:|:---:|---|
| 业务压力导致技术债优先级下降 | 高 | 高 | 固定 20% 时间配额，Tech Lead 强制执行 |
| 修复引入新 bug | 中 | 高 | 测试先行、小步迭代、feature flag |
| 团队成员流动导致债务意识缺失 | 中 | 中 | 入职培训包含本文档，季度回顾 |
| 工具链升级导致守门失效 | 低 | 高 | 守门测试覆盖工具本身，CI 监控 |
| 技术债清理与新功能冲突 | 高 | 中 | 技术债周与功能迭代交替进行 |

---

## 修订记录

| 日期 | 变更 |
|------|------|
| 2026-09-02 上午 | 初版：26 项负债（5×P0 / 10×P1 / 11×P2）+ 8 项排除说明 + 与既有文档的差异映射 |
| 2026-09-02 中午 | 二次核验：并入另一路排查的 10 条高严重度项（D-27～D-36）+ 中低严重度摘要（§0b）。全部实测复核；**修正 D-33 的 vendored 体积数据**（flowable-rust-oss 入库 54.5MB，非 385MB）；重排处置顺序，D-27 升为前置条件 |
| 2026-09-02 下午 | **三次验证**：所有关键发现已通过独立命令验证，数据更新至最新基线。确认项：D-27（CI YAML 非法）、D-28（边界守门 ignore）、D-29（113 处反模式）、D-30（scheduler_runtime_service.rs 2134 行，硬编码存在）、D-31（双写者）、D-32（本地凭据存在）、D-09（迁移 157 个文件、编号至 158，升为 P0）、D-05（redis 依赖）。更新项：D-07（超 2000 行文件 9 个）、unwrap/expect 口径统一（非测试 1149 处）、Python 锁文件验证（有 uv.lock，扩展-25 排除）。前端巨型 Composable 验证：3 个文件总计 4490 行 |
| 2026-09-02 晚 | 终审修正：① §8.8 关键指标"Python 锁文件：无"与扩展-25 排除结论矛盾，改为已存在；② D-09 迁移计数口径修正为 157 个文件（编号 100–158，001–099 已压缩、122/123 缺号），正文与 §8.5 同步 |
| 2026-09-02 晚（处置批次） | 按 §6 ROI 顺序执行修复批次，逐条回填「🛠」状态：**已修复** D-28（扫描器剥注释/`cfg(test)`、守门解禁、补 Cargo.toml 断言）、D-02（production 缺 `FLOWABLE_DATABASE_URL` 拒绝启动，`resolve_backend` 可测决策函数）、D-05（application redis 依赖移除、探针端口化至 server composition root、双守门补齐）、D-01（`run_on_engine` 超时，`FLOWABLE_ENGINE_TIMEOUT_SECS` 默认 30s）、D-08（测试替身 `cfg(any(test, feature = "test-support"))` 门控）、D-29（113 处/41 文件单向棘轮守门）、D-20（`certs/` 整目录 ignore）、D-23（actix-rt/argon2 幽灵依赖移除 + vue-app globals 显式声明）、D-30 守门部分（扫描范围扩至 `src/routes + src/services`）。**仅标记未执行（决策/运维/破坏性）**：D-32（凭据轮换）、D-31（`ai_entities` 写者归属）、D-34（生产静态资产位置）、D-16/D-17（legacy 退役）及其余 P1 结构项。CI 批次（D-27/09/10/03/24/26/35/36）另行处理。**既有问题记录**：`benches/` 下 fms-benches 存在先于本批次的编译失败（与本批改动无关）；`FLOWABLE_ENGINE_TIMEOUT_SECS` 需随 D-03 补入 `.env.example` |
| 2026-09-02 晚（处置二批） | §2/§3 结构项小步收口：**D-22 部分修复**（`ACCESS_TOKEN_COOKIE` 单一真源，jwt.rs 定义 + shared.rs re-export）；**D-04 残余弱点修复**（nightly health-wait 超时 `exit 1`，patrol 加 `if: always()` 保信号）；**D-14 证据修正**（sidecar 代理已传播 `x-request-id` 且有单测，剩余 traceparent/span 属 OpenTelemetry 路线图工程）。维持不变：D-35 检查为红（等发布链决策）、tests/tools 2 失败（等在途改动落地） |
| 2026-09-02 深夜（处置四批） | **D-35 中性重建完成 + 溯源断裂实锤**：WSL 全量重建通过（ninja 全并行两次 OOM 崩 VM，`--parallel 4` 修复）；与 8/8 旧构建对照——上游 sha256、em++、build.ninja FLAGS/DEFINES 全等，但当前 bridge 源码用 8/8 原始命令重编 `.o` 与存档不等 → **8/8 产物无法从当前仓库复现**（重初始化前历史不可回溯）。重建产物违反 fixture 契约（lateness 期望 20 实得 0）且 golden 11/12 漂移，**未安装、未发布**，仅存证。「先中性重建再发布」路线作废，发布决策升级为三选一（原样发布 / 先修契约再重建 / 维持 bridge.2 现状），见 D-35 节；`upstream-manifest.json` toolchain 字段修正为实测值（cmake 3.28.3 / ninja 1.11.1） |
| 2026-09-03 实施验收 | 修复其他在途改动造成的 Rust 模块导入/测试可见性断裂（215 个 application 编译错误及后续 test-target 错误），清除安全清单两段不可达旧实现与重复 helpers；全 workspace all-target 编译、全量 Rust 测试通过，严格 Clippy（workspace/all-targets/all-features）通过。一方 Rust 格式检查通过。修正 D-09 workflow 中不存在的 `sqlx migrate validate` 子命令；对象引用元守门改为格式无关结构检查，`tests/tools` 111/111；Vue typecheck + 684 tests、sidecar 1099 tests 通过；使用 `C:\flutter\bin\flutter.bat` 执行移动端 `flutter analyze integration_test/dispatch_acceptance_test.dart integration_test/support/acceptance_config.dart`，无问题。保留外部阻塞：D-32 运维轮换、D-35 发布决策。 |
| 2026-09-03 CI 修复批次 | 修正 Golden Tests 工作目录（从仓库父目录回到仓库根目录）；Vue 锁文件执行 `npm audit fix --package-lock-only`，`npm audit --audit-level=high` 由 6 个漏洞降为 0，typecheck 与 684 tests 仍通过；Python sidecar 测试实测 1099 通过，CI 的 mypy 暂以 5 个已类型化 AI 模块作为 smoke set，完整 `src/` 类型迁移保留为后续债务；Rust API/MQ Gateway 的 cargo-audit 对 RocketMQ/Actix 上游传递漏洞使用显式 RUSTSEC 忽略并保留升级说明。 |
| 2026-09-03 续修 | `fms-benches` 仅保留 Criterion benchmark targets，移除无用途的自动 `src/lib.rs` 库目标，修复 Rust 1.98 在普通 `cargo test` 中的编译器 ICE；`cargo test`（含 workspace 默认成员）重新通过。Vue CI 将 Playwright 从无后端的静态 job 移至完整 Compose 栈的 `e2e-integration`，避免把环境依赖误报为前端回归。 |
| 2026-09-03 CI 反馈续修 | 根据 GitHub Actions `33746483712` / `33759187935` 的失败日志继续修复：Rust/Python parity 测试不再硬编码 Windows `.venv/Scripts/python.exe`，统一解析 Windows/Unix workspace venv、`FMS_TEST_PYTHON` 显式覆盖及平台 PATH fallback；RocketMQ `BrokerConfig::default()` 移除启动期 `unwrap()`，容器无法直接枚举本机 IP 时改由 hostname 解析容器地址、最后才退回 loopback，避免向其他容器发布 `127.0.0.1`；Rust Docker builder 安装 Swagger UI 构建所需的 `curl`，并停止重新制造已删除的 benchmark lib target；CI/nightly 的复制密码改为满足数据库初始化策略的显式强测试值；Postgres `synchronous_standby_names` 中含连字符的 standby 名改为合法引用标识符，启动默认 `synchronous_commit=local` 避免主库在副本基线完成前阻塞，待运维窗口再显式切换 `remote_apply`；新增幂等 Flowable 数据库/角色 bootstrap 脚本，本地验证主库 healthy 且 Flowable role/database 均创建；Integration/E2E 失败诊断补齐 Postgres、Redis、双 broker、MQ Gateway 与 Rust API 日志。 |
| 2026-09-03 CI 反馈三修 | 按 Actions `33762007499` 失败日志修复：pprof 两个 `cfg(unix)` 测试原先以零时长采样必得空火焰图（本机 Windows 从未执行这两个测试），改为采样期间烧 CPU 产生真实样本并以共享锁串行化；mq-gateway 的 admin/pull 直连地址 `MQ_GATEWAY_BROKER_ADDR` 缺省 `127.0.0.1:10911` 在容器内不可达，compose 显式指向 `rocketmq-broker:10911` 并补守门断言；**D-35 按路径 A 发布 bridge.4**（公开 Release + active-manifest 真实摘要），manifest 守门转绿 |
