# Palantir AIP 模式重构实施计划

> **历史文档（非运行时事实源）**  
> 创建：2026-05-08。仅作早期 AIP/Ontology 原型记录。  
> 当前事实以 `docs/SOURCE_OF_TRUTH.md`、`docs/SYSTEM_MANUAL.md` 与 `services/api-server/` 为准。  
> 文中 Python/LangGraph 直连业务动作的描述已过时；HTTP/SSE、权限、审批与生产写入由 Rust 承接，Python 只做 AI 侧车（及可选 worker）。

---

## 一、总体目标

将航班监控系统从现有的**工具驱动架构**升级为**Palantir AIP模式**的**语义驱动架构**，实现：

1. **语义层**：统一的 Ontology 对象建模（Flight, Stand, Team 等）
2. **权限层**：从工具级升级到对象+实例级权限控制
3. **执行层**：Action-based 执行框架，集成 HITL 审批
4. **推理层**：Schema 驱动的 LLM 上下文注入，增强推理能力

---

## 二、实施阶段总览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              实施阶段总览                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Phase 1: 核心集成          Phase 2: 工具桥接        Phase 3: 增强完善      │
│  ═══════════════════        ═════════════════        ═════════════════     │
│  • LangGraph节点集成         • Legacy工具适配          • Ontology查询       │
│  • Action Handler绑定        • 双轨并行运行            • 约束推理          │
│  • 数据源集成                • ACL迁移                 • 审批增强          │
│                                                                             │
│  Phase 4: 测试验证          Phase 5: 生产部署                                │
│  ═══════════════════        ══════════════════                               │
│  • 单元测试                 • 灰度发布                                        │
│  • 集成测试                 • 监控指标                                        │
│  • 性能测试                 • 回滚方案                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 三、详细实施计划

### Phase 1: 核心集成

**目标**：让 AIP 模块能实际进入 LangGraph 执行流程

**时间估算**：2-3 周

