"""
Ontology 驱动 AI 示例

演示 Ontology 如何真正影响 AI 行为。
"""


def demo_ontology_drives_ai():
    """
    演示 Ontology 的核心价值

    场景：用户通过 AI 更改航班停机位
    """

    print("=" * 60)
    print("Ontology 驱动 AI 行为演示")
    print("=" * 60)

    print("\n【场景】用户问: '把 CA123 航班的停机位改到 A5'")
    print()

    print("【1. 没有 Ontology 的情况】")
    print("-" * 40)
    print("""
AI 收到请求，但没有结构化上下文：
- 不知道有哪些对象类型
- 不知道对象有哪些属性
- 不知道有哪些可执行的动作
- 可能生成无效的参数或调用错误的方法

AI 输出：
"我想把航班 CA123 的停机位改到 A5"

问题：
- LLM 可能不知道 'stand' 是 'Flight' 的一个属性
- 可能生成错误的参数名: change_stand() vs updateStand()
- 不知道哪些操作需要审批
""")

    print("\n【2. 有 Ontology 的情况】")
    print("-" * 40)
    print("""
AI 收到请求 + Ontology 上下文：

 Ontology 定义:
 ┌─────────────────────────────────────────────────────────────┐
 │ Object: Flight                                               │
 │ ├─ properties:                                               │
 │ │   ├─ flight_number: string (required)                      │
 │ │   ├─ stand: string                                         │
 │ │   ├─ status: enum[scheduled, arrived, departed]           │
 │ │   └─ eta, etd: datetime                                   │
 │ ├─ relationships:                                            │
 │ │   └─ stand → Stand (当前停靠的停机位)                      │
 │ └─ actions:                                                 │
 │     ├─ change_stand(stand_id: string) → 需要审批 ⚠️         │
 │     ├─ delay_flight(delay_minutes: integer)                  │
 │     └─ update_status(status: enum) → 需要审批 ⚠️             │
 └─────────────────────────────────────────────────────────────┘

AI 现在的输出：

"我理解您想把航班 CA123 的停机位改到 A5。
 让我先检查一下...

  Ontology 分析:
  ✓ 对象类型: Flight
  ✓ 动作: change_stand
  ✓ 参数: stand_id = "A5"
  ✓ 风险等级: MEDIUM
  ✓ 需要审批: 是

  是否确认执行此操作？"
""")

    print("\n【3. 修改 Ontology 配置的影响】")
    print("-" * 40)
    print("""
场景：管理员通过前端修改 Ontology 配置

修改前 - change_stand 动作定义:
{
  "name": "change_stand",
  "risk_level": "MEDIUM",
  "requires_approval": true,    ← 需要审批
  "parameters": [
    {"name": "stand_id", "type": "string", "required": true}
  ]
}

修改后 - 管理员认为这个操作很常见，降低风险:
{
  "name": "change_stand",
  "risk_level": "LOW",          ← 改为 LOW
  "requires_approval": false,   ← 不再需要审批
  "parameters": [
    {"name": "stand_id", "type": "string", "required": true}
  ]
}

影响：
  1. LLM 收到的 function schema 自动更新
  2. 风险等级提示变为 "LOW"
  3. 不再触发审批流程
  4. 用户体验：操作立即执行，无需等待

这就是 Ontology 的真正价值！
""")


def demo_constraint_validation():
    """演示 Ontology 约束验证"""
    print("\n" + "=" * 60)
    print("Ontology 约束验证演示")
    print("=" * 60)

    print("""
【场景】尝试将航班分配到已满的停机位

数据库中的约束定义:
┌─────────────────────────────────────────────────────────────┐
 │ Constraint: stand_capacity_check                            │
 │ ├─ object_type: Stand                                       │
 │ ├─ action_name: occupy                                      │
 │ ├─ constraint_type: capacity                                │
 │ ├─ expression: "stand.capacity > 0"                        │
 │ └─ error_message: "停机位容量必须大于0"                     │
 ├─────────────────────────────────────────────────────────────┤
 │ Constraint: stand_availability                             │
 │ ├─ object_type: Stand                                       │
 │ ├─ action_name: occupy                                      │
 │ ├─ constraint_type: availability                            │
 │ ├─ expression: "stand.status == 'available'"               │
 │ └─ error_message: "停机位不可用"                            │
 └─────────────────────────────────────────────────────────────┘

用户操作：occupy_stand(flight_id="CA123", stand_id="A5")

Ontology 验证流程:
  1. 检查 stand_id = "A5" 的 capacity
     → capacity = 0 ❌ 违反: stand_capacity_check

  2. 检查 stand_id = "A5" 的 status
     → status = "closed" ❌ 违反: stand_availability

  3. 验证结果:
     ✗ 操作被阻止
     ✗ 返回错误: "停机位 A5 已关闭，无法分配航班"

  4. 如果使用硬编码：
     ✗ 可能在运行时才发现问题
     ✗ 错误信息不友好
     ✗ 需要修改代码才能修复
""")


