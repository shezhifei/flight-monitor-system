from dataclasses import dataclass, field


@dataclass
class ToolPermission:
    """工具权限定义"""

    tool_name: str
    allowed_users: set[str] = field(default_factory=set)
    allowed_roles: set[str] = field(default_factory=set)
    rate_limit: int | None = None  # 每用户每分钟调用次数
    cost_limit: float | None = None  # 每用户成本限制
    time_restrictions: dict | None = None  # 时间限制 (e.g., {"start": "09:00", "end": "18:00"})


class ToolPermissionManager:
    """工具权限管理器"""

    def __init__(self, default_allow: bool = True):
        self._permissions: dict[str, ToolPermission] = {}
        self._default_allow = default_allow

    def set_default_allow(self, default_allow: bool) -> None:
        """设置未配置权限规则时的默认行为。"""
        self._default_allow = bool(default_allow)

    def add_permission(self, permission: ToolPermission):
        """添加或更新工具权限"""
        self._permissions[permission.tool_name] = permission

    def add_user_permission(self, user_id: str, tool_name: str) -> None:
        """兼容旧接口：为用户添加工具白名单权限。"""
        permission = self._permissions.get(tool_name)
        if permission is None:
            permission = ToolPermission(tool_name=tool_name)
        permission.allowed_users.add(user_id)
        self._permissions[tool_name] = permission

    def add_role_permission(self, role_name: str, tool_name: str) -> None:
        """为角色添加工具白名单权限。"""
        permission = self._permissions.get(tool_name)
        if permission is None:
            permission = ToolPermission(tool_name=tool_name)
        permission.allowed_roles.add(role_name)
        self._permissions[tool_name] = permission

    def check_permission(self, tool_name: str, user_id: str, user_roles: list[str]) -> bool:
        """
        检查用户是否有权限使用特定工具

        Args:
            tool_name: 工具名称
            user_id: 用户ID
            user_roles: 用户角色列表

        Returns:
            bool: 是否允许
        """
        permission = self._permissions.get(tool_name)

        # 如果没有特定权限配置，遵循默认策略
        if not permission:
            return self._default_allow

        has_user_whitelist = bool(permission.allowed_users)
        has_role_whitelist = bool(permission.allowed_roles)

        # 检查用户白名单
        if has_user_whitelist and user_id in permission.allowed_users:
            return True

        # 检查角色白名单
        if has_role_whitelist:
            for role in user_roles:
                if role in permission.allowed_roles:
                    return True

        # 只要配置了任一白名单但未命中，都应拒绝
        return not (has_user_whitelist or has_role_whitelist)

    def get_permission(self, tool_name: str) -> ToolPermission | None:
        return self._permissions.get(tool_name)