#### Week 1-2: LangGraph 节点集成

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 1-3: 状态扩展                                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 1.1: 扩展 AgentCoreState                                               │
│ ─────────────────────────────────────────────────────────────────────────  │
│ 文件: src/infrastructure/ai/graph/state.py                                 │
│                                                                             │
│ 变更:                                                                       │
│ class AIPAgentState(AgentCoreState):                                        │
│     object_context: Optional[Dict[str, Any]]  # 当前操作的对象上下文         │
│     action_queue: List[Dict[str, Any]]        # 待执行的动作队列             │
│     resolved_objects: Dict[str, Any]         # 已解析的对象缓存            │
│     pending_approvals: List[Dict[str, Any]]   # 待审批的变更                │
│                                                                             │
│ 验收标准:                                                                   │
│ □ AgentCoreState 继承 AIPAgentState 无报错                                  │
│ □ 新字段不影响现有状态流转                                                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 4-7: 创建 AIP 专用节点                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 1.2: 创建 aip_nodes.py                                                 │
│ ─────────────────────────────────────────────────────────────────────────  │
│ 文件: src/infrastructure/ai/graph/aip_nodes.py [新建]                       │
│                                                                             │
│ 内容:                                                                       │
│                                                                             │
│ def object_action_node(state: AIPAgentState, config) -> Dict:              │
│     '''                                                                    │
│     AIP专用节点：执行对象动作                                                │
│     1. 从 state.messages 提取 LLM 的工具调用                                │
│     2. 调用 AIPFunctionRegistry.resolve_action()                           │
│     3. 调用 ObjectACL.check_permission()                                   │
│     4. 调用 AIPActionExecutor.execute()                                    │
│     5. 处理 PENDING_APPROVAL 状态，注入 interrupt                           │
│     '''                                                                    │
│                                                                             │
│ def ontology_query_node(state: AIPAgentState, config) -> Dict:              │
│     '''                                                                    │
│     AIP专用节点：本体查询                                                    │
│     利用对象模型的关系进行语义查询                                            │
│     '''                                                                    │
│                                                                             │
│ 验收标准:                                                                   │
│ □ object_action_node 能正确解析 function call                               │
│ □ 权限检查失败时返回错误消息                                                 │
│ □ 需要审批时触发 approval_node                                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 8-10: Graph Builder 集成                                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 1.3: 修改 Graph Builder                                                 │
│ ─────────────────────────────────────────────────────────────────────────  │
│ 文件: src/infrastructure/ai/graph/builder.py                                │
│                                                                             │
│ 变更:                                                                       │
│                                                                             │
│ class AIAgentBuilder:                                                       │
│     def __init__(self, config: AIConfig):                                   │
│         # ... 现有代码 ...                                                  │
│                                                                             │
│         # 新增 AIP 组件                                                     │
│         self.aip_app = None  # AIPApplication 实例                          │
│                                                                             │
│     def with_aip(self, enabled: bool = True):                               │
│         '''启用 AIP 模式'''                                                 │
│         if enabled:                                                         │
│             from src.infrastructure.ai.aip.app import get_aip_app            │
│             self.aip_app = get_aip_app()                                    │
│         return self                                                         │
│                                                                             │
│     def build_graph(self):                                                  │
│         # ... 现有代码 ...                                                  │
│                                                                             │
│         # 在适当位置插入 AIP 节点                                            │
│         if self.aip_app:                                                    │
│             # 将 object_action_node 接入图的执行链                           │
│             pass                                                             │
│                                                                             │
│ 验收标准:                                                                   │
│ □ Builder 支持 with_aip() 调用                                              │
│ □ AIP 启用时能正确加载组件                                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Week 3: Action Handler 绑定

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 11-15: 业务逻辑绑定                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 1.4: 实现 Action Handler                                                │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 需要为每个 Ontology Action 实现业务处理逻辑：                                 │
│                                                                             │
│ Flight Actions:                                                             │
│ ├── change_stand      → FlightService.change_stand(flight_id, new_stand)     │
│ ├── delay_flight     → FlightService.update_delay(flight_id, minutes)       │
│ ├── assign_team      → FlightService.assign_team(flight_id, team_id)        │
│ ├── update_status    → FlightService.update_status(flight_id, status)      │
│ ├── mark_arrived     → FlightService.mark_arrived(flight_id)                │
│ └── mark_departed    → FlightService.mark_departed(flight_id)              │
│                                                                             │
│ Stand Actions:                                                              │
│ ├── occupy           → StandService.occupy(stand_id, flight_id)             │
│ ├── release         → StandService.release(stand_id)                       │
│ ├── reserve         → StandService.reserve(stand_id, flight_id, time_range) │
│ ├── close           → StandService.close(stand_id, reason)                 │
│ └── update_status   → StandService.update_status(stand_id, status)         │
│                                                                             │
│ Team Actions:                                                               │
│ ├── assign_flight   → TeamService.assign_flight(team_id, flight_id)         │
│ ├── update_status  → TeamService.update_status(team_id, status)           │
│ └── change_location → TeamService.update_location(team_id, location)        │
│                                                                             │
│ 任务 1.5: 注册 Handler 到 AIPApplication                                    │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/app.py                                     │
│                                                                             │
│ async def _register_action_handlers(self):                                   │
│     '''                                                                   │
│     在初始化时注册所有 Action Handler                                       │
│     '''                                                                   │
│                                                                             │
│     from src.domain.services.flight_service import FlightService            │
│     from src.domain.services.stand_service import StandService             │
│     from src.domain.services.team_service import TeamService               │
│                                                                             │
│     flight_service = FlightService(...)                                     │
│     stand_service = StandService(...)                                       │
│     team_service = TeamService(...)                                         │
│                                                                             │
│     # Flight handlers                                                       │
│     self.register_action_handler("Flight", "change_stand",                 │
│         lambda oid, params: flight_service.change_stand(...))              │
│                                                                             │
│ 验收标准:                                                                   │
│ □ 所有 Flight/Stand/Team Actions 有对应的 Handler                            │
│ □ Handler 能正确调用业务服务                                                │
│ □ 执行结果能正确返回                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Week 4: 数据源集成

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 16-20: 对象状态获取                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 1.6: 实现 _get_object_state()                                            │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/action_executor.py                          │
│                                                                             │
│ async def _get_object_state(self, object_type, object_id):                 │
│     '''从实际数据源获取对象当前状态'''                                        │
│                                                                             │
│     if object_type == "Flight":                                             │
│         flight = await self._flight_repo.get_by_id(object_id)               │
│         return {                                                            │
│             "flight_id": flight.id,                                         │
│             "flight_number": flight.flight_number,                          │
│             "stand": flight.stand,                                          │
│             "status": flight.status.value,                                  │
│             "assigned_team_id": flight.assigned_team_id,                    │
│             ...                                                             │
│         }                                                                   │
│                                                                             │
│     elif object_type == "Stand":                                            │
│         stand = await self._stand_repo.get_by_id(object_id)                  │
│         return {...}                                                        │
│                                                                             │
│     elif object_type == "Team":                                            │
│         team = await self._team_repo.get_by_id(object_id)                   │
│         return {...}                                                        │
│                                                                             │
│ 任务 1.7: 实现 _simulate_action()                                           │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│     def _simulate_action(self, current_state, action, parameters):          │
│         '''模拟 Action 执行后的状态'''                                        │
│                                                                             │
│         after_state = current_state.copy()                                  │
│                                                                             │
│         # 根据 Action 类型更新对应字段                                       │
│         if action == "change_stand":                                        │
│             after_state["stand"] = parameters.get("new_stand")              │
│         elif action == "delay_flight":                                      │
│             after_state["delay_minutes"] = parameters.get("delay_minutes")  │
│         elif action == "update_status":                                     │
│             after_state["status"] = parameters.get("status")                │
│         ...                                                                 │
│                                                                             │
│         return after_state                                                  │
│                                                                             │
│ 验收标准:                                                                   │
│ □ _get_object_state 能从数据库获取真实数据                                  │
│ □ _simulate_action 能正确模拟状态变更                                        │
│ □ Diff 计算基于真实数据                                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Phase 2: 工具桥接

