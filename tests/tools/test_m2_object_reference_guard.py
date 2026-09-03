"""Static guards for schema-driven object-reference validation."""

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ATTRIBUTES = ROOT / "services/api-server/crates/application/src/services/attribute_validation.rs"
ORDER_LIFECYCLE = ROOT / "services/api-server/crates/application/src/services/dispatch_service/order_lifecycle.rs"
RULE_SERVICE = ROOT / "services/api-server/crates/application/src/services/dispatch_rule_service.rs"
DI = ROOT / "services/api-server/crates/server/src/di/dispatch.rs"
RULE_CENTER = ROOT / "frontend/vue-app/src/pages/dispatch_rule_center/DispatchRuleCenter.vue"
PERSONNEL_WRITER = ROOT / "services/api-server/crates/application/src/services/personnel_runtime_writer.rs"
RESOURCE_SERVICE = ROOT / "services/api-server/crates/application/src/services/dispatch_resource_service/service.rs"
DEPARTMENT_WRITER = ROOT / "services/api-server/crates/application/src/services/department_writer.rs"
TEAM_WRITER = ROOT / "services/api-server/crates/application/src/services/team_writer.rs"
EQUIPMENT_TYPE_WRITER = ROOT / "services/api-server/crates/application/src/services/equipment_type_writer.rs"
EQUIPMENT_WRITER = ROOT / "services/api-server/crates/application/src/services/equipment_writer.rs"
TASK_TYPE_WRITER = ROOT / "services/api-server/crates/application/src/services/task_type_writer.rs"
TERMINAL_RESOURCE_WRITER = ROOT / "services/api-server/crates/application/src/services/terminal_resource_writer.rs"
TERMINAL_RESOURCE_SERVICE = ROOT / "services/api-server/crates/application/src/services/terminal_resource_service/service.rs"
TEAM_TYPE_WRITER = ROOT / "services/api-server/crates/application/src/services/team_type_writer.rs"
QUALIFICATION_WRITER = ROOT / "services/api-server/crates/application/src/services/qualification_writer.rs"
PG_DISPATCH_ORDER_REPO = ROOT / "services/api-server/crates/infrastructure/src/repositories/pg_dispatch_order_repository.rs"
RULE_SERVICE = ROOT / "services/api-server/crates/application/src/services/dispatch_rule_service.rs"


def test_shared_validator_resolves_only_active_targets() -> None:
    source = ATTRIBUTES.read_text(encoding="utf-8")
    assert "pub trait ObjectReferenceValidator" in source
    assert "RepositoryObjectReferenceValidator" in source
    assert "active != Some(true)" in source
    for target in (
        "Department",
        "Team",
        "TeamType",
        "Equipment",
        "EquipmentType",
        "Stand",
        "TaskType",
        "Personnel",
        "Gate",
        "Terminal",
        "BaggageCarousel",
        "Qualification",
    ):
        assert f'"{target}"' in source, f"shared validator missing target: {target}"


def test_shared_validator_rejects_unknown_target_as_config_error() -> None:
    """未知 object_name_target 是字段定义配置错误（ValidationError），不能落到
    `_ => None` 再当成 409「目标不存在」——那会把配置错误伪装成业务冲突。"""
    source = ATTRIBUTES.read_text(encoding="utf-8")
    assert "配置了不支持的对象引用目标" in source
    impl = source[source.index("impl ObjectReferenceValidator for RepositoryObjectReferenceValidator"):]
    match_block = impl[impl.index("let active = match target"):]
    match_block = match_block[: match_block.index("if active != Some(true)")]
    assert "_ => None" not in match_block
    assert "DomainError::ValidationError" in match_block


def test_dispatch_order_creation_invokes_shared_reference_validator() -> None:
    source = ORDER_LIFECYCLE.read_text(encoding="utf-8")
    assert 'validator.validate("DispatchOrder", &attributes)' in source


