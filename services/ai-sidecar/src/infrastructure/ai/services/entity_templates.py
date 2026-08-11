from dataclasses import dataclass
from typing import Any, ClassVar

from ..ai_entity import AIEntityConfig


@dataclass
class EntityTemplate:
    """AI实体模板"""

    name: str
    config: AIEntityConfig
    description: str = ""

    def create_config(self, overrides: dict[str, Any] | None = None) -> AIEntityConfig:
        """基于模板创建实体配置"""
        # 创建配置副本
        base_config = self.config

        if overrides:
            # 简单的属性覆盖
            # 注意：这里应该是一个深拷贝或者更复杂的合并逻辑，简化起见直接创建新对象
            new_config_dict = base_config.__dict__.copy()
            new_config_dict.update(overrides)
            return AIEntityConfig(**new_config_dict)

        return base_config


# 预定义模板工厂
class EntityTemplateFactory:
    _templates: ClassVar[dict[str, EntityTemplate]] = {}

    @classmethod
    def register_template(cls, template: EntityTemplate):
        cls._templates[template.name] = template

    @classmethod
    def get_template(cls, name: str) -> EntityTemplate | None:
        return cls._templates.get(name)

    @classmethod
    def list_templates(cls) -> dict[str, str]:
        return {name: t.description for name, t in cls._templates.items()}


# 初始化预定义模板
def _init_default_templates():
    # Todo Agent Template
    EntityTemplateFactory.register_template(
        EntityTemplate(
            name="todo_agent",
            config=AIEntityConfig(
                default_model="gpt-4o",
                temperature=0.7,
                allowed_tool_categories=["todo", "system"],
                system_prompt="你是一个高效的待办事项管理助手，帮助用户组织和跟踪任务。",
            ),
            description="专门用于管理待办事项的AI助手",
        )
    )

    # Flight Analyzer Template
    EntityTemplateFactory.register_template(
        EntityTemplate(
            name="flight_analyzer",
            config=AIEntityConfig(
                default_model="gpt-4o",
                temperature=0.3,  # 分析任务需要较低的温度
                allowed_tool_categories=["flight", "flight_event"],
                system_prompt="你是一个专业的航班数据分析师，可以查询和分析航班状态、延误原因等信息的专家。",
            ),
            description="专门用于分析航班信息的AI助手",
        )
    )


# 模块加载时初始化
_init_default_templates()
