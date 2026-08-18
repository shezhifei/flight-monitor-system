# AI Agent 对象入环与评测闭环 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在已冻结的单环 harness 上，把本体动作真正接入工具面、把评测接到真实轨迹、把证据/新鲜度做成运行时不变量，并让派工建议走「确定性候选 → LLM 解释 → proposal」，不再堆第四套运行时。

**Architecture:** 加深 harness，不换框架。生产环仍是 `RuntimeService.stream_run_with_tools` → `LLMStreamRunner.stream_chat_with_tools`。Rust 继续拥有 job/run、lease、本体动作执行、proposal、领域写入。Python 只做适配：三个本体工具调用 Rust 内部动作面；hooks 校验 evidence 覆盖与新鲜度；eval 回放真实/录制轨迹。前端不新建 `/api/v1/agents`，继续用 `/api/v2/ai/*` + `frontend/ai-react` + 现有 Vue AI 页。

**Tech Stack:** Rust (Actix-web, SQLx, `OntologyActionServices`, Service Identity), Python sidecar (LLMStreamRunner, ToolExecutor, hooks, eval), PostgreSQL (`ai_eval_jobs` / `ai_eval_spans` / `ai_tool_calls` / `ai_run_checkpoints`，已有，不新建评测产品表), Prometheus/Grafana 现有栈, Vue 3 + `frontend/ai-react`。

**Sources synthesized:**

- 对照评审：2026-08-18（本系统实际 vs Palantir AIP / Claude Agent SDK / Deep Agents / Braintrust 评测闭环）
- 前序已批准计划：`docs/plans/2026-08-14-hybrid-agent-architecture.md`（Phase A–E）
- 控制面：`docs/plans/2026-06-29-ai-agent-resilient-tool-architecture.md`
- 执行环契约：`docs/architecture/AGENT_RUNTIME_LOOP.md`
- 本体事实：`docs/architecture/ONTOLOGY_V1.md`、`services/api-server/crates/application/src/services/ontology_actions/`
- 事实源：`docs/SOURCE_OF_TRUTH.md`

**给执行者：** 按 Task 编号线性做：`F0 → F1 → … → F7 → G* → H*（可与 G 后半并行）→ I* → J*（可与 I 并行）→ K*`。每个 Task 结束必须：相关测试绿、一次独立 commit、不把下一阶段的抽象提前做完。

---

## 0. 相对现状的最终决策

混合计划 Phase A–D 的架子已经在代码里。本计划**不重开架构讨论**，只补诚实缺口并产品化。

| 提议 | 决定 | 理由 |
|---|---|---|
| 新建 `BaseAgent` / LangGraph 业务节点 / Temporal / CrewAI | **否决** | 执行环已冻结；再加循环会变成第四套运行时 |
| 新 `/api/v1/agents/*` 或 WebSocket 平行面 | **否决** | 正式面仍是 `/api/v2/ai/*` |
| 侧车直连业务表做本体 lookup | **否决** | 对象读取走 Rust `OntologyActionServices`；侧车禁止写 `flights` / `dispatch_orders` / `todos` / `business_cases` / `domain_event_outbox` |
| 侧车用用户 JWT 打公开 `/api/v2/ai/ontology/actions/*` | **否决** | `ContextEnvelope.requester` 没有 access token；权限字段不可信 |
| 侧车用 Service Identity 打公开用户面 | **否决** | Service Identity 只服务 `/internal/ai/v1/*` |
| 在 Python 重写 `ontology_v1_rules` | **否决** | 约束真相在 Rust；Python 只展示 Rust 返回的 constraint / evidence |
| 必选 Chroma / 向量库才能做评测 | **否决** | 幻觉判定看 `evidence.json` 覆盖，不看向量相似度 |
| Eval 继续 stub `_run_agent_on_query → {success: True}` | **否决** | 门禁必须可证伪 |
| 幻觉率用航班号正则 `^[A-Z]{2}\\d{3,4}$` | **否决** | 格式对 ≠ 有证据；改为 evidence 覆盖 |
| `dispatch_ops` 让模型直接 `replan-apply` | **否决** | solver 只出候选；apply 只走 proposal → 审批 → Rust |
| 为对齐 Dify 做拖拽编排 | **否决** | 确定性 SOP 继续 Flowable |
| 执行 skill `scripts/` | **否决** | 维持现约束 |
| 通用跨业务 undo | **否决** | 只允许本 run / 同一 proposal chain |
| 本阶段新建评测表 | **否决** | `migrations/124` 已有 `ai_eval_jobs` / `ai_eval_spans` / `ai_eval_metrics_summary`；下一迁移号从 `126_*` 起，仅在确缺字段时追加 |
| 把 `sql_query_readonly` 留在生产 `query_ops` 实体 | **否决** | 生产查询走对象动作；SQL 只留给显式 debug 实体 |

**本计划要升级的意图：**

1. 对象语言入环：三个本体工具接真 Rust 动作，模板不再承诺不存在的工具。
2. 评测闭环：作业跑真实/录制轨迹，门禁看工具准确率、证据覆盖、越权、计划板、费用。
3. 证据与新鲜度从提示词纪律升级为 hook 不变量。
4. 派工：solver / 规则出候选，LLM 只排序解释。
5. Agent SLO 接到现有 Prometheus/Grafana，不引入 Jaeger 作为上线门槛。
6. 收敛工具面与死路径，审批卡带约束 diff。

---

## 1. 当前基线（不要再铺一层）

已经能用、本计划必须复用：

| 能力 | 位置 | 本计划怎么用 |
|---|---|---|
| 唯一生产环 | `llm_stream_runner.py`、`runtime_service/_streaming_tools.py` | 禁止新增循环 |
| 模板策略 | `templates/query_ops.py` / `anomaly_ops.py` / `dispatch_ops.py` | 改策略与 allowed categories，不改循环 |
| Hook 管道 | `hooks/pipeline.py`（PreToolUse / PostToolUse / PreCompact / Stop） | 新钩子挂进去 |
| 工作记忆 | `working_memory.py`（`evidence.json`） | 覆盖判定读这里 |
| 证据元数据 | `evidence_metadata.py` | 新鲜度/出处复用，不另造 schema |
| Shadow 阈值 | `templates/shadow_mode_config.py` | 新鲜度上限与此对齐 |
| 工具目录 | `ai_runtime_bootstrap.py` `_builtin_tool_catalog()` | 注册本体工具 |
| 工具执行 | `tools/tool_executor.py` | 增加 ontology 路由；写动作仍 proposal |
| 本体动作（Rust） | `ontology_actions/*`、`routes/ai_ontology.rs` | 内部端点复用同一组 service |
| 公开本体 HTTP | `GET/POST /api/v2/ai/ontology/*` | 人机/配置中心继续用；Agent 不走这条 |
| 内部面 | `routes/ai_internal/mod.rs` | 新增 ontology 动作内部路由 |
| Service Identity | `service_identity_issuer.py`、Rust middleware | 侧车调内部面 |
| Schema 镜像 | `ontology/schema_mirror.py` | 只读 schema；不拿它当动作执行器 |
| 评测表 | `migrations/124_ai_sidecar_migrations_redirect.sql` | 复用，修 service 不要修表除非缺列 |
| 语音评测 JSONL | `docs/fixtures/copilot_transcript_eval_standard.jsonl` | NL/派工评测同形 |
| 控制面 ledger | `ai_tool_calls` / `ai_run_checkpoints` | eval 从这里采样，不造第二套 trace |
| 派工 solver | `POST /api/v2/dispatch-orders/replan-snapshot`（Vue `useDispatchReplan.ts`） | Agent 只读 snapshot，禁止 `replan-apply` |
| 指标 | `monitoring/prometheus_exporter.py` | 加 label，不加新后端 |
| Playground | `frontend/ai-react/src/components/chat/*` | 审批卡补 diff；不新聊天壳 |
| 工具可解释 | `tools/tool_explain.py` | 本体工具纳入判定链 |