def test_qualification_creation_invokes_shared_reference_validator() -> None:
    source = RULE_SERVICE.read_text(encoding="utf-8")
    assert 'validator.validate("Qualification", &attributes)' in source
    assert "with_object_reference_validator" in source


def test_production_dispatch_di_wires_reference_validator() -> None:
    source = DI.read_text(encoding="utf-8")
    assert "RepositoryObjectReferenceValidator::new" in source
    assert "object_reference_validator: Some(object_reference_validator)" in source


def test_dispatch_rule_center_passes_directory_candidates_to_overlay_forms() -> None:
    source = RULE_CENTER.read_text(encoding="utf-8")
    assert "const fieldReferenceEntries = computed" in source
    assert source.count(":field-reference-entries=\"fieldReferenceEntries\"") >= 2


def test_personnel_runtime_attributes_and_reference_index_share_uow() -> None:
    source = PERSONNEL_WRITER.read_text(encoding="utf-8")
    assert "PersonnelRuntimeAttributeTransactionalWriter" in source
    assert "self.runtime_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert source.index("save_in_tx") < source.index("replace_owner_references_in_tx")
    assert "self.uow.commit(tx).await" in source


def test_personnel_attribute_service_prefers_atomic_writer_when_wired() -> None:
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_personnel_runtime_writer" in source
    assert "collect_attribute_references" in source
    assert "writer.save_with_references(&runtime, &references).await?" in source


def test_department_owner_and_reference_index_share_uow() -> None:
    source = DEPARTMENT_WRITER.read_text(encoding="utf-8")
    assert "DepartmentAttributeTransactionalWriter" in source
    assert "self.department_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert "self.uow.commit(tx).await?" in source


def test_team_owner_and_reference_index_share_uow() -> None:
    source = TEAM_WRITER.read_text(encoding="utf-8")
    assert "TeamAttributeTransactionalWriter" in source
    assert "self.team_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert "self.uow.commit(tx).await?" in source


def test_team_resource_service_prefers_atomic_writer_when_wired() -> None:
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_team_writer" in source
    assert len(re.findall(r'collect_attribute_references\(\s*"Team"', source)) >= 3
    assert "writer.save_with_references(&team, &references).await" in source


def test_equipment_type_owner_and_reference_index_share_uow() -> None:
    source = EQUIPMENT_TYPE_WRITER.read_text(encoding="utf-8")
    assert "EquipmentTypeAttributeTransactionalWriter" in source
    assert "self.equipment_type_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert "self.uow.commit(tx).await?" in source


def test_equipment_type_resource_service_prefers_atomic_writer_when_wired() -> None:
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_equipment_type_writer" in source
    assert source.count('collect_attribute_references(\n                "EquipmentType"') >= 3
    assert "writer.save_with_references(&equipment_type, &references).await" in source


def test_equipment_owner_and_reference_index_share_uow() -> None:
    source = EQUIPMENT_WRITER.read_text(encoding="utf-8")
    assert "EquipmentAttributeTransactionalWriter" in source
    assert "self.equipment_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert "self.uow.commit(tx).await?" in source


def test_equipment_resource_service_prefers_atomic_writer_when_wired() -> None:
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_equipment_writer" in source
    assert source.count('collect_attribute_references(\n                "Equipment"') >= 2
    assert "writer.save_with_references(&equipment, &references).await" in source


def test_task_type_owner_and_reference_index_share_uow() -> None:
    source = TASK_TYPE_WRITER.read_text(encoding="utf-8")
    assert "TaskTypeAttributeTransactionalWriter" in source
    assert "self.task_type_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert "self.uow.commit(tx).await?" in source


def test_task_type_resource_service_prefers_atomic_writer_when_wired() -> None:
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_task_type_writer" in source
    assert source.count('collect_attribute_references(\n                "TaskType"') >= 2
    assert "writer.save_with_references(&task_type, &references).await" in source


