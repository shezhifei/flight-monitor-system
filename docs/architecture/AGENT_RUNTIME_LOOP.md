# Agent 运行时执行环契约（AGENT_RUNTIME_LOOP）

> 本文是混合 Agent 架构（`docs/plans/2026-08-14-hybrid-agent-architecture.md`）的执行环冻结文档。
> 目标是防止再造第四套运行时。任何新增 Agent 执行路径必须先与本文对齐，否则不得合入。

| 字段 | 值 |
|------|-----|
| 状态 | Active（契约冻结） |
| 编制日期 | 2026-08-14 |
| 事实源 | `services/ai-sidecar/src/infrastructure/ai/`、`docs/SOURCE_OF_TRUTH.md` |

---

## 1. 唯一生产环

系统只有一条生产执行环：

```text
Rust 控制面（job/run · lease · checkpoint · proposal · 审批 · 领域写入 · SSE）
        │  ContextEnvelope / ai_runtime_commands / ai.runtime.events（RocketMQ）
        ▼
Python sidecar —— LLMStreamRunner.stream_chat_with_tools()（工具轮次环）
        │  hooks → 真工具 / MCP / 按需 skill / 本体工具 → 工作记忆
        ▼
Rust DomainActionExecutor → 业务表 + outbox
```

### 1.1 生产入口与循环

| 层 | 组件 | 契约 |
|----|------|------|
| 生产入口 | `RuntimeService.stream_run_with_tools`（`runtime_service/_streaming_tools.py`） | 唯一允许被 job worker / 控制面调用的流式工具执行入口 |
| 工具轮次环 | `LLMStreamRunner.stream_chat_with_tools`（`llm_stream_runner.py`） | 唯一的 LLM ↔ 工具多轮循环；轮次预算按模板配置，不再只依赖 `MAX_TOOL_ROUNDS = 5` 单一上限 |
| 只读预分类 | `RuntimeGraph`（`runtime_graph.py`） | **可选**：validate → enrich → assemble_prompt 的只读前置；不得执行工具、不得写库、不得成为第二条业务环 |
| LangGraph 试点 | `graph/builder.py` + `domain/ai/todo_graph_pilot.py` | 仅限 Todo 试点 + checkpoint/interrupt；**禁止**再增加业务节点 |
| 失败语义 | `run.fail` + 结构化错误码 | 无工具快照（`AI_TOOL_SNAPSHOT_MISSING`）、capability 缺失均 fail-closed |

### 1.2 明确禁止

1. **禁止新增 `BaseAgent.run()` 模板方法环**（`observe/act` 长循环）——任何新的「Agent 类 + 主循环」都会被拒绝。
2. **禁止 `RuntimeGraph` 执行工具或直连业务 API**——它只能产出 context，不产出副作用。
3. **禁止 `graph/builder.py` 追加业务节点**——LangGraph 只保留 Todo 试点与 checkpoint/interrupt 用途。
4. **禁止侧车直写业务表**（`flights` / `dispatch_orders` / `todos` / `business_cases` / `domain_event_outbox`）——写操作一律生成 proposal，由 Rust `DomainActionExecutor` 执行。
5. **禁止生产路径 mock 工具回落**——`resolved_config` 缺失或 `tools == []` 时 fail-closed，不得回落到 `READ_ONLY_TOOL_SCHEMAS` 假数据。
6. **禁止 Python 公共 HTTP / gRPC / Async-Stdio / 共享内存传输层**——控制事件只走 RocketMQ，命令只走 `ai_runtime_commands`。

## 2. 模板策略（不是新循环）

`task_type` / 实体配置决定「策略」，不决定「循环」：

| 模板 | 策略 | 轮次预算（默认 / 硬顶） |
|------|------|--------------------------|
| `query_ops` | 只陈述事实；必须引用 evidence；禁止写动作工具 | 6 / 8 |
| `anomaly_ops` | 先列异常/KPI → 根因假设 → 建议（proposal） | 12 / 16 |
| `dispatch_ops` | 只读现状 → 候选 → LLM 排序解释 → 高风险 waiting_for_approval | 16 / 20 |
| 未识别 | 通用 | 8 / 12 |

新增模板 = 一个策略文件 + 实体绑定 + eval cases，**不新增循环**。

## 3. 三层边界（对应 Rust 控制面）

| 面 | 归属 | 说明 |
|----|------|------|
| 控制面 | Rust | job/run、lease、checkpoint、proposal、审批、领域写入、SSE |
| 推理面 | Python sidecar | 系统提示、工具轮次、结构化输出、工作记忆、hooks |
| 持久化真相 | Rust | 写侧只落库 + outbox（ADR-0002 / ADR-0003） |

## 4. 验收锚点

- `tests/tools/test_architecture_docs_consistency.py`：文档一致性测试，修改本文涉及清单需同步。
- sidecar 测试：`test_read_only_tools_no_mock.py`、`test_streaming_tools_require_resolved_snapshot.py`（见计划 Task A2）。
- 控制面测试：`cargo test -p fms_api -- routes::ai`（见计划 Task A7）。