已核实的诚实缺口（实施时不要当「已完成」）：

1. `ontology_tools.py` 的 `_fetch_entity_from_api` 等是 stub；`capability_resolver` / `tool_executor` **未注册** `ontology.*`。模板却要求模型调用它们。
2. `EvaluationService._run_agent_on_query` 返回 `{"success": True}`；文件末尾 `def time(): pass` 会遮蔽 `import time`。`get_instance()` 单例违反组合期装配。
3. `test_llm_eval_service_e3.py` 多数是字典断言，不是服务行为测试。
4. `hybrid_retriever.index_chunk` 仍是 TODO（本计划 Phase K 才碰，F–J 不依赖向量）。
5. `aip/` 源码已删，仅剩 `__pycache__`。
6. 路线图进度表 W2-4 写「未开始」，正文写「进行中」。

---

## 2. 非协商约束

1. 不新增生产循环。模板 / hook / 工具 / eval 都挂在现有环上。
2. Python 不写 `flights` / `dispatch_orders` / `todos` / `business_cases` / `domain_event_outbox`。
3. 有效权限 = 实体能力 ∩ 工具治理 ∩ 调用模式 ∩ **Rust 已存储的 requester 权限** ∩ 对象策略 ∩ feature flag。工具配置不授予权限。Python 传入的 `requester.permissions` 不可信。
4. 控制事件只走 RocketMQ；SSE 只推 token / UI；命令只走 `ai_runtime_commands`。
5. Skill 不执行 `scripts/`。Hook 不执行 shell。
6. 生产路径禁止 mock 工具回落。本体工具缺 Rust 响应时 fail-closed，不得返回 stub 航班。
7. 新迁移号从 `126_*` 起，不改已有编号。本阶段默认不建表。
8. 命令用 PowerShell；Python 测试用 `.\.venv\Scripts\python.exe`。
9. 契约版本轴见 `docs/architecture/AI_CONTRACT_VERSIONING.md`：本计划不碰 `ai-runtime.v1` / `ai-structured-output.v1` tag，除非补可选字段且双端 `#[serde(default)]`。

---

## 3. 目标形状（相对混合计划只加深，不改拓扑）

```text
Vue / ai-react
        │  SSE + /api/v2/ai/*
        ▼
Rust 控制面
  job/run · lease · checkpoint · proposal · 审批 · 本体动作 · 领域写入
        │  ContextEnvelope / ai_runtime_commands / ai.runtime.events
        ▼
Python sidecar（仍是唯一生产环）
  模板策略 → 计划板 → LLMStreamRunner
       → PreToolUse（lease / plan-first / solver-first）
       → 真工具：ontology.* 经 /internal/ai/v1/ontology/actions/*
                 域 query 工具（内部实现，逐步对 LLM 降权）
                 dispatch.list_solver_candidates（只读 snapshot）
       → PostToolUse（裁剪、evidence.json、新鲜度）
       → Stop（无承诺 + 证据覆盖）
       → 结构化输出 / proposal
        │
        ▼
Rust DomainActionExecutor → 业务表 + outbox
        │
        ▼
ai_tool_calls / checkpoints / eval spans → 门禁 → shadow / canary
```

本体工具与 Rust 动作对照（不得发明第三套名字）：

| Agent 工具 | 内部调用 | 允许的 `action_name` |
|---|---|---|
| `ontology.lookup` | `POST /internal/ai/v1/ontology/actions/read` | `flight.get_context`（默认）、`flight.search`、`dispatch.get_status`、`anomaly.list_open`、`stand.check_availability`、`report.generate_briefing` |
| `ontology.explain_constraints` | 同上，优先 `stand.check_availability`；把 Rust 返回的 `constraints` / `evidence` 原样上送 | 不在 Python 算规则 |
| `ontology.propose_action` | 建议类：`POST .../actions/advisory`；受控写：只生成 proposal，不执行 | 建议：`flight.suggest_stand_adjustment`、`dispatch.suggest_replan`、`anomaly.suggest_escalation`、`flight.suggest_delay_action`、`notification.suggest_broadcast`。受控写：`Flight.change_stand`（及已注册的 DomainActionExecutor 动作）必须 `proposal_only` |

`ontology.lookup` 的 `entity_id` 解析规则（写进测试）：

| 输入 | 映射 |
|---|---|
| `flight:<uuid>` 或裸 `flight_id` | `flight.get_context` + `{flight_id}` |
| `stand:<id>` | `stand.check_availability` + stand 参数 |
| 无法解析 | 工具失败，`blocked_by=schema`，不返回 stub |

---

## Phase F — 对象语言入环（第 1–3 周）

目标：模板承诺的三个本体工具在生产环里能打到真实 Rust 动作；生产 `query_ops` 看不到 SQL。

### Task F0: 把本计划挂到冻结契约

**Files:**

- Modify: `docs/architecture/AGENT_RUNTIME_LOOP.md`
- Modify: `docs/architecture/ARCHITECTURE_IMPROVEMENT_ROADMAP.md`（W2-4 进度表与正文对齐；相关文档索引加入本计划）
- Modify: `.gitignore`（白名单本文件，便于入库）
- Test: `tests/tools/test_architecture_docs_consistency.py`（若改了它引用的文档，跑一遍）

**Step 1: 改契约文档**

在 `AGENT_RUNTIME_LOOP.md` 文首「相关计划」补一行指向本文件。在 §1.1 表格「工具轮次环」后加一行：

