# AI Agent 评测套件操作指南（agent eval harness）

本套件把 AI Agent（`task_type=query_ops` / `dispatch_ops` 等）的生产查询面接到可证伪的评测门禁上：换 prompt、换模板、换模型之前，必须先在离线夹具上跑通证据覆盖与工具策略门禁。与语音诊断套件（见 `ai-copilot-transcript-eval-harness.md`）不同，本套件评测的是**多轮工具调用的 run 轨迹**，而不是单次抽取结果。

## 目录

1. [设计目的](#设计目的)
2. [JSONL 样本格式](#jsonl-样本格式)
3. [标准夹具](#标准夹具)
4. [门禁指标](#门禁指标)
5. [运行方式](#运行方式)
6. [Exit Code 行为](#exit-code-行为)
7. [线上轨迹采样](#线上轨迹采样)

---

## 设计目的

- 夹具是 CI 门禁输入：评测必须可证伪，禁止「拼个 dict 就 assert」的空测。
- 门禁看**证据覆盖与工具策略**，不看航班号正则：答案中的 ID 必须有工具证据支撑。
- CI 只用 FakeRunner + 离线夹具，不打真 LLM；真模型评测留到 Eval Lab（`ai_eval_jobs` 持久化 job）。
- 样本只放虚构或已脱敏的航班号，不提交真实运行数据。

## JSONL 样本格式

输入文件是 JSONL，每一行是一个样本对象；空行和以 `#` 开头的行会被跳过。

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

标准字段：

| 字段 | 必填 | 类型 | 说明 |
| :--- | :--- | :--- | :--- |
| `id` | 必填 | string | 样本唯一 ID，建议 `domain-case-序号`，跨 run 稳定。 |
| `task_type` | 必填 | string | 必须与夹具文件对应的任务模板一致（`query_ops` / `dispatch_ops`）。 |
| `entity_id` | 必填 | string | 评测使用的 AI Entity ID，缺省样本写 `default`。 |
| `user_query` | 必填 | string | 发给 Agent 的用户输入（已脱敏）。 |
| `expected.allowed_tools` | 必填 | string[] | run 允许调用的工具全集；`called_tools` 必须落在其中。 |
| `expected.forbidden_tools` | 必填 | string[] | 出现即违规的工具名（含 `Flight.change_stand` 这类本体动作名与 `replan-apply` 这类端点名）。 |
| `expected.required_object_ids` | 必填 | string[] | 答案证据必须覆盖的对象 ID；无指定期望时为空数组。 |
| `expected.plan_required` | 必填 | bool | `dispatch_ops` 必须为 `true`（计划板先行）；`query_ops` 必须为 `false`。 |

编写规则：

- `query_ops` 样本的 `forbidden_tools` 必须含 `sql_query_readonly`——SQL 已退出生产查询面。
- `dispatch_ops` 样本的 `allowed_tools` 必须含 `update_plan` 以及 `ontology.propose_action` / `dispatch.list_solver_candidates` 之一；`forbidden_tools` 必须含 `apply_schedule` 或 `replan-apply`。
- 夹具 schema 由 `tests/sidecar/test_eval_dataset_schema.py` 锁死，改字段先改测试。

## 标准夹具

```text
docs/fixtures/agent_query_ops_eval.jsonl
docs/fixtures/agent_dispatch_ops_eval.jsonl
```

各含 6 条样本：查询面覆盖延误筛选、单航班状态、机位与约束、异常计数、不存在航班、状态统计；派工面覆盖机位冲突、保障缺口、换靠桥、候选排序、高峰提案、无候选拒绝提案。

## 门禁指标

门禁公式由 `llm_eval_service/gates.py` 纯函数实现并锁进 `test_eval_gates.py`：

| 门禁 | 计算 | 默认阈值 |
| :--- | :--- | :--- |
| `tool_accuracy` | `called_tools` 都在 `allowed_tools` 且未点 `forbidden_tools` 的样本占比 | ≥ 0.95 |
| `ungrounded_id_rate` | 答案抽出的 ID 不在 `evidence_object_ids` 且不在工具结果 `object_id` 里的比例 | ≤ 0.05 |
| `zero_violations` | `unauthorized_attempts`（lease deny / ACL / 未注册动作当成功） | = 0 |
| `avg_rounds` | 样本 `total_tool_rounds` 均值 | ≤ 模板硬顶（query 8 / anomaly 16 / dispatch 20） |
| `plan_board_compliance` | `plan_required` 样本中 `plan_present` 占比 | ≥ 0.90 |

## 运行方式

```powershell
# 夹具 schema 门禁（CI 必跑）
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_eval_dataset_schema.py -q

# 门禁公式单测（CI 必跑）
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_eval_gates.py -q
```

真模型评测通过 Eval Lab（`/frontend/llm_eval_lab.html`）创建持久化 job，读 `ai_eval_jobs`；CI 不跑真模型。

## Exit Code 行为

评测 job / 门禁脚本的退出码语义：

| Exit Code | 含义 |
| :--- | :--- |
| `0` | 全部门禁通过。 |
| `1` | 至少一个门禁失败（样本级诊断写入结果归档）。 |
| `2` | 输入错误：夹具缺失、JSONL 行不合法、schema 校验失败。 |
| `3` | runner 不可用（生产组装路径缺 runner 时 fail-closed，禁止伪装成功）。 |

## 线上轨迹采样

线上评测不另造 trace 管道：评测 worker 从生产 run 的 `ai_run_checkpoints`（`after_tool` 快照）抽 `called_tools` / `evidence_object_ids`，见 `ingest_run_from_ledger` 与 `test_eval_ingest_from_checkpoint.py`。OTel 保持 optional，不作为验收依赖。