def test_terminal_resource_owner_and_reference_index_share_uow() -> None:
    source = TERMINAL_RESOURCE_WRITER.read_text(encoding="utf-8")
    assert "TerminalResourceAttributeTransactionalWriter" in source
    # 全部 7 个 writer 方法：四类基础 save + 三个带 Terminal attach 的 save。
    for save_call in (
        "save_terminal_in_tx",
        "save_gate_in_tx",
        "save_gate_with_terminal_in_tx",
        "save_carousel_in_tx",
        "save_carousel_with_terminal_in_tx",
        "save_stand_in_tx",
        "save_stand_with_terminal_in_tx",
    ):
        assert save_call in source, f"missing writer call: {save_call}"
    # 每个方法内 owner 保存、引用替换、commit 必须在同一事务里顺序出现。
    impl_start = source.index("impl<U> TerminalResourceAttributeTransactionalWriter")
    for method in (
        "save_terminal_with_references",
        "save_gate_with_references",
        "save_gate_with_terminal_and_references",
        "save_carousel_with_references",
        "save_carousel_with_terminal_and_references",
        "save_stand_with_references",
        "save_stand_with_terminal_and_references",
    ):
        # 从 impl 块起找，跳过 trait 里的同名声明。
        start = source.index(f"async fn {method}(", impl_start)
        rest = source[start + 1:]
        end = start + 1 + rest.index("async fn ") if "async fn " in rest else len(source)
        body = source[start:end]
        save_match = re.search(r"self\s*\.\s*resource_repo", body)
        assert save_match is not None, f"{method}: missing transactional owner save"
        save_pos = save_match.start()
        replace_pos = body.index("replace_owner_references_in_tx")
        commit_pos = body.index("self.uow.commit(tx).await?")
        assert save_pos < replace_pos < commit_pos, (
            f"{method}: owner save / reference replace / commit 顺序异常"
        )
    # 至少覆盖基础四类；当前应为 7（每方法各一次）。
    assert source.count("replace_owner_references_in_tx") >= 4
    assert source.count("self.uow.commit(tx).await?") >= 4


def test_terminal_resource_service_prefers_atomic_directory_writer_when_wired() -> None:
    source = TERMINAL_RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_attribute_writer" in source
    assert "collect_attribute_references" in source
    # 全部 7 个 writer 方法都要被 service 调用。
    for call in (
        "save_terminal_with_references",
        "save_gate_with_references",
        "save_gate_with_terminal_and_references",
        "save_carousel_with_references",
        "save_carousel_with_terminal_and_references",
        "save_stand_with_references",
        "save_stand_with_terminal_and_references",
    ):
        assert call in source, f"service 未调用 writer 方法: {call}"


def test_terminal_resource_service_only_syncs_references_without_writer() -> None:
    """writer 路径不得在事务提交后再做第二次非事务同步（半成功风险）。"""
    source = TERMINAL_RESOURCE_SERVICE.read_text(encoding="utf-8")
    lines = source.splitlines()
    sync_lines = [i for i, line in enumerate(lines) if "sync_attribute_references(" in line]
    assert sync_lines, "service 应保留无 writer 路径的 sync_attribute_references 兜底"
    for i in sync_lines:
        window = "\n".join(lines[max(0, i - 6):i])
        assert "self.attribute_writer.is_none()" in window, (
            f"第 {i + 1} 行的 sync_attribute_references 未包在 attribute_writer.is_none() 守卫内"
        )


def test_terminal_resource_update_cannot_bypass_deactivate_checks() -> None:
    """update_* 路径带 is_active=false 时必须复用 deactivate 的占用/引用检查。"""
    source = TERMINAL_RESOURCE_SERVICE.read_text(encoding="utf-8")
    for method, conflict_check in (
        ("update_gate", "active_gate_assignments"),
        ("update_carousel", "active_carousel_assignments"),
        ("update_stand", "active_stand_occupations"),
    ):
        start = source.index(f"pub async fn {method}(")
        rest = source[start + 1:]
        end = start + 1 + rest.index("pub async fn ") if "pub async fn " in rest else len(source)
        body = source[start:end]
        assert "reject_referenced_target" in body, (
            f"{method}: 停用前未做 object_ref 引用检查"
        )
        assert conflict_check in body, f"{method}: 停用前未做占用/分配冲突检查"
        # 检查必须在 is_active 赋值之前发生。
        check_pos = body.index(conflict_check)
        assign_pos = body.index("is_active = is_active")
        assert check_pos < assign_pos, f"{method}: 冲突检查必须在 is_active 赋值之前"


