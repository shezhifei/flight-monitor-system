"""班组工具执行器。"""

import logging
from typing import Any

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .team_tools import TeamToolName

logger = logging.getLogger(__name__)


class TeamToolExecutor(BaseToolExecutor):
    """将班组工具调用路由到 team repository。"""

    def __init__(
        self,
        team_repository: Any = None,
        member_repository: Any = None,
        default_user: str = "AI_Assistant",
    ):
        super().__init__(default_user)
        self._team_repo = team_repository
        self._member_repo = member_repository

    def _register_handlers(self) -> None:
        self._handlers = {
            TeamToolName.LIST_TEAMS.value: self._handle_list_teams,
            TeamToolName.GET_TEAM_DETAILS.value: self._handle_get_team_details,
            TeamToolName.GET_AVAILABLE_TEAMS.value: self._handle_get_available_teams,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.TEAM

    async def _handle_list_teams(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        status = args.get("status")
        team_type_id = args.get("team_type_id")
        terminal = args.get("terminal")

        teams = await self._team_repo.find_all(
            include_inactive=False,
            team_type_id=team_type_id,
            terminal=terminal,
        )
        # 按状态过滤
        if status:
            teams = [t for t in teams if getattr(t, "current_status", None) and t.current_status.value == status]

        return {
            "total": len(teams),
            "items": [self._team_to_dict(t) for t in teams],
        }

    async def _handle_get_team_details(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        team_id = args.get("team_id")
        team_name = args.get("team_name")

        team = None
        if team_id:
            team = await self._team_repo.find_by_id(team_id)
        elif team_name:
            all_teams = await self._team_repo.find_all(include_inactive=False)
            for t in all_teams:
                if team_name in (t.name or ""):
                    team = t
                    break

        if not team:
            raise ToolExecutionError(
                f"未找到班组: {team_id or team_name}",
                ToolExecutionStatus.NOT_FOUND,
            )

        result = self._team_to_dict(team)

        # 加载成员列表
        if self._member_repo:
            try:
                members = await self._member_repo.find_by_team(team.id, include_inactive=False)
                result["members"] = [
                    {
                        "id": m.id,
                        "user_id": m.user_id,
                        "username": m.username,
                        "display_name": m.user_display_name,
                        "role": m.role.value if hasattr(m.role, "value") else str(m.role),
                        "can_drive": m.can_drive,
                    }
                    for m in members
                ]
                result["member_count"] = len(members)
            except Exception as exc:  # noqa: BLE001 - service call fallback must catch all errors
                logger.warning("team member list enrichment failed: %s", exc)
                result["members"] = []
                result["member_count"] = 0

        return result

    async def _handle_get_available_teams(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        team_type_id = args.get("team_type_id")
        terminal = args.get("terminal")

        teams = await self._team_repo.find_available_for_dispatch(
            team_type_id=team_type_id,
            terminal=terminal,
        )

        return {
            "total": len(teams),
            "items": [self._team_to_dict(t) for t in teams],
        }

    def _ensure_service(self) -> None:
        if not self._team_repo:
            raise ToolExecutionError(
                "班组服务未初始化",
                ToolExecutionStatus.INTERNAL_ERROR,
            )

    @staticmethod
    def _team_to_dict(team: Any) -> dict[str, Any]:
        return {
            "id": team.id,
            "name": team.name,
            "code": getattr(team, "code", None),
            "status": team.current_status.value if hasattr(team.current_status, "value") else str(team.current_status),
            "terminal": getattr(team, "terminal", None),
            "team_type": getattr(team.team_type, "name", None) if getattr(team, "team_type", None) else None,
            "leader_id": getattr(team, "leader_id", None),
            "current_stand_id": getattr(team, "current_stand_id", None),
        }
