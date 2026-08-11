"""
AIP 模块使用示例

展示如何使用 AIP 模式执行航班保障操作。
"""

import asyncio


async def basic_usage_example():
    """
    基础使用示例：初始化 AIP 应用并执行 Action
    """
    from src.infrastructure.ai.aip.app import get_aip_app

    app = get_aip_app()
    await app.initialize()

    result = await app.execute_action(
        principal="user:operator1",
        object_type="Flight",
        object_id="CA1234",
        action="delay_flight",
        parameters={"flight_id": "CA1234", "delay_minutes": 30, "reason": "weather delay"},
    )

    print(f"Execution result: {result}")
    return result


async def change_stand_with_approval_example():
    """
    示例：变更航班机位（需要审批）
    """
    from src.infrastructure.ai.aip.app import get_aip_app

    app = get_aip_app()

    result = await app.execute_action(
        principal="user:operator1",
        object_type="Flight",
        object_id="CA1234",
        action="change_stand",
        parameters={"flight_id": "CA1234", "new_stand": "A5", "reason": "原机位需要维护"},
    )

    if result["status"] == "pending_approval":
        print(f"Action requires approval: {result['pending_action_id']}")
        print(f"Change preview: {result['change_preview']}")
    else:
        print(f"Action executed directly: {result}")

    return result


def get_tools_for_user_example():
    """
    示例：获取用户可用的工具列表
    """
    from src.infrastructure.ai.aip.app import get_aip_app

    app = get_aip_app()

    tools = app.get_tools_for_user(user_id="operator1", user_roles=["operator"], object_types=["Flight", "Stand"])

    print(f"Found {len(tools)} tools for user")
    for tool in tools[:3]:
        print(f"  - {tool['function']['name']}: {tool['function']['description'][:50]}...")

    return tools


def build_system_prompt_example():
    """
    示例：构建增强的系统提示词
    """
    from src.infrastructure.ai.aip.app import get_aip_app

    app = get_aip_app()

    prompt = app.build_system_prompt(object_types=["Flight", "Team"])

    print("Generated System Prompt:")
    print("=" * 60)
    print(prompt[:1000] + "..." if len(prompt) > 1000 else prompt)
    print("=" * 60)

    return prompt


def check_permission_example():
    """
    示例：检查对象级权限
    """
    from src.infrastructure.ai.aip.app import get_aip_app

    app = get_aip_app()

    result = app.check_permission(
        principal="user:operator1", object_type="Flight", object_id="CA1234", permission="execute"
    )

    print(f"Permission check result: {result}")
    return result


async def langgraph_integration_example():
    """
    示例：LangGraph 集成
    """
    from src.infrastructure.ai.aip.app import get_aip_app
    from src.infrastructure.ai.graph.builder import AIAgentBuilder

    app = get_aip_app()
    await app.initialize()

    builder = AIAgentBuilder()
    graph = builder.with_aip(True).set_aip_app(app).build()

    print("AIP LangGraph built successfully")
    print("Nodes: observe, object_context, act, aip_tools, summarize, aip_approval, graceful_abort")

    return graph


async def full_workflow_example():
    """
    完整工作流示例：从用户请求到 Action 执行
    """
    from src.infrastructure.ai.aip.app import initialize_aip_app
    from src.infrastructure.ai.aip.data_access import get_object_accessor

    print("=" * 60)
    print("AIP Full Workflow Example")
    print("=" * 60)

    app = await initialize_aip_app()
    print("\n1. AIP Application initialized")

    flight_id = "CA1234_20240101"
    accessor = get_object_accessor()

    flight_state = await accessor.get_object_state("Flight", flight_id)
    print(f"\n2. Fetched Flight state: {flight_id}")
    print(f"   - flight_number: {flight_state.get('flight_number')}")
    print(f"   - status: {flight_state.get('status')}")
    print(f"   - stand: {flight_state.get('stand')}")

    tools = app.get_tools_for_user(user_id="operator1", user_roles=["operator"], object_types=["Flight"])
    print(f"\n3. Available Flight tools: {len(tools)}")
    for tool in tools[:2]:
        print(f"   - {tool['function']['name']}")

    result = await app.execute_action(
        principal="user:operator1",
        object_type="Flight",
        object_id=flight_id,
        action="delay_flight",
        parameters={"flight_id": flight_id, "delay_minutes": 15, "reason": "air traffic control"},
    )
    print(f"\n4. Action executed: {result['status']}")

    print("\n" + "=" * 60)
    return result


async def main():
    """运行所有示例"""
    print("\n" + "=" * 60)
    print("AIP Module Usage Examples")
    print("=" * 60 + "\n")

    try:
        await basic_usage_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"basic_usage_example failed: {e}")

    try:
        change_stand_with_approval_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"change_stand_with_approval_example failed: {e}")

    try:
        get_tools_for_user_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"get_tools_for_user_example failed: {e}")

    try:
        build_system_prompt_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"build_system_prompt_example failed: {e}")

    try:
        check_permission_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"check_permission_example failed: {e}")

    try:
        await langgraph_integration_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"langgraph_integration_example failed: {e}")

    try:
        await full_workflow_example()
    except Exception as e:  # noqa: BLE001 - example runner must catch all failures to continue
        print(f"full_workflow_example failed: {e}")

    print("\n" + "=" * 60)
    print("Examples completed")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