**目标**：复用现有 ToolRegistry，平滑迁移

**时间估算**：1-2 周

#### Week 5: Legacy 适配

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 21-25: 工具适配集成                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 2.1: 集成 LegacyToolAdapter                                            │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/app.py                                     │
│                                                                             │
│ async def _init_legacy_adapter(self):                                       │
│     '''                                                                   │
│     从 ToolRegistry 迁移现有工具到 AIP Function Registry                    │
│     '''                                                                   │
│                                                                             │
│     from src.infrastructure.ai.tools.registry import get_tool_registry      │
│                                                                             │
│     tool_registry = get_tool_registry()                                     │
│     legacy_tools = []                                                       │
│                                                                             │
│     for (tool_def, category) in tool_registry._tools.values():              │
│         legacy_tools.append(tool_def)                                        │
│                                                                             │
│     # 使用 LegacyToolAdapter 批量适配                                       │
│     adapted_count = self.adapt_legacy_tools(legacy_tools)                   │
│     logger.info(f"Adapted {adapted_count} legacy tools to AIP functions")  │
│                                                                             │
│ 任务 2.2: 定义 Tool → AIP Function 映射                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/legacy_adapter.py (扩展)                    │
│                                                                             │
│ # 自定义映射配置                                                            │
│ TOOL_MAPPINGS = {                                                           │
│     "change_flight_stand": {                                                │
│         "object_type": "Flight",                                           │
│         "action_name": "Flight.change_stand",                              │
│         "requires_approval": True,                                          │
│         "risk_level": "MEDIUM"                                             │
│     },                                                                      │
│     "create_todo": {                                                        │
│         "object_type": "Todo",                                             │
│         "action_name": "Todo.create",                                       │
│         "requires_approval": False,                                        │
│         "risk_level": "LOW"                                                │
│     },                                                                      │
│     # ... 更多映射                                                          │
│ }                                                                           │
│                                                                             │
│ 验收标准:                                                                   │
│ □ Legacy 工具能正确适配为 AIP Function                                      │
│ □ 映射配置正确处理                                                          │
│ □ 双轨并行时无冲突                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Week 6: 双轨并行

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 26-30: 双轨运行模式                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 2.3: 实现模式切换                                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: config/ai_config.py                                                  │
│                                                                             │
│ class AIPConfig(BaseModel):                                                 │
│     enabled: bool = False                    # 总开关                        │
│     mode: str = "aip_only"                  # aip_only | legacy_only | dual │
│     legacy_fallback: bool = True             # AIP 失败时回退到 Legacy        │
│     migration_progress: float = 0.0          # 迁移进度 0.0-1.0              │
│                                                                             │
│ 任务 2.4: 实现回退机制                                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/app.py                                     │
│                                                                             │
│ async def execute_with_fallback(self, ...):                                 │
│     '''                                                                   │
│     支持回退的执行方式                                                      │
│     1. 尝试 AIP 模式执行                                                    │
│     2. 如果失败且 legacy_fallback=True，回退到 Legacy                        │
│     '''                                                                   │
│                                                                             │
│     try:                                                                     │
│         return await self.execute_action(...)                                │
│     except AIPExecutionError:                                               │
│         if not self.config.aip.legacy_fallback:                             │
│             raise                                                           │
│         logger.warning("AIP execution failed, falling back to legacy")       │
│         return await self._legacy_execute(...)                               │
│                                                                             │
│ 验收标准:                                                                   │
│ □ 支持三种运行模式切换                                                      │
│ □ 回退机制正常工作                                                          │
│ □ 降级不影响现有功能                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Phase 3: 增强完善

