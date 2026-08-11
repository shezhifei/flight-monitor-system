"""
Ontology Security - 对象级 ACL

扩展现有的 ToolPermissionManager 为对象级访问控制，
支持基于对象类型、对象ID、属性的细粒度权限控制。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import StrEnum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class Permission(StrEnum):
    """权限类型"""

    READ = "read"
    WRITE = "write"
    DELETE = "delete"
    EXECUTE = "execute"
    ADMIN = "admin"


class PrincipalType(StrEnum):
    """主体类型"""

    USER = "user"
    ROLE = "role"
    GROUP = "group"


@dataclass
class ObjectPolicy:
    """对象级安全策略"""

    object_type: str
    object_id: str | None = None
    principal_type: PrincipalType = PrincipalType.USER
    principal_id: str = ""
    permission: Permission = Permission.READ
    conditions: dict[str, Any] | None = None
    granted: bool = True
    expires_at: datetime | None = None
    created_at: datetime = field(default_factory=datetime.now)
    description: str = ""


@dataclass
class PermissionCheckResult:
    """权限检查结果"""

    allowed: bool
    reason: str = ""
    policy_matched: ObjectPolicy | None = None
    requires_approval: bool = False
    audit_required: bool = False


class ObjectACL:
    """
    对象级访问控制列表

    扩展现有的 ToolPermissionManager，支持：
    - 对象类型级权限
    - 对象实例级权限
    - 基于属性的条件权限
    - 角色继承
    """

    def __init__(self, default_allow: bool = True):
        self._policies: list[ObjectPolicy] = []
        self._default_allow = default_allow
        self._cache: dict[str, bool] = {}
        self._cache_ttl = timedelta(minutes=5)

    def add_policy(self, policy: ObjectPolicy) -> None:
        """添加安全策略"""
        if policy.expires_at and policy.expires_at < datetime.now():
            logger.debug(f"Skipping expired policy: {policy}")
            return

        self._policies.append(policy)
        self._invalidate_cache()

    def add_policies_batch(self, policies: list[ObjectPolicy]) -> None:
        """批量添加策略"""
        now = datetime.now()
        valid_policies = [p for p in policies if not p.expires_at or p.expires_at > now]
        self._policies.extend(valid_policies)
        self._invalidate_cache()

    def grant(
        self,
        principal_type: PrincipalType,
        principal_id: str,
        object_type: str,
        permission: Permission,
        object_id: str | None = None,
        conditions: dict[str, Any] | None = None,
        expires_at: datetime | None = None,
        description: str = "",
    ) -> ObjectPolicy:
        """授予权限的便捷方法"""
        policy = ObjectPolicy(
            object_type=object_type,
            object_id=object_id,
            principal_type=principal_type,
            principal_id=principal_id,
            permission=permission,
            conditions=conditions,
            granted=True,
            expires_at=expires_at,
            description=description,
        )
        self.add_policy(policy)
        return policy

    def revoke(
        self,
        principal_type: PrincipalType,
        principal_id: str,
        object_type: str,
        permission: Permission,
        object_id: str | None = None,
    ) -> bool:
        """撤销权限"""
        removed = False
        self._policies = [
            p
            for p in self._policies
            if not (
                p.principal_type == principal_type
                and p.principal_id == principal_id
                and p.object_type == object_type
                and p.permission == permission
                and p.object_id == object_id
            )
        ]
        if removed:
            self._invalidate_cache()
        return removed

    def check_permission(
        self,
        principal: str,
        object_type: str,
        object_id: str | None,
        permission: Permission,
        context: dict[str, Any] | None = None,
    ) -> PermissionCheckResult:
        """
        检查权限，支持对象级和类型级策略

        Args:
            principal: 主体标识，格式为 "user:xxx" 或 "role:xxx"
            object_type: 对象类型
            object_id: 对象ID（可选，为None时检查类型级权限）
            permission: 权限类型
            context: 额外上下文（用于条件判断）

        Returns:
            PermissionCheckResult
        """
        cache_key = f"{principal}:{object_type}:{object_id}:{permission.value}"
        if cache_key in self._cache:
            cached = self._cache[cache_key]
            return PermissionCheckResult(allowed=cached)

        result = self._check_permission_internal(principal, object_type, object_id, permission, context)

        self._cache[cache_key] = result.allowed

        return result

    def _check_permission_internal(
        self,
        principal: str,
        object_type: str,
        object_id: str | None,
        permission: Permission,
        context: dict[str, Any] | None,
    ) -> PermissionCheckResult:
        """内部权限检查逻辑"""
        principal_type, principal_id = self._parse_principal(principal)

        object_level_result = self._find_matching_policy(
            principal_type, principal_id, object_type, object_id, permission
        )

        if object_level_result and self._evaluate_conditions(object_level_result.conditions, context):
            return PermissionCheckResult(
                allowed=object_level_result.granted,
                reason=f"Object-level policy: {object_level_result.description}",
                policy_matched=object_level_result,
                audit_required=not object_level_result.granted,
            )

        type_level_result = self._find_matching_policy(principal_type, principal_id, object_type, None, permission)

        if type_level_result and self._evaluate_conditions(type_level_result.conditions, context):
            return PermissionCheckResult(
                allowed=type_level_result.granted,
                reason=f"Type-level policy: {type_level_result.description}",
                policy_matched=type_level_result,
                audit_required=True,
            )

        if self._check_role_inheritance(principal_type, principal_id, object_type, permission):
            return PermissionCheckResult(allowed=True, reason="Role inheritance", audit_required=True)

        return PermissionCheckResult(
            allowed=self._default_allow, reason="Default policy" if self._default_allow else "No matching policy"
        )

    def _find_matching_policy(
        self,
        principal_type: PrincipalType,
        principal_id: str,
        object_type: str,
        object_id: str | None,
        permission: Permission,
    ) -> ObjectPolicy | None:
        """查找匹配的安全策略"""
        now = datetime.now()

        for policy in reversed(self._policies):
            if policy.expires_at and policy.expires_at < now:
                continue

            if policy.principal_type != principal_type or policy.principal_id != principal_id:
                continue

            if policy.object_type != object_type:
                continue

            if policy.object_id != object_id:
                continue

            if policy.permission != permission and not self._implies_permission(policy.permission, permission):
                continue

            return policy

        return None

    def _check_role_inheritance(
        self, principal_type: PrincipalType, principal_id: str, object_type: str, permission: Permission
    ) -> bool:
        """检查角色继承的权限"""
        if principal_type != PrincipalType.USER:
            return False

        admin_policy = self._find_matching_policy(PrincipalType.ROLE, "admin", object_type, None, Permission.ADMIN)
        if admin_policy and admin_policy.granted:
            return True

        operator_policy = self._find_matching_policy(PrincipalType.ROLE, "operator", object_type, None, permission)
        return bool(operator_policy and operator_policy.granted)

    @staticmethod
    def _parse_principal(principal: str) -> tuple[PrincipalType, str]:
        """解析主体标识"""
        if principal.startswith("user:"):
            return PrincipalType.USER, principal[5:]
        elif principal.startswith("role:"):
            return PrincipalType.ROLE, principal[5:]
        elif principal.startswith("group:"):
            return PrincipalType.GROUP, principal[6:]
        else:
            return PrincipalType.USER, principal

    @staticmethod
    def _implies_permission(base: Permission, check: Permission) -> bool:
        """检查权限隐含关系"""
        if base == Permission.ADMIN:
            return True
        if base == Permission.WRITE and check in {Permission.READ, Permission.WRITE}:
            return True
        if base == Permission.EXECUTE and check in {Permission.READ, Permission.EXECUTE}:
            return True
        return base == check

    @staticmethod
    def _evaluate_conditions(conditions: dict[str, Any] | None, context: dict[str, Any] | None) -> bool:
        """评估条件表达式"""
        if not conditions:
            return True

        if not context:
            return False

        for key, expected in conditions.items():
            actual = context.get(key)
            if actual != expected:
                return False

        return True

    def _invalidate_cache(self) -> None:
        """清空缓存"""
        self._cache.clear()

    def get_policies_for_principal(self, principal: str) -> list[ObjectPolicy]:
        """获取主体的所有策略"""
        principal_type, principal_id = self._parse_principal(principal)
        now = datetime.now()

        return [
            p
            for p in self._policies
            if p.principal_type == principal_type
            and p.principal_id == principal_id
            and (not p.expires_at or p.expires_at > now)
        ]

    def get_policies_for_object(self, object_type: str, object_id: str | None = None) -> list[ObjectPolicy]:
        """获取对象的所有策略"""
        now = datetime.now()

        return [
            p
            for p in self._policies
            if p.object_type == object_type and p.object_id == object_id and (not p.expires_at or p.expires_at > now)
        ]


_acl_instance: ObjectACL | None = None


def get_object_acl() -> ObjectACL:
    """获取全局对象 ACL 实例"""
    global _acl_instance
    if _acl_instance is None:
        _acl_instance = ObjectACL()
    return _acl_instance


def set_object_acl(acl: ObjectACL) -> None:
    """设置全局对象 ACL 实例"""
    global _acl_instance
    _acl_instance = acl


__all__ = [
    "ObjectACL",
    "ObjectPolicy",
    "Permission",
    "PermissionCheckResult",
    "PrincipalType",
    "get_object_acl",
    "set_object_acl",
]