def test_team_type_owner_and_reference_index_share_uow() -> None:
    source = TEAM_TYPE_WRITER.read_text(encoding="utf-8")
    assert "TeamTypeAttributeTransactionalWriter" in source
    assert "self.team_type_repo.save_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert 'replace_owner_references_in_tx(&mut tx, "TeamType"' in source
    assert source.index("save_in_tx") < source.index("replace_owner_references_in_tx")
    assert "self.uow.commit(tx).await?" in source


def test_team_type_resource_service_prefers_atomic_writer_when_wired() -> None:
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    assert "with_team_type_writer" in source
    assert source.count('collect_attribute_references(\n                "TeamType"') >= 2
    assert "writer.save_with_references(&team_type, &references).await" in source


def test_qualification_catalog_and_reference_index_share_uow() -> None:
    source = QUALIFICATION_WRITER.read_text(encoding="utf-8")
    assert "QualificationAttributeTransactionalWriter" in source
    assert "self.qualification_repo.save_catalog_in_tx" in source
    assert "replace_owner_references_in_tx" in source
    assert 'replace_owner_references_in_tx(&mut tx, "Qualification"' in source
    assert source.index("save_catalog_in_tx") < source.index("replace_owner_references_in_tx")
    assert "self.uow.commit(tx).await?" in source


def test_qualification_creation_uses_atomic_writer_when_wired() -> None:
    source = RULE_SERVICE.read_text(encoding="utf-8")
    assert "with_qualification_writer" in source
    assert "writer.save_catalog_with_references(&item, &references).await" in source
    assert 'collect_attribute_references(\n                "Qualification"' in source


def test_dispatch_order_atomic_create_writes_reference_index_in_same_tx() -> None:
    """create_order_atomic 事务内必须同时写 owner 行与 object_ref 引用投影。"""
    source = PG_DISPATCH_ORDER_REPO.read_text(encoding="utf-8")
    assert "attribute_references" in source
    assert "replace_owner_references_in_transaction" in source
    # 写入必须发生在 persist_order_command_in_tx 内（create/save_orders_atomic 共用），
    # 且在 owner 保存之后、事务提交之前。
    start = source.index("async fn persist_order_command_in_tx")
    end = source.index("fn base_order_select", start)
    body = source[start:end]
    assert "save_order_in_tx" in body
    assert "replace_owner_references_in_transaction" in body
    assert body.index("save_order_in_tx") < body.index("replace_owner_references_in_transaction")
    assert "if let Some(references)" in body, "非属性写路径（None）不得清空引用索引"


def test_dispatch_order_creation_collects_references_for_atomic_write() -> None:
    source = ORDER_LIFECYCLE.read_text(encoding="utf-8")
    assert 'collect_attribute_references(\n            "DispatchOrder"' in source
    assert "attribute_references," in source


def test_personnel_status_and_position_prefer_atomic_writer_when_wired() -> None:
    """status/position 更新也必须走 writer（owner + 索引同事务），不能只留 sync 兜底。"""
    source = RESOURCE_SERVICE.read_text(encoding="utf-8")
    for method in ("update_personnel_status", "update_personnel_position"):
        start = source.index(f"pub async fn {method}(")
        rest = source[start + 1:]
        end = start + 1 + rest.index("pub async fn ") if "pub async fn " in rest else len(source)
        body = source[start:end]
        assert "personnel_runtime_writer.as_ref()" in body, (
            f"{method}: 缺少 personnel_runtime_writer 分支"
        )
        assert "writer.save_with_references(&runtime, &references).await?" in body