**目标**：实现高级 AIP 功能

**时间估算**：2-3 周

#### Week 7: Ontology 查询增强

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 31-35: 本体查询能力                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 3.1: 实现 ontology_query_node                                           │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ def ontology_query_node(state: AIPAgentState, config) -> Dict:              │
│     '''                                                                   │
│     利用对象模型的关系进行语义查询                                            │
│                                                                             │
│     支持:                                                                   │
│     - 跨对象关联查询 (Flight → Team → Equipment)                             │
│     - 基于约束的推理 (找所有可用的大机位)                                     │
│     - 路径查询 (查询某航班的完整上下文)                                      │
│     '''                                                                   │
│                                                                             │
│ 任务 3.2: 实现关系遍历                                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ class OntologyPathResolver:                                                 │
│     '''Ontology 路径解析器'''                                                │
│                                                                             │
│     def resolve_path(self, start_object, path: str):                        │
│         '''                                                                │
│         解析路径表达式                                                      │
│         path: "Flight.assigned_team.equipment"                              │
│         返回: 某航班的班组及其设备                                           │
│         '''                                                                │
│                                                                             │
│ 验收标准:                                                                   │
│ □ 支持跨对象关系查询                                                        │
│ □ 支持路径表达式解析                                                        │
│ □ 查询性能可接受                                                            │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Week 8-9: 约束推理

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 36-45: 约束推理引擎                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 3.3: 实现约束定义                                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/ontology/constraints.py [新建]                  │
│                                                                             │
│ class ConstraintDefinition:                                                 │
│     '''约束定义'''                                                          │
│                                                                             │
│     def __init__(self, name, object_type, condition, error_message):        │
│         self.name = name                                                    │
│         self.object_type = object_type                                      │
│         self.condition = condition  # Lambda 或表达式                         │
│         self.error_message = error_message                                  │
│                                                                             │
│ # 预定义约束                                                                │
│ FLIGHT_CONSTRAINTS = [                                                      │
│     ConstraintDefinition(                                                   │
│         "stand_capacity",                                                   │
│         "Flight",                                                           │
│         lambda flight, new_stand: stand_service.check_capacity(              │
│             new_stand, flight.aircraft_type                                  │
│         ),                                                                  │
│         "新机位无法容纳该机型"                                               │
│     ),                                                                      │
│     ConstraintDefinition(                                                   │
│         "stand_available",                                                  │
│         "Flight",                                                           │
│         lambda flight, new_stand: stand_service.is_available(new_stand),   │
│         "机位不可用"                                                        │
│     ),                                                                      │
│ ]                                                                           │
│                                                                             │
│ 任务 3.4: 约束检查集成                                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/action_executor.py                          │
│                                                                             │
│ async def _check_constraints(self, object_type, action, parameters):         │
│     '''                                                                   │
│     在 Action 执行前检查约束                                                │
│     '''                                                                   │
│                                                                             │
│     constraints = get_constraints_for_action(object_type, action)            │
│     violations = []                                                         │
│                                                                             │
│     for constraint in constraints:                                          │
│         if not constraint.condition(object_type, parameters):               │
│             violations.append(constraint.error_message)                     │
│                                                                             │
│     if violations:                                                          │
│         raise ConstraintViolationError(violations)                          │
│                                                                             │
│ 验收标准:                                                                   │
│ □ 约束定义完整覆盖关键业务规则                                              │
│ □ 约束检查在 Action 执行前触发                                              │
│ □ 违反约束时返回清晰错误信息                                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Week 10: HITL 审批增强

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 46-50: 审批体验增强                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 3.5: 实现 Ontology-Aware Diff                                          │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/object_change_diff.py [新建]               │
│                                                                             │
│ class OntologyObjectDiff:                                                   │
│     '''基于 Schema 的变更差异计算'''                                        │
│                                                                             │
│     def compute_diff(self, schema, before, after):                         │
│         '''                                                                │
│         1. 只计算 Schema 中定义属性的变更                                    │
│         2. 标注关键属性变更                                                  │
│         3. 检测关系变更                                                     │
│         4. 计算风险等级                                                     │
│         '''                                                                │
│                                                                             │
│ 任务 3.6: 审批 UI 数据增强                                                  │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 文件: src/infrastructure/ai/aip/action_executor.py                        │
│                                                                             │
│ # 扩展 PendingAction 的 ui_hints                                            │
│ ui_hints = {                                                                │
│     "show_diff": True,                                                      │
│     "diff_source": "ontology_aware",                                        │
│     "critical_properties": ["status", "assigned_team"],                    │
│     "affected_relationships": [                                             │
│         {"type": "assigned_team", "count": 1, "preview": "T001 甲班"}       │
│     ],                                                                      │
│     "constraint_warnings": [                                                │
│         "变更后可能导致保障超时"                                             │
│     ],                                                                      │
│     "schema_info": {                                                        │
│         "object_type": "Flight",                                           │
│         "action": "change_stand"                                           │
│     }                                                                       │
│ }                                                                           │
│                                                                             │
│ 验收标准:                                                                   │
│ □ Diff 计算包含 Schema 感知信息                                             │
> □ UI 能展示关键属性和关系变更                                               │
│ □ 审批人员能看到完整的变更影响                                              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Phase 4: 测试验证

