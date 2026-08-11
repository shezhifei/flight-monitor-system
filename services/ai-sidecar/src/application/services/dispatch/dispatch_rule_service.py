"""Application service for department qualification and task type requirement rules."""

from __future__ import annotations

from datetime import datetime
from typing import Any

from src.domain.exceptions.base import BusinessRuleException, EntityNotFoundException, SystemException
from src.domain.models.dispatch import (
    DepartmentQualificationCatalog,
    DepartmentQualificationLevel,
    DepartmentRuleStatus,
    DepartmentTaskTypeRequirementVersion,
    DispatchPublicationState,
    FlightGenerationRule,
    GenerationAdjustmentRule,
    LegScope,
    PublishTriggerMode,
    QualificationGrant,
    QualificationGrantStatus,
    TaskTypeCrewSlotRequirement,
    TaskTypeEquipmentRequirement,
    TemporaryTaskTemplate,
    TurnaroundConstraintMode,
    TurnaroundContinuityRule,
    TurnaroundSlotPair,
)
from src.domain.utils.time_utils import utc_now
from src.shared.id_generator import generate_id


class DispatchRuleService:
    """Manages department-defined qualification catalogs, grants, and task type rules."""

    def __init__(
        self,
        *,
        department_repo: Any,
        qualification_repo: Any,
        qualification_grant_repo: Any,
        task_type_requirement_repo: Any,
        generation_rule_repo: Any = None,
        adjustment_rule_repo: Any = None,
        equipment_type_repo: Any = None,
        temporary_task_template_repo: Any = None,
    ) -> None:
        self._department_repo = department_repo
        self._qualification_repo = qualification_repo
        self._qualification_grant_repo = qualification_grant_repo
        self._task_type_requirement_repo = task_type_requirement_repo
        self._step_requirement_repo = task_type_requirement_repo
        self._generation_rule_repo = generation_rule_repo
        self._adjustment_rule_repo = adjustment_rule_repo
        self._equipment_type_repo = equipment_type_repo
        self._temporary_task_template_repo = temporary_task_template_repo

    async def _ensure_department(self, department_id: str) -> None:
        department = await self._department_repo.find_by_id(department_id)
        if department is None:
            raise EntityNotFoundException(entity_type="科室", entity_id=department_id)

    @staticmethod
    def _normalize_crew_requirements(payload: dict[str, Any]) -> list[TaskTypeCrewSlotRequirement]:
        raw_items = payload.get("crew_requirements")
        if raw_items is None:
            raw_items = payload.get("requirements")
        requirements: list[TaskTypeCrewSlotRequirement] = []
        for item in raw_items or []:
            if not isinstance(item, dict):
                continue
            requirements.append(
                TaskTypeCrewSlotRequirement(
                    slot_code=str(item.get("slot_code") or "").strip(),
                    qualification_code=str(item.get("qualification_code") or "").strip(),
                    min_level_code=str(item.get("min_level_code") or "").strip() or None,
                    required_count=int(item.get("required_count") or 1),
                    must_be_distinct=bool(item.get("must_be_distinct", True)),
                    exclusive_group=str(item.get("exclusive_group") or "").strip() or None,
                    remarks=item.get("remarks"),
                )
            )
        return requirements

    @staticmethod
    def _normalize_equipment_requirements(payload: dict[str, Any]) -> list[TaskTypeEquipmentRequirement]:
        requirements: list[TaskTypeEquipmentRequirement] = []
        for item in payload.get("equipment_requirements") or []:
            if not isinstance(item, dict):
                continue
            requirements.append(
                TaskTypeEquipmentRequirement(
                    slot_code=str(item.get("slot_code") or "").strip(),
                    equipment_type_id=str(item.get("equipment_type_id") or "").strip() or None,
                    equipment_type_code=str(item.get("equipment_type_code") or "").strip() or None,
                    required_count=int(item.get("required_count") or 1),
                    must_be_distinct=bool(item.get("must_be_distinct", True)),
                    requires_driver=bool(item.get("requires_driver", False)),
                    driver_qualification_code=str(item.get("driver_qualification_code") or "").strip() or None,
                    driver_min_level_code=str(item.get("driver_min_level_code") or "").strip() or None,
                    remarks=item.get("remarks"),
                )
            )
        return requirements

    @staticmethod
    def _normalize_turnaround_rules(payload: dict[str, Any]) -> list[TurnaroundContinuityRule]:
        rules: list[TurnaroundContinuityRule] = []
        for item in payload.get("turnaround_continuity_rules") or []:
            if not isinstance(item, dict):
                continue
            rules.append(
                TurnaroundContinuityRule(
                    enabled=bool(item.get("enabled", False)),
                    counterpart_leg_scope=LegScope(str(item.get("counterpart_leg_scope") or LegScope.OUTBOUND.value)),
                    counterpart_task_type=str(item.get("counterpart_task_type") or "").strip(),
                    slot_pairs=[
                        TurnaroundSlotPair(
                            inbound_slot_code=str(pair.get("inbound_slot_code") or "").strip(),
                            outbound_slot_code=str(pair.get("outbound_slot_code") or "").strip(),
                        )
                        for pair in (item.get("slot_pairs") or [])
                        if isinstance(pair, dict)
                    ],
                    constraint_mode=TurnaroundConstraintMode(
                        str(item.get("constraint_mode") or TurnaroundConstraintMode.DISABLED.value)
                    ),
                    tight_threshold_minutes=(
                        int(item.get("tight_threshold_minutes"))
                        if item.get("tight_threshold_minutes") is not None
                        else None
                    ),
                    relax_threshold_minutes=(
                        int(item.get("relax_threshold_minutes"))
                        if item.get("relax_threshold_minutes") is not None
                        else None
                    ),
                    flight_filters=dict(item.get("flight_filters") or {}),
                    aircraft_type_filters=list(item.get("aircraft_type_filters") or []),
                    notes=item.get("notes"),
                )
            )
        return rules

    # ── Condition tree evaluation engine ──────────────────────────────────

    @staticmethod
    def _is_condition_tree(conditions: Any) -> bool:
        """Return True when *conditions* uses the new tree format."""
        if not isinstance(conditions, dict):
            return False
        return "operator" in conditions and "children" in conditions

    @classmethod
    def _normalize_conditions(cls, raw: Any) -> dict[str, Any]:
        """Convert legacy flat-dict conditions into a standard condition tree.

        Legacy format (implicit AND of field checks):
            {"is_vip": true, "flight_nature": "domestic"}
        Normalized tree:
            {"operator": "AND", "children": [
                {"field": "is_vip", "op": "eq", "value": true},
                {"field": "flight_nature", "op": "eq", "value": "domestic"}
            ]}

        If *raw* is already a tree it is returned unchanged.
        """
        if raw is None or raw == {}:
            return {"operator": "AND", "children": []}
        if cls._is_condition_tree(raw):
            return dict(raw)
        # Legacy flat dict → convert each key-value pair into a leaf node.
        children: list[dict[str, Any]] = []
        for key, value in (raw if isinstance(raw, dict) else {}).items():
            if value is None or value == "" or value == [] or value == {}:
                continue
            if isinstance(value, list):
                children.append({"field": key, "op": "in", "value": value})
            elif isinstance(value, bool):
                children.append({"field": key, "op": "eq", "value": value})
            else:
                children.append({"field": key, "op": "eq", "value": value})
        return {"operator": "AND", "children": children}

    @classmethod
    def _evaluate_condition_tree(cls, tree: dict[str, Any], context: dict[str, Any]) -> bool:
        """Recursively evaluate a condition tree against a flight context dict.

        A *group node* has ``operator`` (AND | OR) and ``children`` (list).
        A *leaf node* has ``field``, ``op``, and ``value``.
        """
        if not tree:
            return True
        # Leaf node
        if "field" in tree:
            return cls._evaluate_leaf(tree, context)
        operator = str(tree.get("operator") or "AND").upper()
        children = list(tree.get("children") or [])
        if not children:
            return True
        if operator == "OR":
            return any(cls._evaluate_condition_tree(child, context) for child in children)
        # default AND
        return all(cls._evaluate_condition_tree(child, context) for child in children)

    @staticmethod
    def _evaluate_leaf(leaf: dict[str, Any], context: dict[str, Any]) -> bool:
        """Evaluate a single condition leaf against context."""
        field = str(leaf.get("field") or "")
        op = str(leaf.get("op") or "eq").lower()
        expected = leaf.get("value")
        actual = context.get(field)

        # Treat missing / empty actual as "no data → condition not met"
        # unless the op explicitly checks for absence.
        if actual is None or actual == "" or actual == [] or actual == {}:
            if op == "eq" and expected in (None, "", False):
                return True
            return op == "neq"

        # Normalize scalars for comparison
        def _str(v: Any) -> str:
            return str(v).strip().lower()

        def _to_set(v: Any) -> set:
            items = v if isinstance(v, list) else [v]
            return {_str(i) for i in items if str(i).strip()}

        if op == "eq":
            if isinstance(expected, bool):
                return bool(actual) == expected
            return _str(actual) == _str(expected)
        if op == "neq":
            if isinstance(expected, bool):
                return bool(actual) != expected
            return _str(actual) != _str(expected)
        if op == "in":
            expected_set = _to_set(expected)
            actual_set = _to_set(actual)
            return bool(actual_set & expected_set)
        if op == "nin":
            expected_set = _to_set(expected)
            actual_set = _to_set(actual)
            return not bool(actual_set & expected_set)
        if op == "contains":
            return _str(expected) in _str(actual)
        # Numeric comparisons
        try:
            actual_num = float(actual)
            expected_num = float(expected)
        except (TypeError, ValueError):
            return False
        if op == "gt":
            return actual_num > expected_num
        if op == "gte":
            return actual_num >= expected_num
        if op == "lt":
            return actual_num < expected_num
        if op == "lte":
            return actual_num <= expected_num
        return False

    @classmethod
    def _filters_overlap(cls, left: Any, right: dict[str, Any]) -> bool:
        """Check whether rule conditions *left* overlap / match flight data *right*.

        Accepts both legacy flat dicts and new condition trees.
        """
        if left is None or left == {} or right is None or right == {}:
            return True
        tree = cls._normalize_conditions(left)
        return cls._evaluate_condition_tree(tree, right)

    @staticmethod
    def _build_requirement_messages(
        task_type: str,
        requirement_version: DepartmentTaskTypeRequirementVersion | None,
    ) -> list[str]:
        if requirement_version is None:
            return [f"作业类型 {task_type} 缺少已发布作业类型规则"]

        messages: list[str] = []
        crew_requirements = list(
            getattr(requirement_version, "crew_requirements", None)
            or getattr(requirement_version, "requirements", None)
            or []
        )
        equipment_requirements = list(getattr(requirement_version, "equipment_requirements", None) or [])
        if not crew_requirements:
            messages.append(f"作业类型 {task_type} 缺少人员资质要求")
        if not equipment_requirements:
            messages.append(f"作业类型 {task_type} 缺少设备类型要求")
        return messages

    @staticmethod
    def _apply_adjustments_to_preview_order(order_payload: dict[str, Any], actions: list[dict[str, Any]]) -> None:
        crew_requirements = [dict(item) for item in (order_payload.get("crew_requirement_snapshot") or [])]
        equipment_requirements = [dict(item) for item in (order_payload.get("equipment_requirement_snapshot") or [])]
        for action in actions or []:
            if not isinstance(action, dict):
                continue
            action_type = str(action.get("action_type") or "").strip()
            slot_code = str(action.get("slot_code") or "").strip()
            if action_type == "increase_slot_count":
                for item in crew_requirements:
                    if str(item.get("slot_code") or "").strip() == slot_code:
                        item["required_count"] = int(item.get("required_count") or 1) + int(action.get("delta") or 1)
            elif action_type == "add_slot":
                slot_payload = dict(action.get("slot") or {})
                if slot_payload:
                    crew_requirements.append(slot_payload)
            elif action_type == "upgrade_min_level":
                for item in crew_requirements:
                    if str(item.get("slot_code") or "").strip() == slot_code:
                        item["min_level_code"] = action.get("min_level_code")
            elif action_type == "extend_duration":
                order_payload["duration_minutes"] = int(order_payload.get("duration_minutes") or 0) + int(
                    action.get("delta_minutes") or 0
                )
            elif action_type == "advance_publish_offset":
                order_payload["publish_offset_minutes"] = int(order_payload.get("publish_offset_minutes") or 0) - int(
                    action.get("delta_minutes") or 0
                )
            elif action_type == "delay_publish_offset":
                order_payload["publish_offset_minutes"] = int(order_payload.get("publish_offset_minutes") or 0) + int(
                    action.get("delta_minutes") or 0
                )
            elif action_type == "increase_equipment_count":
                for item in equipment_requirements:
                    if str(item.get("slot_code") or "").strip() == slot_code:
                        item["required_count"] = int(item.get("required_count") or 1) + int(action.get("delta") or 1)
            elif action_type == "add_equipment_type_requirement":
                slot_payload = dict(action.get("equipment_slot") or {})
                if slot_payload:
                    equipment_requirements.append(slot_payload)
            elif action_type == "require_driver_for_equipment":
                for item in equipment_requirements:
                    if str(item.get("slot_code") or "").strip() == slot_code:
                        item["requires_driver"] = True
                        if action.get("driver_qualification_code") is not None:
                            item["driver_qualification_code"] = action.get("driver_qualification_code")
                        if action.get("driver_min_level_code") is not None:
                            item["driver_min_level_code"] = action.get("driver_min_level_code")
        order_payload["crew_requirement_snapshot"] = crew_requirements
        order_payload["equipment_requirement_snapshot"] = equipment_requirements

    def _matches_turnaround_rule_preview(
        self,
        sample_flight: dict[str, Any],
        turnaround_rule: TurnaroundContinuityRule,
    ) -> bool:
        if not getattr(turnaround_rule, "enabled", False):
            return False
        if not bool(sample_flight.get("is_turnaround", False)):
            return False
        if not self._filters_overlap(getattr(turnaround_rule, "flight_filters", None) or {}, sample_flight):
            return False
        aircraft_filters = {
            str(item).strip().lower()
            for item in (getattr(turnaround_rule, "aircraft_type_filters", None) or [])
            if str(item).strip()
        }
        aircraft_type = str(sample_flight.get("aircraft_type") or "").strip().lower()
        return not (aircraft_filters and aircraft_type not in aircraft_filters)

    @staticmethod
    def _build_turnaround_preview_entry(
        *,
        task_type: str,
        leg_scope: str,
        turnaround_rule: TurnaroundContinuityRule,
        sample_flight: dict[str, Any],
    ) -> dict[str, Any]:
        delta_t_raw = sample_flight.get("delta_t_minutes")
        minimum_turnaround_raw = (
            sample_flight.get("minimum_turnaround_minutes")
            if sample_flight.get("minimum_turnaround_minutes") is not None
            else sample_flight.get("mt_minutes")
        )
        delta_t_minutes = int(delta_t_raw) if delta_t_raw is not None else None
        minimum_turnaround_minutes = int(minimum_turnaround_raw) if minimum_turnaround_raw is not None else None
        slack_minutes = None
        if delta_t_minutes is not None and minimum_turnaround_minutes is not None:
            slack_minutes = max(0, delta_t_minutes - minimum_turnaround_minutes)
        return {
            "pair_key": sample_flight.get("turnaround_pair_key") or sample_flight.get("flight_id"),
            "task_type": task_type,
            "leg_scope": leg_scope,
            "counterpart_leg_scope": getattr(
                turnaround_rule.counterpart_leg_scope, "value", turnaround_rule.counterpart_leg_scope
            ),
            "counterpart_task_type": turnaround_rule.counterpart_task_type,
            "constraint_mode": getattr(turnaround_rule.constraint_mode, "value", turnaround_rule.constraint_mode),
            "slot_pairs": [
                {
                    "inbound_slot_code": pair.inbound_slot_code,
                    "outbound_slot_code": pair.outbound_slot_code,
                }
                for pair in (turnaround_rule.slot_pairs or [])
            ],
            "delta_t_minutes": delta_t_minutes,
            "minimum_turnaround_minutes": minimum_turnaround_minutes,
            "slack_minutes": slack_minutes,
            "tight_threshold_minutes": turnaround_rule.tight_threshold_minutes,
            "relax_threshold_minutes": turnaround_rule.relax_threshold_minutes,
        }

    @staticmethod
    def _serialize_template_crew_requirements(
        template: TemporaryTaskTemplate,
    ) -> list[dict[str, Any]]:
        return [
            {
                "slot_code": item.slot_code,
                "qualification_code": item.qualification_code,
                "min_level_code": item.min_level_code,
                "required_count": item.required_count,
                "must_be_distinct": item.must_be_distinct,
                "exclusive_group": item.exclusive_group,
                "remarks": item.remarks,
            }
            for item in (template.crew_requirements or [])
        ]

    @staticmethod
    def _serialize_template_equipment_requirements(
        template: TemporaryTaskTemplate,
    ) -> list[dict[str, Any]]:
        return [
            {
                "slot_code": item.slot_code,
                "equipment_type_id": item.equipment_type_id,
                "equipment_type_code": item.equipment_type_code,
                "required_count": item.required_count,
                "must_be_distinct": item.must_be_distinct,
                "requires_driver": item.requires_driver,
                "driver_qualification_code": item.driver_qualification_code,
                "driver_min_level_code": item.driver_min_level_code,
                "remarks": item.remarks,
            }
            for item in (template.equipment_requirements or [])
        ]

    async def create_qualification(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> DepartmentQualificationCatalog:
        await self._ensure_department(department_id)
        return await self._qualification_repo.save_catalog(
            DepartmentQualificationCatalog(
                id=generate_id(),
                department_id=department_id,
                qualification_code=str(payload.get("qualification_code") or "").strip(),
                qualification_name=str(payload.get("qualification_name") or "").strip(),
                description=payload.get("description"),
                is_active=bool(payload.get("is_active", True)),
            )
        )

    async def list_qualifications(
        self,
        department_id: str,
        *,
        include_inactive: bool = False,
    ) -> list[DepartmentQualificationCatalog]:
        await self._ensure_department(department_id)
        return await self._qualification_repo.list_catalogs(
            department_id,
            include_inactive=include_inactive,
        )

    async def create_level(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> DepartmentQualificationLevel:
        await self._ensure_department(department_id)
        return await self._qualification_repo.save_level(
            DepartmentQualificationLevel(
                id=generate_id(),
                department_id=department_id,
                qualification_code=str(payload.get("qualification_code") or "").strip(),
                level_code=str(payload.get("level_code") or "").strip(),
                level_name=str(payload.get("level_name") or "").strip(),
                level_rank=int(payload.get("level_rank") or 0),
                covered_level_codes=list(payload.get("covered_level_codes") or []),
                is_active=bool(payload.get("is_active", True)),
            )
        )

    async def list_levels(
        self,
        department_id: str,
        *,
        qualification_code: str | None = None,
        include_inactive: bool = False,
    ) -> list[DepartmentQualificationLevel]:
        await self._ensure_department(department_id)
        return await self._qualification_repo.list_levels(
            department_id,
            qualification_code=qualification_code,
            include_inactive=include_inactive,
        )

    async def create_grant(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> QualificationGrant:
        await self._ensure_department(department_id)
        return await self._qualification_grant_repo.save(
            QualificationGrant(
                id=generate_id(),
                user_id=str(payload.get("user_id") or "").strip(),
                department_id=department_id,
                qualification_code=str(payload.get("qualification_code") or "").strip(),
                level_code=str(payload.get("level_code") or "").strip(),
                valid_from=payload.get("valid_from"),
                valid_to=payload.get("valid_to"),
                status=QualificationGrantStatus(str(payload.get("status") or QualificationGrantStatus.ACTIVE.value)),
                source_team_id=payload.get("source_team_id"),
                metadata=dict(payload.get("metadata") or {}),
            )
        )

    async def list_grants(
        self,
        department_id: str,
        *,
        user_ids: list[str] | None = None,
        include_inactive: bool = False,
        at_time: datetime | None = None,
    ) -> list[QualificationGrant]:
        await self._ensure_department(department_id)
        return await self._qualification_grant_repo.find_by_department(
            department_id,
            user_ids=user_ids,
            include_inactive=include_inactive,
            at_time=at_time,
        )

    async def save_requirement_draft(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> DepartmentTaskTypeRequirementVersion:
        await self._ensure_department(department_id)
        task_type = str(payload.get("task_type") or "").strip()
        crew_requirements = self._normalize_crew_requirements(payload)
        equipment_requirements = self._normalize_equipment_requirements(payload)
        turnaround_rules = self._normalize_turnaround_rules(payload)
        existing = await self._step_requirement_repo.find_latest_draft(department_id, task_type)
        if existing is not None:
            existing.crew_requirements = crew_requirements
            existing.equipment_requirements = equipment_requirements
            existing.turnaround_continuity_rules = turnaround_rules
            existing.notes = payload.get("notes")
            existing.status = DepartmentRuleStatus.DRAFT
            return await self._step_requirement_repo.save(existing)

        return await self._step_requirement_repo.save(
            DepartmentTaskTypeRequirementVersion(
                id=generate_id(),
                department_id=department_id,
                task_type=task_type,
                version_no=await self._step_requirement_repo.next_version_no(department_id, task_type),
                status=DepartmentRuleStatus.DRAFT,
                crew_requirements=crew_requirements,
                equipment_requirements=equipment_requirements,
                turnaround_continuity_rules=turnaround_rules,
                notes=payload.get("notes"),
            )
        )

    async def list_requirement_versions(
        self,
        department_id: str,
        *,
        task_type: str | None = None,
        status: str | None = None,
    ) -> list[DepartmentTaskTypeRequirementVersion]:
        await self._ensure_department(department_id)
        return await self._step_requirement_repo.list_versions(
            department_id,
            task_type=task_type,
            status=status,
        )

    async def get_published_requirement(
        self,
        department_id: str,
        task_type: str,
    ) -> DepartmentTaskTypeRequirementVersion | None:
        return await self._step_requirement_repo.find_published(department_id, task_type)

    async def publish_requirement(
        self,
        department_id: str,
        *,
        task_type: str,
        draft_id: str | None = None,
    ) -> DepartmentTaskTypeRequirementVersion:
        await self._ensure_department(department_id)
        draft = None
        if draft_id:
            draft = await self._step_requirement_repo.find_by_id(draft_id)
            if draft is None:
                raise EntityNotFoundException(entity_type="作业类型规则草稿", entity_id=draft_id)
            if draft.department_id != department_id or draft.task_type != task_type:
                raise EntityNotFoundException(entity_type="作业类型规则草稿", entity_id=draft_id)
        else:
            draft = await self._step_requirement_repo.find_latest_draft(department_id, task_type)
        if draft is None:
            raise EntityNotFoundException(entity_type="作业类型规则草稿", entity_id=f"{department_id}:{task_type}")

        await self._step_requirement_repo.archive_published(department_id, task_type)
        draft.status = DepartmentRuleStatus.PUBLISHED
        draft.published_at = utc_now()
        return await self._step_requirement_repo.save(draft)

    async def list_generation_rules(
        self,
        department_id: str | None = None,
        *,
        status: str | None = None,
    ) -> list[FlightGenerationRule]:
        if self._generation_rule_repo is None:
            return []
        if department_id:
            await self._ensure_department(department_id)
        return await self._generation_rule_repo.list_rules(department_id, status=status)

    async def save_generation_rule(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> FlightGenerationRule:
        await self._ensure_department(department_id)
        if self._generation_rule_repo is None:
            raise SystemException(message="基础生成规则仓储未配置")

        normalized_status = DepartmentRuleStatus(str(payload.get("status") or DepartmentRuleStatus.DRAFT.value))
        candidate = FlightGenerationRule(
            id=str(payload.get("rule_id") or payload.get("id") or generate_id()).strip(),
            department_id=department_id,
            task_type=str(payload.get("task_type") or "").strip(),
            leg_scope=LegScope(str(payload.get("leg_scope") or LegScope.NONE.value)),
            status=normalized_status,
            rule_name=str(payload.get("rule_name") or "").strip() or None,
            conditions=dict(payload.get("conditions") or {}),
            generation_anchor_type=str(payload.get("generation_anchor_type") or "scheduled_time").strip(),
            start_offset_minutes=int(payload.get("start_offset_minutes") or 0),
            duration_minutes=int(payload.get("duration_minutes"))
            if payload.get("duration_minutes") is not None
            else None,
            publication_state=DispatchPublicationState(
                str(payload.get("publication_state") or DispatchPublicationState.PREPUBLISHED.value)
            ),
            publish_trigger_mode=PublishTriggerMode(
                str(payload.get("publish_trigger_mode") or PublishTriggerMode.TIME.value)
            ),
            publish_offset_minutes=(
                int(payload.get("publish_offset_minutes"))
                if payload.get("publish_offset_minutes") is not None
                else None
            ),
            publish_event_code=str(payload.get("publish_event_code") or "").strip() or None,
            notes=payload.get("notes"),
        )
        conflicts = await self.validate_generation_rule(department_id, payload, current_rule_id=candidate.id)
        if normalized_status == DepartmentRuleStatus.PUBLISHED and not conflicts["valid"]:
            raise BusinessRuleException(message="；".join(conflicts["messages"]) or "基础生成规则校验失败")
        if normalized_status == DepartmentRuleStatus.PUBLISHED:
            candidate.published_at = utc_now()
        return await self._generation_rule_repo.save(candidate)

    async def delete_generation_rule(
        self,
        department_id: str,
        rule_id: str,
    ) -> dict[str, str]:
        await self._ensure_department(department_id)
        if self._generation_rule_repo is None:
            raise SystemException(message="基础生成规则仓储未配置")

        existing = await self._generation_rule_repo.find_by_id(rule_id)
        if existing is None or existing.department_id != department_id:
            raise EntityNotFoundException(entity_type="基础生成规则", entity_id=rule_id)

        existing.status = DepartmentRuleStatus.ARCHIVED
        existing.published_at = None
        await self._generation_rule_repo.save(existing)
        return {"message": "触发规则已删除"}

    async def validate_generation_rule(
        self,
        department_id: str,
        payload: dict[str, Any],
        *,
        current_rule_id: str | None = None,
    ) -> dict[str, Any]:
        await self._ensure_department(department_id)
        if self._generation_rule_repo is None:
            return {"valid": True, "conflicts": [], "messages": []}

        task_type = str(payload.get("task_type") or "").strip()
        leg_scope = str(payload.get("leg_scope") or "").strip()
        candidate_conditions = dict(payload.get("conditions") or {})
        conflicts: list[dict[str, Any]] = []
        existing_rules = await self._generation_rule_repo.list_rules(department_id)
        for rule in existing_rules:
            if current_rule_id and str(rule.id) == str(current_rule_id):
                continue
            if rule.task_type != task_type or rule.leg_scope.value != leg_scope:
                continue
            if self._filters_overlap(rule.conditions, candidate_conditions):
                conflicts.append(
                    {
                        "rule_id": rule.id,
                        "task_type": rule.task_type,
                        "leg_scope": rule.leg_scope.value,
                        "rule_name": rule.rule_name,
                        "status": rule.status.value,
                    }
                )
        messages: list[str] = ["存在可重叠的基础生成规则"] if conflicts else []
        normalized_status = DepartmentRuleStatus(str(payload.get("status") or DepartmentRuleStatus.DRAFT.value))
        if normalized_status == DepartmentRuleStatus.PUBLISHED and task_type:
            requirement_version = await self.get_published_requirement(department_id, task_type)
            messages.extend(self._build_requirement_messages(task_type, requirement_version))
        return {
            "valid": not conflicts and not messages,
            "conflicts": conflicts,
            "messages": list(dict.fromkeys(messages)),
        }

    async def list_adjustment_rules(
        self,
        department_id: str | None = None,
        *,
        status: str | None = None,
    ) -> list[GenerationAdjustmentRule]:
        if self._adjustment_rule_repo is None:
            return []
        if department_id:
            await self._ensure_department(department_id)
        return await self._adjustment_rule_repo.list_rules(department_id, status=status)

    async def save_temporary_task_template(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> TemporaryTaskTemplate:
        await self._ensure_department(department_id)
        if self._temporary_task_template_repo is None:
            raise SystemException(message="临时任务模板仓储未配置")
        template = TemporaryTaskTemplate(
            id=str(payload.get("id") or payload.get("template_id") or generate_id()).strip(),
            department_id=department_id,
            template_code=str(payload.get("template_code") or "").strip(),
            template_name=str(payload.get("template_name") or "").strip(),
            task_type=str(payload.get("task_type") or "").strip(),
            crew_requirements=self._normalize_crew_requirements(payload),
            equipment_requirements=self._normalize_equipment_requirements(payload),
            notes=payload.get("notes"),
            is_active=bool(payload.get("is_active", True)),
        )
        return await self._temporary_task_template_repo.save(template)

    async def list_temporary_task_templates(
        self,
        department_id: str,
        *,
        include_inactive: bool = False,
    ) -> list[TemporaryTaskTemplate]:
        await self._ensure_department(department_id)
        if self._temporary_task_template_repo is None:
            return []
        return await self._temporary_task_template_repo.list_templates(
            department_id,
            include_inactive=include_inactive,
        )

    async def get_temporary_task_template(
        self,
        department_id: str,
        template_code: str,
    ) -> TemporaryTaskTemplate | None:
        await self._ensure_department(department_id)
        if self._temporary_task_template_repo is None:
            return None
        return await self._temporary_task_template_repo.find_by_code(department_id, template_code)

    async def save_adjustment_rule(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> GenerationAdjustmentRule:
        await self._ensure_department(department_id)
        if self._adjustment_rule_repo is None:
            raise SystemException(message="增量调整规则仓储未配置")
        normalized_status = DepartmentRuleStatus(str(payload.get("status") or DepartmentRuleStatus.DRAFT.value))
        rule = GenerationAdjustmentRule(
            id=str(payload.get("rule_id") or payload.get("id") or generate_id()).strip(),
            department_id=department_id,
            task_type=str(payload.get("task_type") or "").strip(),
            status=normalized_status,
            rule_name=str(payload.get("rule_name") or "").strip() or None,
            conditions=dict(payload.get("conditions") or {}),
            actions=list(payload.get("actions") or []),
            notes=payload.get("notes"),
            published_at=utc_now() if normalized_status == DepartmentRuleStatus.PUBLISHED else None,
        )
        return await self._adjustment_rule_repo.save(rule)

    async def preview_dispatch_rules(
        self,
        department_id: str,
        payload: dict[str, Any],
    ) -> dict[str, Any]:
        await self._ensure_department(department_id)
        sample_flight = dict(payload.get("sample_flight") or {})
        generated_orders: list[dict[str, Any]] = []
        applied_adjustments: list[dict[str, Any]] = []
        turnaround_constraints: list[dict[str, Any]] = []
        blocking_errors: list[str] = []
        conflicts: list[dict[str, Any]] = []
        generation_rules = await self.list_generation_rules(
            department_id,
            status=DepartmentRuleStatus.PUBLISHED.value,
        )
        adjustment_rules = await self.list_adjustment_rules(
            department_id,
            status=DepartmentRuleStatus.PUBLISHED.value,
        )
        matched_rules = [
            rule
            for rule in generation_rules
            if str(sample_flight.get("leg_scope") or "") == rule.leg_scope.value
            and self._filters_overlap(rule.conditions, sample_flight)
        ]
        matched_rules_by_key: dict[tuple[str, str], list[FlightGenerationRule]] = {}
        for rule in matched_rules:
            matched_rules_by_key.setdefault((rule.leg_scope.value, rule.task_type), []).append(rule)
        blocked_keys = {key for key, rules in matched_rules_by_key.items() if len(rules) > 1}
        for (leg_scope, task_type), rules in matched_rules_by_key.items():
            if len(rules) <= 1:
                continue
            conflict_entry = {
                "task_type": task_type,
                "leg_scope": leg_scope,
                "matched_rule_ids": [item.id for item in rules],
                "matched_rule_names": [item.rule_name for item in rules if item.rule_name],
            }
            conflicts.append(conflict_entry)
            blocking_errors.append(
                f"基础生成规则冲突：leg_scope={leg_scope} task_type={task_type} 同时命中 {len(rules)} 条已发布规则"
            )

        for rule in matched_rules:
            if (rule.leg_scope.value, rule.task_type) in blocked_keys:
                continue
            requirement_version = await self.get_published_requirement(department_id, rule.task_type)
            requirement_messages = self._build_requirement_messages(rule.task_type, requirement_version)
            if requirement_messages:
                blocking_errors.extend(requirement_messages)
                continue
            order_payload = {
                "task_type": rule.task_type,
                "leg_scope": rule.leg_scope.value,
                "generation_rule_id": rule.id,
                "generation_rule_version": rule.version_no,
                "generation_anchor_type": rule.generation_anchor_type,
                "duration_minutes": rule.duration_minutes,
                "publication_state": getattr(rule.publication_state, "value", rule.publication_state),
                "publish_trigger_mode": getattr(rule.publish_trigger_mode, "value", rule.publish_trigger_mode),
                "publish_offset_minutes": rule.publish_offset_minutes,
                "crew_requirement_snapshot": [
                    {
                        "slot_code": item.slot_code,
                        "qualification_code": item.qualification_code,
                        "min_level_code": item.min_level_code,
                        "required_count": item.required_count,
                    }
                    for item in (requirement_version.crew_requirements or [])
                ],
                "equipment_requirement_snapshot": [
                    {
                        "slot_code": item.slot_code,
                        "equipment_type_id": item.equipment_type_id,
                        "equipment_type_code": item.equipment_type_code,
                        "required_count": item.required_count,
                        "requires_driver": item.requires_driver,
                    }
                    for item in (requirement_version.equipment_requirements or [])
                ],
            }
            matched_adjustments: list[str] = []
            for adjustment_rule in adjustment_rules:
                if adjustment_rule.task_type != rule.task_type:
                    continue
                if self._filters_overlap(adjustment_rule.conditions, sample_flight):
                    self._apply_adjustments_to_preview_order(order_payload, adjustment_rule.actions)
                    applied_adjustments.append(
                        {
                            "task_type": rule.task_type,
                            "rule_id": adjustment_rule.id,
                            "actions": adjustment_rule.actions,
                        }
                    )
                    matched_adjustments.append(adjustment_rule.id)
            order_payload["matched_adjustment_rule_ids"] = matched_adjustments
            generated_orders.append(order_payload)
            for turnaround_rule in requirement_version.turnaround_continuity_rules or []:
                if self._matches_turnaround_rule_preview(sample_flight, turnaround_rule):
                    turnaround_constraints.append(
                        self._build_turnaround_preview_entry(
                            task_type=rule.task_type,
                            leg_scope=rule.leg_scope.value,
                            turnaround_rule=turnaround_rule,
                            sample_flight=sample_flight,
                        )
                    )
        return {
            "generated_orders": generated_orders,
            "applied_adjustments": applied_adjustments,
            "turnaround_constraints": turnaround_constraints,
            "conflicts": conflicts,
            "blocking_errors": list(dict.fromkeys(blocking_errors)),
        }
