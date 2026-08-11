from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class UserInfo:
    id: str
    username: str
    display_name: str = ""
    email: str = ""
    is_active: bool = True
    is_verified: bool = False
    is_admin: bool = False
    department: str | None = None
    department_id: str | None = None
    roles: list[str] = field(default_factory=list)
    permissions: list[str] = field(default_factory=list)


@dataclass
class RoleInfo:
    id: str
    name: str
    code: str
    description: str = ""
    permissions: list[str] = field(default_factory=list)


@dataclass
class PermissionInfo:
    id: str
    code: str
    name: str = ""
    description: str = ""
    category: str = ""