```text
| 本体工具 | `ontology.lookup` / `explain_constraints` / `propose_action` | 必须经 Rust `/internal/ai/v1/ontology/actions/*` 打到 `OntologyActionServices`；禁止 sidecar stub 对象图 |
```

在 §1.2 增加：

```text
7. **禁止模板点名未注册工具**——system prompt 提到的工具名必须出现在 `_builtin_tool_catalog()`（或 MCP/skill 快照）里。
8. **禁止 Python 重写 ontology_v1_rules**。
```

路线图：进度表 W2-4 改为「进行中（混合计划 A–D；对象入环/评测见 2026-08-18 计划）」，与正文一致。索引表加入本计划。

**Step 2: 跑文档测试**

```powershell
.\.venv\Scripts\python.exe -m pytest tests/tools/test_architecture_docs_consistency.py tests/tools/test_docs_no_stale_references.py -q
```

Expected: PASS

**Step 3: Commit**

```powershell
git add docs/architecture/AGENT_RUNTIME_LOOP.md docs/architecture/ARCHITECTURE_IMPROVEMENT_ROADMAP.md docs/plans/2026-08-18-ai-agent-optimization.md .gitignore
git commit -m "docs: schedule ontology-in-the-loop and eval-gate follow-on plan"
```

---

### Task F1: Rust 内部本体动作端点

公开面 `POST /api/v2/ai/ontology/actions/read|advisory` 继续给人用（`JwtAuth` + 当前 claims）。Agent 走内部面：Service Identity 认证，**权限从该 run 已落库的 envelope / job requester 重算**，不信 Python body 里的 permissions。

**Files:**

- Create: `services/api-server/crates/api/src/routes/ai_internal/ontology_actions.rs`
- Modify: `services/api-server/crates/api/src/routes/ai_internal/mod.rs`（挂路由）
- Modify: `services/api-server/crates/api/src/routes/ai_internal/tests.rs` 或新建同目录测试
- Reuse: `application/src/services/ontology_actions/permissions.rs` 的 `read_action_permission` / `advisory_action_permission`
- Reuse: `OntologyActionServices`（已在 `server/di/flight.rs` 装配）
- Test: `cargo test -p fms-api -- routes::ai_internal`

**内部契约（冻结，写入测试）：**

```json
POST /internal/ai/v1/ontology/actions/read
POST /internal/ai/v1/ontology/actions/advisory
{
  "run_id": "run_...",
  "action_name": "flight.get_context",
  "arguments": { "flight_id": "..." }
}
```

成功：`200` + 现有动作 service 的 JSON（已含 `evidence`，见 `flight_context_service.rs`）。

失败码（HTTP + JSON `error_code`，与现有 ApiError 对齐）：

| 条件 | 行为 |
|---|---|
| 无 / 坏 Service Identity | 401 |
| `run_id` 不存在 | 404 `AI_RUN_NOT_FOUND` |
| 动作名不是 read/advisory 白名单 | 400 `unknown read/advisory action` |
| run 上存储的 requester 缺对应权限 | 403 `TOOL_ACTOR_PERMISSION_DENIED` |
| 对象不存在 | 404（复用 `OntologyActionError::NotFound`） |

**Step 1: 先写失败测试**

覆盖：无身份 401；未知动作 400；run 存在但 requester 无 `flight:read` 时 `flight.get_context` 403；权限足够时把请求转到 `FlightContextService`（可用 test double / 已有 integration 风格）。

**Step 2: 跑测试确认失败**

```powershell
cd services\api-server
cargo test -p fms-api -- routes::ai_internal::ontology -- --nocapture
```

Expected: FAIL（模块/路由还不存在）

**Step 3: 最小实现**

- Handler 只做：验 Service Identity → 读 run 的已存 envelope → `read_action_permission` / `advisory_action_permission` → `claims` 用 **run 上的权限**（job 创建时 Rust edge 写入的那份，不是 Python 重放）→ 调用已有 `OntologyActionServices`。
- 不要复制 `execute_read_action` 的 match 臂到第三处：抽一个 `dispatch_read_action(services, action_name, args)` 给公开面和内部面共用（公开面仍用 JwtAuth；内部面用 run 权限）。若抽取会撑大 diff，允许内部 handler 先复制 match，但必须调用同一组 service 方法。
- 不在内部面执行 `Flight.change_stand`。受控写继续走 proposal。

**Step 4: 再跑测试**

```powershell
cd services\api-server
cargo test -p fms-api -- routes::ai_internal -- --nocapture
cargo test -p fms-api -- routes::ai_ontology -- --nocapture
```

Expected: 新测试 PASS；公开本体路由回归 PASS

**Step 5: Commit**

```powershell
git add services/api-server/crates/api/src/routes/ai_internal
git commit -m "feat(api): add internal ontology action endpoints for the agent loop"
```

---

### Task F2: 侧车本体 HTTP 客户端

**Files:**

- Create: `services/ai-sidecar/src/infrastructure/ai/ontology/action_client.py`
- Modify: `services/ai-sidecar/src/infrastructure/ai/ontology/schema_mirror.py`（保持只拉 schema；注释写明「禁止当执行器」）
- Test: `services/ai-sidecar/tests/sidecar/test_ontology_action_client.py`

**行为：**

- Base URL 解析顺序与 job worker 相同：`AI_INTERNAL_API_URL` / `RUST_API_BASE_URL` / `AI_API_BASE_URL`（见 `messaging/ai_job_worker_bootstrap.py` `_resolve_rust_api_base_url`）。
- URL 必须过 `security/url_guard.py`（与 `schema_mirror` 相同）。
- 用 `ServiceIdentityIssuer` 签 **精确 path** 的 JWT（`/internal/ai/v1/ontology/actions/read` 或 `.../advisory`）。
- 缺 base URL、缺 issuer、HTTP 非 2xx：抛 typed error（`OntologyActionClientError`），**不得**吞掉后返回 stub 实体。
- 超时默认 2s（只读 SLO 500ms 是目标，客户端超时略宽）；不重试写语义请求（本客户端只打 read/advisory）。

**Step 1: 失败测试**

- 无 base URL → 构造即失败或首次调用失败（选一种，测试锁死）。
- 对 mock HTTP：200 返回 body；403/404 变成 typed error，`error_code` 透出。
- 断言请求 path、`Authorization` 存在、body 含 `run_id` + `action_name`。

**Step 2:**

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_ontology_action_client.py -q
```

Expected: FAIL

**Step 3–5:** 实现 → 再跑 PASS → commit

```powershell
git add services/ai-sidecar/src/infrastructure/ai/ontology/action_client.py services/ai-sidecar/tests/sidecar/test_ontology_action_client.py
git commit -m "feat(ai): add fail-closed ontology action client"
```

---

### Task F3: 拆掉 `ontology_tools.py` stub

**Files:**

- Modify: `services/ai-sidecar/src/infrastructure/ai/ontology_tools.py`（整文件重写为薄适配，保留公开类型 `ConstraintViolation` / `ProposalCandidate` / `EntityLookupResult` 以免测试 import 崩；或把类型挪到同目录 `ontology/types.py` 并改 import）
- Test: `services/ai-sidecar/tests/sidecar/test_ontology_tools.py`（新建；若已有只测 stub 的文件，改成反 stub）

**必须删除的行为：**

- `_fetch_entity_from_api` 按 `flight_` 前缀返回写死 delayed 航班
- `_load_aircraft_gate_matrix` 写死 B737/A10
- `_filter_registered_actions` 在空 registry 时「允许全部」
- 进程内 `_cached_lookups` 充当假 Redis

**新行为：**

```python
async def lookup(self, *, run_id: str, entity_id: str, include_relations: bool = True) -> dict:
    action_name, arguments = parse_entity_id(entity_id)
    raw = await self._client.read(run_id=run_id, action_name=action_name, arguments=arguments)
    return attach_evidence(raw, source="ontology.lookup", object_id=arguments_object_id(arguments))

async def explain_constraints(self, *, run_id: str, entity_type: str, proposed_change: dict) -> dict:
    # Map proposed_change → stand.check_availability / flight.get_context arguments.
    # Return Rust constraints unchanged. Unknown mapping → hard violation, do not invent rules.

async def propose_action(self, *, run_id: str, action_name: str, parameters: dict, allowed_actions: list[str]) -> dict:
    if action_name not in allowed_actions:
        raise UnregisteredActionError(action_name)
    if action_name in ADVISORY_ACTIONS:
        return await self._client.advisory(...)
    if action_name in CONTROLLED_WRITE_ACTIONS:  # e.g. Flight.change_stand
        return {"execution_mode": "proposal_only", "action_name": action_name, "parameters": parameters}
    raise UnregisteredActionError(action_name)
```

`allowed_actions` 来自 **envelope.ontology.allowed_actions**（run 开始时的快照），不是客户端自报。

**Step 1: 反 stub 测试先写**

```python
@pytest.mark.asyncio
async def test_lookup_does_not_return_stub_flight_when_client_fails(monkeypatch):
    # client raises → lookup raises; result is not {"status": "delayed", "current_gate": "A10"}

@pytest.mark.asyncio
async def test_lookup_flight_id_calls_flight_get_context():
    # capture client.read kwargs

@pytest.mark.asyncio
async def test_propose_unregistered_action_fails_closed():
    # action_name="Flight.delete" not in allowed_actions → error, no client call

@pytest.mark.asyncio
async def test_propose_change_stand_is_proposal_only():
    # no advisory/read HTTP; result.execution_mode == "proposal_only"
```

**Step 2–5:** FAIL → 实现 → PASS → commit

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_ontology_tools.py -q
git add services/ai-sidecar/src/infrastructure/ai/ontology_tools.py services/ai-sidecar/tests/sidecar/test_ontology_tools.py
git commit -m "fix(ai): wire ontology tools to Rust actions and remove stubs"
```

---

### Task F4: 注册进目录与执行器

**Files:**

- Modify: `services/ai-sidecar/src/infrastructure/ai/ai_runtime_bootstrap.py` `_builtin_tool_catalog`
- Modify: `services/ai-sidecar/src/infrastructure/ai/tools/base.py` — `ToolCategory` 增加 `ONTOLOGY = "ontology"`
- Create: `services/ai-sidecar/src/infrastructure/ai/tools/ontology_tool_definitions.py`（三个 `BaseToolDefinition`，`to_schema()` 纯 function schema，治理字段不进 schema）
- Modify: `services/ai-sidecar/src/infrastructure/ai/tools/tool_executor.py` — `get_tool_type` / 执行路由识别 `ontology.*`；只读/解释走 client；`propose_action` 的受控写走现有 proposal 路径
- Modify: `services/ai-sidecar/src/infrastructure/ai/tools/__init__.py`（导出定义）
- Modify: `services/ai-sidecar/src/infrastructure/ai/tools/tool_explain.py`（本体工具出现在 explain 链）
- Test: `services/ai-sidecar/tests/sidecar/test_capability_resolver_builtin_catalog.py`（扩）
- Test: `services/ai-sidecar/tests/sidecar/test_ontology_tool_executor.py`（新建）

**治理预设：**

| 工具 | preset | public | required_account_permissions |
|---|---|---|---|
| `ontology.lookup` | `read_only_query` | false | 由 Rust 按 action 再查（`flight:read` 等） |
| `ontology.explain_constraints` | `read_only_query` | false | 同上 |
| `ontology.propose_action` | 建议=read；受控写=`internal_reversible_action` / proposal_only | false | 受控写不得 `allow_direct` |

LLM schema 示例（lookup）：

```json
{
  "name": "ontology.lookup",
  "description": "Look up a flight-ops object and its relations via the registered ontology read actions. entity_id like flight:<id> or stand:<id>.",
  "parameters": {
    "type": "object",
    "properties": {
      "entity_id": {"type": "string"},
      "include_relations": {"type": "boolean"}
    },
    "required": ["entity_id"]
  }
}
```

**Step 1:** 目录测试断言三个名字在 `_builtin_tool_catalog()`；`to_schema()` 不含 `governance` / `required_account_permissions`。执行器测试：`ontology.lookup` 走到 client，不走到 stub。

**Step 2–5:**

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_capability_resolver_builtin_catalog.py services/ai-sidecar/tests/sidecar/test_ontology_tool_executor.py services/ai-sidecar/tests/sidecar/test_tool_explain.py -q
git add services/ai-sidecar/src/infrastructure/ai/ai_runtime_bootstrap.py services/ai-sidecar/src/infrastructure/ai/tools/base.py services/ai-sidecar/src/infrastructure/ai/tools/ontology_tool_definitions.py services/ai-sidecar/src/infrastructure/ai/tools/tool_executor.py services/ai-sidecar/src/infrastructure/ai/tools/__init__.py services/ai-sidecar/src/infrastructure/ai/tools/tool_explain.py services/ai-sidecar/tests/sidecar/test_capability_resolver_builtin_catalog.py services/ai-sidecar/tests/sidecar/test_ontology_tool_executor.py
git commit -m "feat(ai): register ontology tools in the builtin catalog and executor"
```

---

### Task F5: 模板策略与 SQL 退出生产查询面

**Files:**

- Modify: `templates/query_ops.py`、`anomaly_ops.py`、`dispatch_ops.py`
  - `allowed_tool_categories` 加入 `"ontology"`
  - 保留对 `ontology.*` 的 prompt，但改成与真实参数一致（`entity_id` / `proposed_change` / `action_name`），删掉「调用不存在签名」的句子
- Modify: `query_ops.py` `denied_tools`：`WRITE_ACTION_TOOLS` ∪ `{sql_query_readonly}`
- Modify: 默认实体文档 `config/config_normalizer.py` `default_entity_document`（或现有 seed）：生产 query 实体 `denied_tools` 含 `sql_query_readonly`；如需 SQL，单独 `entity_id` 如 `ai-query-debug` 且不得作为 NL Query 默认实体
- Test: `tests/sidecar/test_query_ops_template.py` — 断言 categories 含 ontology；`sql_query_readonly` 被拒
- Test: `tests/sidecar/test_anomaly_ops_template.py`、`test_dispatch_ops_template.py` — categories 含 ontology
- Test: `tests/sidecar/test_tool_explain.py` 或扩 explain：`query_ops` × `sql_query_readonly` → deny / `blocked_by=template`

**Step 1:** 先改测试（`test_query_ops_template.py` 里 `allowed_tool_categories` 断言今天是 `{"query","flight","anomaly"}`，改成必须包含 `ontology`，且 `sql_query_readonly` ∈ `denied_tools`）。

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_query_ops_template.py services/ai-sidecar/tests/sidecar/test_anomaly_ops_template.py services/ai-sidecar/tests/sidecar/test_dispatch_ops_template.py -q
```

Expected: FAIL → 改模板 → PASS

**Commit:**

```powershell
git add services/ai-sidecar/src/infrastructure/ai/templates services/ai-sidecar/src/infrastructure/ai/config/config_normalizer.py services/ai-sidecar/tests/sidecar/test_query_ops_template.py services/ai-sidecar/tests/sidecar/test_anomaly_ops_template.py services/ai-sidecar/tests/sidecar/test_dispatch_ops_template.py
git commit -m "feat(ai): expose ontology tools on templates and hide SQL from query_ops"
```

---

### Task F6: 模板不得点名未注册工具（守门）

防止 F5 之后再次「prompt 承诺、目录没有」。

**Files:**

- Create: `services/ai-sidecar/tests/sidecar/test_template_tools_are_registered.py`

**行为：** 扫三个模板 `system_prompt_addendum` 里的 ``ontology.*`` / 反引号工具名，每个都必须出现在 `_builtin_tool_catalog()` 或 plan/skill 工具名集合。

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_template_tools_are_registered.py -q
```

Expected: PASS（F4/F5 之后）

**Commit:**

```powershell
git add services/ai-sidecar/tests/sidecar/test_template_tools_are_registered.py
git commit -m "test(ai): reject templates that advertise unregistered tools"
```

---

### Task F7: Phase F 验收

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar -k "ontology or query_ops or dispatch_ops or anomaly_ops or builtin_catalog or tool_explain" -q
cd services\api-server
cargo test -p fms-api -- routes::ai_internal
cargo test -p fms-api -- routes::ai_ontology
cargo test -p fms-application -- ontology_actions
```

**Exit：**

- `ontology.lookup` 对真实/录制的 `flight.get_context` 返回含 `evidence` 的对象图
- 客户端/Rust 失败时无 stub 航班
- 未注册动作 fail-closed
- `Flight.change_stand` 只出 proposal
- `query_ops` 快照不含 `sql_query_readonly`
- 文档契约已更新

有环境时浏览器（非本 Task 阻断）：`/frontend/nl_query.html` 问一个具体航班，时间线出现 `ontology.lookup`，结果 `source` 不是 stub。

---

## Phase G — 评测接到真实轨迹（第 3–6 周）

目标：换 prompt / 模板 / 模型必须过可证伪门禁。不新增评测产品，修现有 `EvaluationService`。

### Task G1: 评测夹具与指标定义

**Files:**

- Create: `docs/fixtures/agent_query_ops_eval.jsonl`
- Create: `docs/fixtures/agent_dispatch_ops_eval.jsonl`
- Create: `docs/operations/ai-agent-eval-harness.md`（对照 `ai-copilot-transcript-eval-harness.md`，写字段与 exit code）
- Test: `services/ai-sidecar/tests/sidecar/test_eval_dataset_schema.py`

**JSONL 每行（稳定字段，不要自由发挥）：**

```json
{
  "id": "query-delay-001",
  "task_type": "query_ops",
  "entity_id": "default",
  "user_query": "今天延误超过30分钟的航班有哪些？",
  "expected": {
    "allowed_tools": ["ontology.lookup", "get_delayed_flights", "search_flights_advanced"],
    "forbidden_tools": ["sql_query_readonly", "assign_gate", "Flight.change_stand"],
    "required_object_ids": [],
    "plan_required": false
  }
}
```

派工样本：`plan_required: true`，`expected.allowed_tools` 含 `update_plan` 与 `ontology.propose_action` 或 `dispatch.list_solver_candidates`；`forbidden_tools` 含 `apply_schedule` / `replan-apply`。

至少各 5 条；航班号用虚构或已脱敏。

**Commit:** `docs: add agent eval fixtures and harness notes`

---

### Task G2: 拆掉 eval stub 与单例

**Files:**

- Modify: `services/ai-sidecar/src/application/services/ai/llm_eval_service/service.py`
  - 删除 `get_instance` 单例
  - 删除文件底部 `def time(): pass`（它遮蔽了 `import time`，`_execute_single_test` 里的 `time.time()` 是坏的）
  - `_run_agent_on_query` 改为注入的 runner 协议，默认走 `RuntimeService.stream_run_with_tools` 的可测封装
- Modify: 所有 `EvaluationService.get_instance` 调用点改为 DI（`ai_container.py` / 路由）
- Test: 重写 `tests/sidecar/test_llm_eval_service_e3.py`，删掉「拼个 dict 就 assert」的空测

**Runner 协议（最小）：**

```python
class EvalAgentRunner(Protocol):
    async def run(self, *, user_query: str, task_type: str, entity_id: str) -> EvalRunResult: ...

@dataclass
class EvalRunResult:
    success: bool
    agent_response: str
    called_tools: list[str]
    evidence_object_ids: list[str]
    extracted_ids: list[str]          # 答案里抽出的航班号 / flight_id / order id
    total_tool_rounds: int
    plan_present: bool
    unauthorized_attempts: int
    tokens: dict[str, int]
    duration_ms: int
```

单测用 FakeRunner，不打真 LLM。另写一条测试：默认生产组装路径在缺 runner 时 fail-closed，不得返回 `{success: True}`。

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_llm_eval_service_e3.py -q
```

**Commit:** `fix(ai): run eval jobs through an injectable agent runner`

---

### Task G3: 门禁改为证据覆盖，而不是航班号正则

**Files:**

- Modify: `llm_eval_service/service.py` `_calculate_hallucination_rate` / `_calculate_tool_correctness`
- Create: `services/ai-sidecar/src/application/services/ai/llm_eval_service/gates.py`（纯函数，便于单测）
- Test: `services/ai-sidecar/tests/sidecar/test_eval_gates.py`

**公式（锁进测试）：**

| 门禁 | 计算 | 默认阈值 |
|---|---|---|
| `tool_accuracy` | `called_tools` 都在 `expected.allowed_tools` 且未点 `forbidden_tools` 的样本占比 | ≥ 0.95 |
| `ungrounded_id_rate`（原 hallucination） | 答案中抽出的 ID 不在 `evidence_object_ids` 且不在工具结果 `object_id` 里的比例 | ≤ 0.05 |
| `zero_violations` | `unauthorized_attempts`（lease deny / ACL / 未注册动作当成功） | = 0 |
| `avg_rounds` | 样本 `total_tool_rounds` 均值 | ≤ 模板硬顶（query 8 / anomaly 16 / dispatch 20） |
| `plan_board_compliance` | `plan_required` 样本中 `plan_present` 占比 | ≥ 0.90 |

删除 `_validate_flight_number` 作为门禁主路径（可留作抽取辅助）。

**Commit:** `feat(ai): score eval gates on evidence coverage and tool policy`

---

### Task G4: 从 ledger 采样生产轨迹

离线 JSONL 是 CI 门禁。线上评测从已有表抽样，不另造 trace 管道。

**Files:**

- Modify: `llm_eval_service/service.py` 增加 `ingest_run_from_ledger(run_id)`
- Reuse: Rust 已有 `GET /api/v2/ai/jobs/{job_id}/runs/{run_id}/tool-calls` 与 checkpoints；侧车用 Service Identity 打内部只读，或评测 job 只在 Rust 侧聚合后把摘要给 sidecar
- **推荐最小路径（少打跨语言）：** 评测 worker 用 sidecar 已持有的 checkpoint / working_memory 快照；若生产 run 的 `evidence.json` 在 `ai_run_checkpoints.snapshot`，直接读 Postgres 控制面表（侧车已允许写/读 `ai_*`）
- Test: `test_eval_ingest_from_checkpoint.py` — 给定一条 `after_tool` checkpoint JSON，抽出 `called_tools` / `evidence_object_ids`

禁止：再开 OpenTelemetry 导出器作为本 Task 验收；OTel 保持 optional。

**Commit:** `feat(ai): build eval spans from run checkpoints`

---

### Task G5: Eval Lab 与 CI 门禁

**Files:**

- Modify: `frontend/ai-react/src/features/llm-eval/LlmEvalLabPage.tsx`、`frontend/vue-app/src/pages/llm_eval_lab/LlmEvalLab.vue`（只展示真实 job 状态 / 门禁表；若现在绑内存作业，改为读 `ai_eval_jobs` API）
- Modify: Rust `routes/ai_eval.rs`（若已有）让 create/run/list 走持久化表
- Create: `services/ai-sidecar/tests/sidecar/test_eval_job_survives_process.py`（写 job → 新 service 实例读到同一 row）
- Optional CI：在现有 sidecar pytest job 加  
  `.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_eval_gates.py services/ai-sidecar/tests/sidecar/test_eval_dataset_schema.py`

**浏览器验收（有环境）：**

1. 打开 `/frontend/llm_eval_lab.html`
2. 跑 `agent_query_ops_eval.jsonl`
3. 刷新页面 job 还在
4. 人为把某条 expected 改到必失败，门禁变红

**Commit:** `feat(ai): persist eval lab jobs and wire gate dashboard`

---

### Task G6: Phase G 验收

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar -k "eval" -q
```

**Exit：** `_run_agent_on_query` 不再恒成功；无 `get_instance`；无 `def time()` 遮蔽；门禁测的是 evidence 覆盖；进程重启不丢 job。

---

## Phase H — 证据与新鲜度成为运行时不变量（可与 G 后半并行）

目标：query_ops 里写在 prompt 上的 freshness / evidence 规则，由 hook 强制。

### Task H1: 新鲜度 PostToolUse hook

**Files:**

- Modify: `hooks/pipeline.py` 新增 `FreshnessCheckHook`（PostToolUse）
- Modify: `templates/shadow_mode_config.py` — 把 key 对齐真实工具名（今天是 `flights.lookup`，生产工具是 `ontology.lookup` / `get_delayed_flights` / `flight_status_lookup`）。**同一份 dict 供 hook 与 shadow 使用。**
- Reuse: `evidence_metadata.py`
- Test: `tests/sidecar/test_freshness_hook.py`

**行为：**

- 工具结果缺 `as_of`：只读查询工具视为过期，结果改写为 `{ok: false, error_code: EVIDENCE_STALE, detail: "missing as_of"}`，hook 仍返回 True（让模型看到错误并重试），并写入 evidence 一条失败记录。
- `as_of` 超过该工具阈值：同样 `EVIDENCE_STALE`，附 `freshness_seconds` 与 `max_age`。
- 非查询工具（plan / skill / propose）跳过。

阈值起步（可调，写进测试）：

| 工具 / source | max_age |
|---|---|
| `ontology.lookup`（flight） | 30s |
| `ontology.lookup`（stand） / stand 类 | 10s |
| dispatch 只读 / `dispatch.get_status` | 60s |

**Commit:** `feat(ai): reject stale tool evidence in PostToolUse`

---

### Task H2: 证据覆盖 Stop hook

**Files:**

- Modify: `hooks/pipeline.py` 新增 `EvidenceCoverageHook`（Stop）
- Reuse: `CRITICAL_ID_PATTERNS`（已有，但今天只匹配 `F[0-9]{4,}`。**必须扩展**为同时覆盖国内航班号 `CA1234` / 纯数字四位 / UUID flight_id。改 patterns 时同步 `test_context_budget_preserves_ids.py`）
- Test: `tests/sidecar/test_evidence_coverage_hook.py`

**行为：**

- 从最后一条 assistant 文本抽 ID。
- 每个 ID 必须出现在 `working_memory` 的 `evidence.json` `object_id` 或 `content` 中。
- 未覆盖：不让「假装知道」的文本出站。将最终回答改写为固定降级（中文：「以下编号缺少工具证据，不能当作事实：…」）或 hook 返回 False 且 runner 使用降级文本。选一种并在测试锁死。
- `query_ops` 默认开启；`anomaly_ops` / `dispatch_ops` 对假设段落可放行，但 **不得**把未覆盖 ID 说成已执行变更（与现有 `NoPromisesHook` 叠加）。

**Commit:** `feat(ai): block ungrounded identifiers on Stop`

---

### Task H3: 挂到默认管道

**Files:**

- Modify: `hooks/pipeline.py` `default_hooks()` 顺序：现有 Pre 钩子 → `ResultSanitizationHook` → `FreshnessCheckHook` → … → `NoPromisesHook` → `EvidenceCoverageHook` → `OutputGuardrailHook`
- Modify: `llm_stream_runner.py` / `_streaming_tools.py` 确认 Stop 在最终文本发出前运行（若现在只在无 tool_calls 时跑 Stop，保持该时机）
- Test: 扩 `test_hook_pipeline.py`

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_hook_pipeline.py services/ai-sidecar/tests/sidecar/test_freshness_hook.py services/ai-sidecar/tests/sidecar/test_evidence_coverage_hook.py services/ai-sidecar/tests/sidecar/test_context_budget_preserves_ids.py -q
```

**Commit:** `feat(ai): enable freshness and grounding hooks by default`

---

### Task H4: Phase H 验收

**Exit：** 过期 evidence 不能进入最终陈述；答案里的航班号无 evidence 时降级；`NoPromisesHook` 仍拦截「已经为您改机位」。

---

## Phase I — 确定性计算在前（第 6–9 周）

目标：`dispatch_ops` 的高风险 proposal 必须来自 solver/规则候选，模型不得凭空改机位。

### Task I1: 只读 solver 候选工具

**Files:**

- Create: `services/ai-sidecar/src/infrastructure/ai/tools/solver_tools.py`
- Modify: `_builtin_tool_catalog` 注册 `dispatch.list_solver_candidates`
- Modify: `tool_executor.py` 路由
- Reuse: 现有 replan snapshot 语义（对照 `frontend/vue-app/src/composables/useDispatchReplan.ts` 的 `GET /api/v2/dispatch-orders/replan-snapshot`）
- **传输：** 同样走内部面。若该 snapshot 只有用户 JWT 面，则按 F1 模式加 `POST /internal/ai/v1/dispatch/replan-snapshot`（Service Identity + run 权限 `dispatch:read`），内部调用同一 application service。禁止侧车打 `replan-apply`。
- Test: `tests/sidecar/test_solver_candidates_tool.py`

工具 schema：

```json
{
  "name": "dispatch.list_solver_candidates",
  "description": "Return deterministic replan / stand candidates from the existing solver. Read-only. Never applies a schedule.",
  "parameters": {
    "type": "object",
    "properties": {
      "window_start": {"type": "string"},
      "window_end": {"type": "string"},
      "strategy": {"type": "string", "enum": ["stability", "balanced", "efficiency"]},
      "order_ids": {"type": "array", "items": {"type": "string"}}
    }
  }
}
```

结果必须带 `source=dispatch.list_solver_candidates`、每个候选 `object_id`、`as_of`。

**Commit:** `feat(ai): expose read-only solver candidates to dispatch_ops`

---

### Task I2: SolverFirst hook

**Files:**

- Modify: `hooks/pipeline.py` 新增 `SolverFirstHook`（PreToolUse）
- Modify: `templates/dispatch_ops.py` — `requires_plan_first` 保持 True；prompt 改为「先 `update_plan`，再 `dispatch.list_solver_candidates` 或 `ontology.explain_constraints`，最后才 `ontology.propose_action`」
- Test: `tests/sidecar/test_solver_first_hook.py`

**行为：** `task_type=dispatch_ops` 时，在本 run 尚未成功执行 `dispatch.list_solver_candidates`（或 `ontology.explain_constraints`）之前，拦截 `ontology.propose_action` 与一切 `WRITE_ACTION_TOOLS`。错误：`blocked_by=hook`，`rule=SolverFirstHook`。

`query_ops` / `anomaly_ops` 不启用。

**Commit:** `feat(ai): require solver or constraint check before dispatch proposals`

---

### Task I3: `Flight.change_stand` 先模拟再提案

**Files:**

- Modify: Rust `DomainActionExecutor` 或 ontology advisory：复用已有 stand 约束 / `stand.check_availability`，给 `Flight.change_stand` 增加 **dry-run 字段**（`simulate: true`）只返回 before/after/constraints，不写库
- 若现有 `flight.suggest_stand_adjustment` 已返回候选与约束，**优先让 `ontology.propose_action(Flight.change_stand)` 先强制 call advisory**，不要新动作名
- Modify: sidecar `propose_action`：受控写在生成 proposal 前必须附上 advisory/simulate 结果；硬约束失败 → 不建 proposal，返回 violations
- Test: Rust `ontology_actions` 已有 stand 测试则扩一条 simulate；sidecar `test_ontology_tools.py` 扩硬约束拒绝

**Commit:** `feat(ai): simulate stand change before creating a proposal`

---

### Task I4: 派工看板不再成为第二条环（收口，不做大重构）

**Files:**

- Modify: `frontend/vue-app/src/pages/dispatch_board/composables/useDispatchBoardPageAi.ts` 与 `AiDrawerSection.vue`
- Reuse: `frontend/ai-react` 的 `AiChatShell` / `PlanBoard` / `PendingActionCard`（与 `nl_query` 相同的 entry shell）
- **本 Task 范围：** 助手 Tab 改为嵌入现有 AI shell，`task_type=dispatch_ops`；情景推演/solver 数字继续用现有 snapshot API，但「应用」按钮只能触发 proposal 审批流，不能直接 `replan-apply`（除非 feature flag 且人工岗位，非 Agent）
- 不要把 Gantt/analytics 搬进 ai-react
- Test: 现有 dispatch board 单测 + `frontend/ai-react` 组件测；`npm run typecheck` / `npm run test`

**浏览器验收：**

1. 桌面打开派工看板 AI 抽屉，发一条建议，出现计划板
2. 批准前领域数据不变
3. 390 宽视口抽屉可用

**Commit:** `feat(frontend): run dispatch board assistant on dispatch_ops shell`

---

### Task I5: Phase I 验收

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar -k "solver or dispatch_ops or ontology" -q
cd services\api-server
cargo test -p fms-application -- ontology_actions
cd ..\..\frontend\vue-app
npm run typecheck
npm run test
```

**Exit：** 无 solver/约束结果不得出派工 proposal；`replan-apply` 不在 Agent 工具面；看板助手与 NL Query 同一套聊天状态。

---

## Phase J — Agent SLO 与费用（可与 I 并行）

目标：按 `task_type` × entity × 错误码切开；费用可告警。不引入 Jaeger。

### Task J1: 指标打点补齐 label

**Files:**

- Modify: `services/ai-sidecar/src/infrastructure/ai/monitoring/prometheus_exporter.py`
- Modify: `llm_stream_runner.py` / `tool_executor.py` / `mq_gate.py` 调用点
- Modify: `docs/observability/SLO.md`（新增 Agent 一节）
- Test: `tests/sidecar/test_prometheus_exporter.py`（扩 label）

**新增 / 扩展：**

| 指标 | labels | 说明 |
|---|---|---|
| `fms_ai_llm_calls_total` | `model`, `task_type`, `entity_id`, `status` | 现有缺 task_type |
| `fms_ai_tool_calls_total` | `tool`, `task_type`, `status`, `blocked_by` | 拦截可切片 |
| `fms_ai_tokens_total` | `model`, `type`, `task_type` | 费用分子 |
| `fms_ai_run_cost_usd`（counter） | `task_type`, `entity_id` | 用现有价目表常量，缺省 0 |
| `fms_ai_first_progress_seconds` | `task_type` | 已有 `FIRST_PROGRESS_TARGET_MS = 1500` |
| `fms_ai_resume_total` | `status` | 成功/失败 |
| `fms_ai_proposal_total` | `action`, `status` | created / approved / rejected / executed |

价目表放 `monitoring/model_prices.py` 常量（每 1M token USD），未知模型 cost=0 且 `price_missing=1`。

**Commit:** `feat(ai): label agent metrics by task type and record cost`

---

### Task J2: Grafana 面板与告警

**Files:**

- Modify: `deploy/docker/grafana/dashboards/fms-overview.json` 或新建 `fms-ai-agent.json` 并在 compose 挂载
- Modify: `deploy/docker/prometheus-rules/fms-slo-alerts.yml`
- Modify: `docs/observability/ALERT_RESPONSE.md`
- Test: 现有 prometheus 规则契约测试（搜 `fms-slo-alerts` 的 pytest/json 测试并扩）

**告警起步（warning，for: 15m）：**

| Alert | 条件 |
|---|---|
| `FmsAiUngroundedSpike` | `rate(fms_ai_errors_total{type="ungrounded"}[15m])` 明显高于 7 日基线，或绝对次数 > N（N 先写 10/15m，上线后改） |
| `FmsAiSidecarDown` | 已有，保留 |
| `FmsAiBudgetExhausted` | `run.budget_exhausted` 率 > 5% |
| `FmsAiFirstProgressSlow` | first progress p95 > 3s |

**Commit:** `feat(obs): add agent SLO panels and alerts`

---

### Task J3: 控制面延迟探针（不含 LLM）

**Files:**

- Modify: Rust lease / checkpoint / command enqueue 路径打 `fms_ai_controlplane_duration_seconds{op}`
- 目标：P99 ≤ 200ms（混合计划已写，本 Task 只让它可测）
- Test: 单元测 histogram observe；不要在 CI 里锁 200ms 墙钟

**Commit:** `feat(api): observe AI control-plane latency histograms`

---

### Task J4: Phase J 验收

**Exit：** Grafana 能按 `query_ops` / `dispatch_ops` 看失败率与 token；sidecar 挂了有告警文档；费用 counter 在假 runner 单测里增加。

---

## Phase K — 收敛表面与 HITL 产品化（第 8–10 周，可拆）

目标：降低认知负载；审批人看得懂。不做通用 undo。

### Task K1: 死物与文档对齐

**Files:**

- Delete: `services/ai-sidecar/src/infrastructure/ai/aip/__pycache__/`（及若仍无 `.py` 的空目录）
- Modify: `ARCHITECTURE_IMPROVEMENT_ROADMAP.md` 进度表 W2-4 与正文一致（若 F0 已做则本 Task 只扫残留）
- Modify: `docs/SOURCE_OF_TRUTH.md` 最新迁移号（当前磁盘是 `125_*`，文档仍写 `121_*` 的要一并纠正——只改事实，不改编号）
- Test: `test_docs_no_stale_references.py`

**Commit:** `chore(ai): remove leftover AIP bytecode and align tracker status`

---

### Task K2: 意图路由降为粗滤（原 E4，未做完就在这里收）

**Files:**

- Modify: `intent_router.py`
- Test: `test_intent_router_e4.py` — `task_type` 已给出时，关键词「机位」不得改路由

**Commit:** `fix(ai): do not override an explicit task_type in the intent router`

---

### Task K3: 审批卡 ontology-aware diff

**Files:**

- Modify: `frontend/ai-react/src/components/chat/PendingActionCard.tsx`
- Modify: Rust proposal payload（若还没有 before/after/constraints）：在 proposal ingest 时带上 I3 的 simulate 结果
- Test: `PendingActionCard` 组件测；无 constraints 时不崩溃

**必须展示：** 对象类型与 id、变更字段、硬约束违规（红）、软约束（黄）、不可逆标记、来源 run / tool。

**浏览器：** 桌面 + 390 宽各走一遍批准/驳回。

**Commit:** `feat(frontend): show constraint-aware diffs on pending actions`

---

### Task K4: 知识库关键词检索收尾（可选，不挡 F–J）

**Files:**

- Modify: `hybrid_retriever.py` `index_chunk` — 去掉 TODO，写入已有或 `126_ai_knowledge_chunks.sql`（仅当 124 没有等价表）
- Test: `test_hybrid_retriever_e2.py` 从架构空测改成「插入 chunk → 关键词能命中正文」

向量后端保持 port，默认 None。

**Commit:** `feat(ai): persist knowledge chunks for keyword retrieval`

---

### Task K5: W2-5 异常收敛（顺手，限热点）

**Files:** 优先 `todo_agent_executor`、`runtime_service/service.py`、`tools/tool_executor.py`、`llm_eval_service/service.py`

做法见路线图 W2-5：收窄 `except Exception`；关键路径结构化错误码。不要求清零。

**Commit:** `refactor(ai): narrow exception handling on agent hot paths`

---

## 4. 端到端验收场景

| # | 场景 | 期望 |
|---|---|---|
| 1 | 简单查询 | `query_ops` → `ontology.lookup` 或专用只读工具 → 中文答案 + evidence；无写动作；无 SQL |
| 2 | 查不存在的航班 | 工具 404 → 回答「没有这条」；不编造机位 |
| 3 | 过期 evidence | hook 打回重查或降级，最终陈述不含过期状态当现状 |
| 4 | 答案里多写一个航班号 | Stop hook 降级；eval `ungrounded_id_rate` 计入 |
| 5 | 异常研判 | `anomaly_ops` → 计划板 → 只读工具 → 假设标注 → 可选 proposal |
| 6 | 派工建议 | `dispatch_ops` → plan → solver 候选 → 解释 → pending；无候选不得提案 |
| 7 | 改机位硬约束失败 | simulate 失败，无 proposal 行 |
| 8 | 改机位可过约束 | proposal → 审批卡有 diff → 批准后 Rust 执行 + outbox |
| 9 | 越权 | 无 `flight:read` 的 run 调 lookup → 403，ledger `denied` |
| 10 | 断线恢复 | 杀 sidecar → resume 从 `after_tool` 续；`fms_ai_resume_total` +1 |
| 11 | Eval 回归 | 夹具红门禁不能合并（CI 单测）；Eval Lab 刷新不丢 job |
| 12 | 模板守门 | 有人在 prompt 里写未注册工具名 → `test_template_tools_are_registered` 红 |

性能口径（沿用混合计划，本计划使其可观测）：

| 指标 | 目标 |
|---|---|
| 控制面 P99（不含 LLM） | ≤ 200ms |
| 只读本体动作 P99 | ≤ 500ms |
| 首 progress | ≤ 1.5s（告警 3s） |
| 并发 run | 每实体 4 × 全局 32 |
| 越权当成功 | 0 |

---

## 5. 风险

| 风险 | 缓解 |
|---|---|
| 内部本体端点再造一套权限 | 复用 `read_action_permission`；权限只读 run 上 Rust 已存 requester |
| lookup 延迟叠在多轮工具上 | 对象图一次拉齐 relations；工作记忆spill；轮次预算不变 |
| 证据 hook 误伤合法叙述 | ID 抽取白名单+测试；假设段落在 anomaly 模板放行 |
| solver 与 Agent 候选不一致 | 同一 application service；禁止 sidecar 重算 |
| Eval 打真 LLM 导致 CI 不稳 | CI 只用 FakeRunner + 录制；真模型 job 留 Eval Lab |
| 看板嵌入壳破坏 Gantt | I4 只换助手 Tab，不动时间线 |
| 认知负载继续涨 | F6 守门 + K 删死物；不新增模板除非带夹具 |

---

## 6. 明确不在范围

- 新的 Rust `agent_runtime` crate / 平行状态机
- Python 公共 HTTP、gRPC、Async-Stdio、共享内存
- Temporal / CrewAI / 新可视化编排器
- 执行 skill 脚本
- 通用跨业务 undo
- 必选向量库 / Jaeger 上线门槛
- 把 `legacy-backend/` 加回功能
- 为每个域再写一套 Agent 类
- 本阶段做「多 Agent 组织架构」——`delegate` / `handoff` 已够，只修权限天花板与可见性（已在混合计划 C4，不在此重做）

---

## 7. 里程碑

| 周 | 交付 | 验收 |
|---|---|---|
| 1 | F0–F3 | 内部动作端点 + 无 stub 客户端 |
| 2 | F4–F7 | 三工具在目录里；query_ops 无 SQL；模板守门绿 |
| 3–4 | G1–G3、H1–H2 | FakeRunner 门禁可证伪；新鲜度/覆盖单测绿 |
| 5–6 | G4–G6、H3–H4 | Eval Lab 刷新不丢；Stop hook 降级无证据 ID |
| 7–8 | I1–I3、J1–J2 | 无 solver 不得提案；Grafana 按模板看费用 |
| 9–10 | I4–I5、J3–J4、K1–K3 | 看板同一壳；审批卡有约束；死物清理 |

---

## 8. 实施顺序与依赖

```text
F0
 └─ F1 (Rust 内部动作)
      └─ F2 (sidecar client)
           └─ F3 (去 stub)
                └─ F4 (目录+执行器)
                     ├─ F5 (模板+藏 SQL)
                     │    └─ F6 (模板守门)
                     └─ F7
G1 可与 F 并行写夹具
G2 依赖 F7（runner 才能调到真工具面；FakeRunner 可提前）
G3 依赖 G2
G4 依赖 checkpoint 已有（混合 D）
H1/H2 依赖 working_memory（混合 B2），可与 G3 并行
I1 依赖 F1 的内部面模式
I2 依赖 I1 + 现有 PlanFirstHook
I3 依赖 F3 propose_action
I4 依赖 I2（否则抽屉会绕过 solver-first）
J* 不依赖 I，可在 F7 后随时打点
K1 随时
K2 不依赖 F
K3 依赖 I3 payload
K4/K5 不挡发布
```

每个 Task：相关测试绿、一次独立 commit、conventional message、不要 `git add -A`。
