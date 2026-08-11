"""Qualification coverage and crew construction for personalized dispatch."""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Any

from src.domain.models.dispatch import DepartmentQualificationLevel, DepartmentTaskTypeRequirementVersion


@dataclass
class CoverageAssignment:
    user_id: str
    username: str | None
    source_team_id: str | None
    source_team_name: str | None
    slot_code: str
    qualification_code: str
    qualification_level_code: str | None


@dataclass
class QualificationCoverageResult:
    covered: bool
    assignments: list[CoverageAssignment] = field(default_factory=list)
    qualification_gap: list[dict[str, Any]] = field(default_factory=list)
    score_breakdown: dict[str, float] = field(default_factory=dict)


class QualificationCoverageService:
    """Greedy qualification-slot coverage with hierarchical level substitution."""

    @staticmethod
    def build_level_index(
        levels: Iterable[DepartmentQualificationLevel],
    ) -> dict[str, dict[str, DepartmentQualificationLevel]]:
        levels_by_qualification: dict[str, dict[str, DepartmentQualificationLevel]] = defaultdict(dict)
        for level in levels:
            levels_by_qualification[level.qualification_code][level.level_code] = level
        return levels_by_qualification

    @staticmethod
    def build_grants_by_user(grants: Iterable[Any]) -> dict[str, list[Any]]:
        grants_by_user: dict[str, list[Any]] = defaultdict(list)
        for grant in grants:
            user_id = str(getattr(grant, "user_id", "") or "").strip()
            if user_id:
                grants_by_user[user_id].append(grant)
        return grants_by_user

    def build_crew(
        self,
        *,
        requirement_version: DepartmentTaskTypeRequirementVersion,
        candidate_users: dict[str, dict[str, Any]],
        grants: Iterable[Any],
        levels: Iterable[DepartmentQualificationLevel],
    ) -> QualificationCoverageResult:
        levels_by_qualification = self.build_level_index(levels)
        grants_by_user = self.build_grants_by_user(grants)

        used_users: set[str] = set()
        assignments: list[CoverageAssignment] = []
        gaps: list[dict[str, Any]] = []

        for requirement in requirement_version.requirements:
            for index in range(max(1, int(requirement.required_count or 1))):
                slot_instance_code = (
                    requirement.slot_code if requirement.required_count <= 1 else f"{requirement.slot_code}#{index + 1}"
                )
                matched = self._pick_candidate(
                    qualification_code=requirement.qualification_code,
                    min_level_code=requirement.min_level_code,
                    candidate_users=candidate_users,
                    grants_by_user=grants_by_user,
                    levels_by_qualification=levels_by_qualification,
                    used_users=used_users if requirement.must_be_distinct else set(),
                )
                if matched is None:
                    gaps.append(
                        {
                            "slot_code": slot_instance_code,
                            "qualification_code": requirement.qualification_code,
                            "min_level_code": requirement.min_level_code,
                            "reason": "no_available_employee",
                        }
                    )
                    continue

                user_id, matched_grant = matched
                if requirement.must_be_distinct:
                    used_users.add(user_id)
                profile = candidate_users.get(user_id) or {}
                assignments.append(
                    CoverageAssignment(
                        user_id=user_id,
                        username=profile.get("username"),
                        source_team_id=profile.get("source_team_id"),
                        source_team_name=profile.get("source_team_name"),
                        slot_code=slot_instance_code,
                        qualification_code=requirement.qualification_code,
                        qualification_level_code=getattr(matched_grant, "level_code", None),
                    )
                )

        covered = not gaps and bool(assignments)
        score_breakdown = {
            "covered_slot_count": float(len(assignments)),
            "gap_slot_count": float(len(gaps)),
            "coverage_ratio": round(len(assignments) / max(1, len(assignments) + len(gaps)), 4),
        }
        return QualificationCoverageResult(
            covered=covered,
            assignments=assignments,
            qualification_gap=gaps,
            score_breakdown=score_breakdown,
        )

    def _pick_candidate(
        self,
        *,
        qualification_code: str,
        min_level_code: str | None,
        candidate_users: dict[str, dict[str, Any]],
        grants_by_user: dict[str, list[Any]],
        levels_by_qualification: dict[str, dict[str, DepartmentQualificationLevel]],
        used_users: set[str],
    ) -> tuple[str, Any] | None:
        candidates: list[tuple[int, str, Any]] = []
        for user_id, profile in candidate_users.items():
            if used_users and user_id in used_users:
                continue
            for grant in grants_by_user.get(user_id, []):
                if str(getattr(grant, "qualification_code", "") or "") != qualification_code:
                    continue
                if not self._grant_covers_level(
                    qualification_code=qualification_code,
                    grant_level_code=getattr(grant, "level_code", None),
                    min_level_code=min_level_code,
                    levels_by_qualification=levels_by_qualification,
                ):
                    continue
                candidates.append(
                    (
                        self._candidate_priority(
                            qualification_code=qualification_code,
                            grant_level_code=getattr(grant, "level_code", None),
                            min_level_code=min_level_code,
                            levels_by_qualification=levels_by_qualification,
                            profile=profile,
                        ),
                        user_id,
                        grant,
                    )
                )
                break
        if not candidates:
            return None
        candidates.sort(key=lambda item: (item[0], item[1]))
        _, user_id, grant = candidates[0]
        return user_id, grant

    def best_matching_grant_for_user(
        self,
        *,
        user_id: str,
        qualification_code: str,
        min_level_code: str | None,
        grants_by_user: dict[str, list[Any]],
        levels_by_qualification: dict[str, dict[str, DepartmentQualificationLevel]],
        profile: dict[str, Any] | None = None,
    ) -> Any | None:
        candidates: list[tuple[int, Any]] = []
        effective_profile = profile or {}
        for grant in grants_by_user.get(str(user_id or "").strip(), []):
            if str(getattr(grant, "qualification_code", "") or "") != qualification_code:
                continue
            if not self.grant_covers_level(
                qualification_code=qualification_code,
                grant_level_code=getattr(grant, "level_code", None),
                min_level_code=min_level_code,
                levels_by_qualification=levels_by_qualification,
            ):
                continue
            candidates.append(
                (
                    self._candidate_priority(
                        qualification_code=qualification_code,
                        grant_level_code=getattr(grant, "level_code", None),
                        min_level_code=min_level_code,
                        levels_by_qualification=levels_by_qualification,
                        profile=effective_profile,
                    ),
                    grant,
                )
            )
        if not candidates:
            return None
        candidates.sort(key=lambda item: item[0])
        return candidates[0][1]

    def _candidate_priority(
        self,
        *,
        qualification_code: str,
        grant_level_code: str | None,
        min_level_code: str | None,
        levels_by_qualification: dict[str, dict[str, DepartmentQualificationLevel]],
        profile: dict[str, Any],
    ) -> int:
        exact_penalty = 0 if (grant_level_code or "") == (min_level_code or "") else 10
        levels = levels_by_qualification.get(qualification_code, {})
        grant_rank = int(getattr(levels.get(grant_level_code or ""), "level_rank", 0) or 0)
        required_rank = int(getattr(levels.get(min_level_code or ""), "level_rank", 0) or 0)
        overqualified_penalty = max(0, grant_rank - required_rank)
        fallback_penalty = 20 if profile.get("schedule_source") == "current_status_fallback" else 0
        return exact_penalty + overqualified_penalty + fallback_penalty

    def grant_covers_level(
        self,
        *,
        qualification_code: str,
        grant_level_code: str | None,
        min_level_code: str | None,
        levels_by_qualification: dict[str, dict[str, DepartmentQualificationLevel]],
    ) -> bool:
        if not min_level_code:
            return True
        if not grant_level_code:
            return False
        if grant_level_code == min_level_code:
            return True
        levels = levels_by_qualification.get(qualification_code, {})
        grant_level = levels.get(grant_level_code)
        required_level = levels.get(min_level_code)
        if grant_level is None:
            return False
        covered_codes = set(str(item) for item in (grant_level.covered_level_codes or []) if str(item).strip())
        if min_level_code in covered_codes:
            return True
        if required_level is None:
            return False
        return int(grant_level.level_rank or 0) >= int(required_level.level_rank or 0)

    def _grant_covers_level(
        self,
        *,
        qualification_code: str,
        grant_level_code: str | None,
        min_level_code: str | None,
        levels_by_qualification: dict[str, dict[str, DepartmentQualificationLevel]],
    ) -> bool:
        return self.grant_covers_level(
            qualification_code=qualification_code,
            grant_level_code=grant_level_code,
            min_level_code=min_level_code,
            levels_by_qualification=levels_by_qualification,
        )