**目标**：确保功能正确性

**时间估算**：1-2 周

#### Week 11: 测试覆盖

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 51-55: 测试完善                                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 4.1: 单元测试                                                           │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 目录: tests/infrastructure/ai/test_aip_*.py                                │
│                                                                             │
│ 覆盖:                                                                       │
│ ├── test_function_registry.py      # 函数注册、解析、权限                    │
│ ├── test_object_acl.py             # 权限检查、条件、继承                    │
│ ├── test_action_executor.py        # 执行、审批、Diff                       │
│ ├── test_context_bridge.py         # 提示词生成                            │
│ ├── test_ontology_schema.py        # 对象定义、验证                        │
│ └── test_legacy_adapter.py         # 工具适配                              │
│                                                                             │
│ 任务 4.2: 集成测试                                                           │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 覆盖:                                                                       │
│ ├── test_aip_graph_integration.py   # LangGraph 集成                       │
│ ├── test_action_handler_binding.py  # Handler 绑定                         │
│ ├── test_hitl_workflow.py          # 审批流程                              │
│ └── test_legacy_fallback.py        # 回退机制                              │
│                                                                             │
│ 任务 4.3: 端到端测试                                                         │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 场景:                                                                       │
│ 1. 用户请求变更航班机位 → LLM → AIP → 审批 → 执行 → 结果                    │
│ 2. 权限不足的用户尝试操作 → 权限检查 → 拒绝                                │
│ 3. 约束检查失败 → 错误提示 → 不执行                                        │
│                                                                             │
│ 验收标准:                                                                   │
│ □ 单元测试覆盖率 > 80%                                                     │
│ □ 关键路径集成测试通过                                                      │
│ □ 端到端场景验证通过                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Phase 5: 生产部署

**目标**：安全上线

**时间估算**：1-2 周