def demo_permission_based_tool_filtering():
    """演示基于权限的工具过滤"""
    print("\n" + "=" * 60)
    print("Ontology 权限过滤演示")
    print("=" * 60)

    print("""
【场景】不同角色的用户看到不同的工具

数据库中的权限策略:
┌─────────────────────────────────────────────────────────────┐
 │ Policy: admin_full_access                                   │
 │ ├─ principal: role/admin                                    │
 │ ├─ object_type: *                                          │
 │ ├─ permission: admin                                       │
 │ └─ result: 可以执行所有操作                                  │
 ├─────────────────────────────────────────────────────────────┤
 │ Policy: operator_read_only                                  │
 │ ├─ principal: role/operator                                │
 │ ├─ object_type: Flight                                      │
 │ ├─ permission: read                                        │
 │ └─ result: 只能读取，不能修改                                 │
 ├─────────────────────────────────────────────────────────────┤
 │ Policy: guest_limited                                      │
 │ ├─ principal: role/guest                                   │
 │ ├─ object_type: Flight                                     │
 │ ├─ permission: read                                        │
 │ ├─ conditions: {"status": "public"}                         │
 │ └─ result: 只能读取公开状态的航班                            │
 └─────────────────────────────────────────────────────────────┘

工具过滤结果:

Admin 用户 (role=admin):
  tools = [
    "Flight.change_stand",
    "Flight.delay_flight",
    "Flight.update_status",
    "Stand.occupy",
    "Stand.close",     ← 可以关闭停机位
    "Anomaly.resolve",
    ...
  ]

Operator 用户 (role=operator):
  tools = [
    "Flight.change_stand",
    "Flight.delay_flight",
    "Flight.update_status",  ← 可以执行，但需要审批
    "Stand.occupy",
    "Stand.reserve",
    ...
  ]

Guest 用户 (role=guest):
  tools = [
    "Flight.get_info",      ← 只有查询权限
    "Stand.get_status",
    ...
  ]

这就是基于 Ontology 的细粒度权限控制！
""")


def demo_hot_reload():
    """演示热更新"""
    print("\n" + "=" * 60)
    print("Ontology 热更新演示")
    print("=" * 60)

    print("""
【场景】运营中新增一个 'Equipment' 对象类型

步骤 1: 管理员通过前端创建新对象
─────────────────────────────────
POST /api/v1/aip/ontology/objects
{
  "name": "Equipment",
  "plural_name": "Equipments",
  "description": "地面保障设备",
  "properties": [
    {"name": "equipment_id", "type": "string", "required": true},
    {"name": "equipment_type", "type": "enum", "enum_values": ["行李车", "加油车", "牵引车", "客梯车"]},
    {"name": "status", "type": "enum", "enum_values": ["available", "in_use", "maintenance"]},
    {"name": "location", "type": "string"}
  ],
  "actions": ["assign", "release", "maintenance", "update_status"]
}

步骤 2: 创建关联的动作
─────────────────────────────────
POST /api/v1/aip/ontology/actions
{
  "name": "assign",
  "object_type": "Equipment",
  "description": "分配设备到航班",
  "risk_level": "LOW",
  "parameters": [
    {"name": "flight_id", "type": "string", "required": true},
    {"name": "equipment_id", "type": "string", "required": true}
  ]
}

步骤 3: 触发热更新
─────────────────────────────────
POST /api/v1/aip/ontology/reload

步骤 4: AI 立即获得新工具
─────────────────────────────────
LLM 收到的 function schemas 新增:
{
  "type": "function",
  "function": {
    "name": "Equipment.assign",
    "description": "地面保障设备: 分配设备到航班",
    "parameters": {
      "type": "object",
      "properties": {
        "flight_id": {"type": "string", "description": "航班ID"},
        "equipment_id": {"type": "string", "description": "设备ID"}
      },
      "required": ["flight_id", "equipment_id"]
    }
  }
}

AI 立即可以回答：
"我可以帮您安排行李车 CA123-001 给 CA123 航班"
""")


def demo_ontology_workflow():
    """演示完整的工作流"""
    print("\n" + "=" * 60)
    print("Ontology 完整工作流")
    print("=" * 60)

    print("""
                    ┌─────────────────────────────────────────┐
                    │            管理后台 (前端)                 │
                    │  ┌─────────────────────────────────────┐ │
                    │  │ • 创建/编辑 Ontology 对象            │ │
                    │  │ • 定义 Actions 和参数               │ │
                    │  │ • 配置权限策略                      │ │
                    │  │ • 设置业务约束                      │ │
                    │  └─────────────────────────────────────┘ │
                    └──────────────────┬────────────────────────┘
                                       │ 保存到数据库
                                       ▼
                    ┌─────────────────────────────────────────┐
                    │         PostgreSQL 数据库               │
                    │  ┌─────────────────────────────────────┐ │
                    │  │ aip_ontology_objects               │ │
                    │  │ aip_ontology_actions               │ │
                    │  │ aip_object_policies                │ │
                    │  │ aip_constraints                    │ │
                    │  │ aip_functions                      │ │
                    │  └─────────────────────────────────────┘ │
                    └──────────────────┬────────────────────────┘
                                       │ 加载/热更新
                                       ▼
                    ┌─────────────────────────────────────────┐
                    │       OntologyDataLoader                 │
                    │  ┌─────────────────────────────────────┐ │
                    │  │ • 从数据库读取配置                   │ │
                    │  │ • 解析为运行时对象                   │ │
                    │  │ • 提供验证和上下文构建               │ │
                    │  └─────────────────────────────────────┘ │
                    └──────────────────┬────────────────────────┘
                                       │
                    ┌──────────────────┴────────────────────────┐
                    ▼                                         ▼
    ┌─────────────────────────────┐         ┌─────────────────────────────┐
    │    Function Registry        │         │    Context Bridge           │
    │  ┌─────────────────────────┐│         │  ┌─────────────────────────┐│
    │  │ generate_tool_schemas() ││         │  │ build_system_prompt()    ││
    │  │                         ││         │  │                         ││
    │  │ • 转换为 OpenAI 格式     ││         │  │ • 构建 AI 理解的结构化   ││
    │  │ • 按权限过滤            ││         │  │   上下文                 ││
    │  │ • 按风险等级标注        ││         │  │                         ││
    │  └─────────────────────────┘│         │  └─────────────────────────┘│
    └──────────────┬──────────────┘         └──────────────┬──────────────┘
                   │                                        │
                   ▼                                        ▼
    ┌─────────────────────────────┐         ┌─────────────────────────────┐
    │       LLM (GPT-4)           │◄────────│     System Prompt          │
    │  ┌─────────────────────────┐│         │  ┌─────────────────────────┐│
    │  │                         ││         │  │ 你是一个航班调度助手，    ││
    │  │ 收到用户请求:           ││         │  │ 你可以操作以下对象：     ││
    │  │ "把 CA123 改到 A5"      ││         │  │ • Flight                 ││
    │  │                         ││         │  │ • Stand                  ││
    │  │ 基于 function schema，   ││         │  │ • Team                   ││
    │  │ 生成调用:               ││         │  │ ...                     ││
    │  │                         ││         │  └─────────────────────────┘│
    │  │ Flight.change_stand(    ││         └─────────────────────────────┘
    │  │   stand_id="A5"         ││
    │  │ )                       ││
    │  │                         ││
    │  └─────────────────────────┘│
    └──────────────┬──────────────┘
                   │
                   ▼
    ┌─────────────────────────────┐
    │    Action Executor          │
    │  ┌─────────────────────────┐│
    │  │ • 权限检查              ││
    │  │ • 约束验证              ││
    │  │ • 需要审批? → 暂停      ││
    │  │ • 执行或返回错误        ││
    │  └─────────────────────────┘│
    └─────────────────────────────┘
""")


if __name__ == "__main__":
    demo_ontology_drives_ai()
    demo_constraint_validation()
    demo_permission_based_tool_filtering()
    demo_hot_reload()
    demo_ontology_workflow()

    print("\n" + "=" * 60)
    print("Ontology 的核心价值总结")
    print("=" * 60)
    print("""
1. 结构化上下文
   - AI 知道有哪些对象、属性、动作
   - 生成正确的函数调用

2. 配置驱动行为
   - 修改数据库 → AI 行为立即改变
   - 无需修改代码和重新部署

3. 约束前置验证
   - 在执行前检查业务规则
   - 减少无效操作

4. 细粒度权限
   - 不同用户看到不同工具
   - 敏感操作需要审批

5. 可观测性
   - 所有配置都可追溯
   - 便于审计和调试
""")