#### Week 12-13: 部署准备

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Day 56-65: 部署实施                                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ 任务 5.1: 灰度发布策略                                                      │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ Week 1: 1% 流量                                                             │
│ - 仅内部用户启用 AIP                                                        │
│ - 监控错误率和延迟                                                         │
│                                                                             │
│ Week 2: 10% 流量                                                            │
│ - 扩大用户范围                                                              │
│ - 收集反馈                                                                  │
│                                                                             │
│ Week 3: 50% 流量                                                            │
│ - 稳定后继续扩大                                                            │
│                                                                             │
│ Week 4: 100% 流量                                                           │
│ - 全部用户启用                                                              │
│ - 准备关闭 Legacy 模式                                                      │
│                                                                             │
│ 任务 5.2: 监控指标                                                          │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 指标:                                                                       │
│ • aip_action_execution_total      # Action 执行总数                         │
│ • aip_action_execution_duration   # 执行耗时                                │
│ • aip_approval_required_total     # 需要审批的 Action                       │
│ • aip_approval_approved_total     # 审批通过数                              │
│ • aip_permission_denied_total     # 权限拒绝数                              │
│ • aip_constraint_violation_total  # 约束违反数                              │
│ • aip_legacy_fallback_total      # 回退到 Legacy 的次数                    │
│                                                                             │
│ 任务 5.3: 回滚方案                                                          │
│ ─────────────────────────────────────────────────────────────────────────  │
│                                                                             │
│ 回滚触发条件:                                                               │
│ • 错误率 > 1% (正常 < 0.1%)                                                 │
│ • P99 延迟 > 5s (正常 < 1s)                                                  │
│ • 审批队列积压 > 100                                                       │
│                                                                             │
│ 回滚操作:                                                                   │
│ 1. 配置 AIP mode = "legacy_only"                                           │
│ 2. 重启服务                                                                  │
│ 3. 验证 Legacy 模式正常                                                     │
│                                                                             │
│ 验收标准:                                                                   │
│ □ 灰度发布顺利完成                                                          │
> □ 监控指标正常                                                              │
│ □ 回滚机制可正常工作                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 四、验收标准总表

| Phase | 验收标准 | 成功条件 |
|-------|----------|----------|
| **Phase 1** | LangGraph 集成 | AIP 节点能正确处理 Action 调用 |
| **Phase 1** | Handler 绑定 | 所有 Actions 有业务逻辑绑定 |
| **Phase 1** | 数据源集成 | 能从数据库获取真实对象状态 |
| **Phase 2** | 工具适配 | Legacy 工具正确适配为 AIP Function |
| **Phase 2** | 双轨运行 | 支持模式切换和回退 |
| **Phase 3** | 约束推理 | 关键业务约束正确执行 |
| **Phase 3** | HITL 增强 | Diff 包含 Schema 信息 |
| **Phase 4** | 测试覆盖 | 单元测试 > 80% |
| **Phase 5** | 灰度发布 | 100% 流量稳定运行 1 周 |

---

## 五、风险与缓解

| 风险 | 影响 | 缓解措施 | 责任人 |
|------|------|----------|--------|
| Handler 实现复杂 | 时间超期 | 复用现有服务，简化适配层 | TBD |
| LLM 推理质量不稳定 | 意图理解错误 | 增加 Prompt 工程，持续优化 | TBD |
| 性能下降 | 用户体验差 | 缓存优化，异步处理 | TBD |
| 与现有功能冲突 | 回归问题 | 充分的测试覆盖 | TBD |
| 迁移风险 | 生产事故 | 灰度发布，快速回滚 | TBD |

---

## 六、团队需求

| 角色 | 人数 | 职责 |
|------|------|------|
| 后端开发 | 2 | Handler 实现、集成、测试 |
| AI/ML 工程师 | 1 | Prompt 工程、LLM 调优 |
| 测试工程师 | 1 | 测试用例、自动化 |
| 产品经理 | 1 | 需求澄清、验收标准定义 |

---

## 七、后续迭代方向

完成基础 AIP 模式后，可考虑以下增强：

1. **动态 Ontology**：运行时修改 Schema，支持业务快速迭代
2. **多 Agent 协作**：不同 Agent 操作不同对象类型，协作完成复杂任务
3. **知识图谱集成**：与现有知识库打通，增强推理能力
4. **自适应权限**：基于用户行为动态调整权限策略

---

## 执行跟踪

| 日期 | 阶段 | 完成情况 | 备注 |
|------|------|----------|------|
| 2026-05-08 | Phase 0 (基础模块) | ✅ 完成 | 已创建 Ontology + Function Registry + Object ACL |
| TBD | Phase 1 | 🔲 待开始 | LangGraph 集成 |
| TBD | Phase 2 | 🔲 待开始 | 工具桥接 |
| TBD | Phase 3 | 🔲 待开始 | 增强完善 |
| TBD | Phase 4 | 🔲 待开始 | 测试验证 |
| TBD | Phase 5 | 🔲 待开始 | 生产部署 |
