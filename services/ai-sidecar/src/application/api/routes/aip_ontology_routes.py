"""
AIP Ontology 自定义配置 API 路由

提供 CRUD 操作的 REST API 端点。
"""

import logging
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel

from src.application.services.aip_ontology_service import AIPOntologyService
from src.infrastructure.ai.service_identity import require_service_identity
from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection

logger = logging.getLogger(__name__)

# Ontology CRUD（含权限策略写操作）必须携带服务身份令牌，
# 与 management_routes / api_routes 保持一致的鉴权基线。
router = APIRouter(
    prefix="/aip/ontology",
    tags=["AIP Ontology"],
    dependencies=[Depends(require_service_identity)],
)


class ObjectCreateRequest(BaseModel):
    name: str
    plural_name: str | None = None
    description: str | None = None
    is_abstract: bool = False
    properties: list[dict[str, Any]] = []
    relationships: list[dict[str, Any]] = []
    actions: list[str] = []
    tags: list[str] = []
    metadata: dict[str, Any] = {}
    is_active: bool = True


class ObjectUpdateRequest(BaseModel):
    plural_name: str | None = None
    description: str | None = None
    is_abstract: bool | None = None
    properties: list[dict[str, Any]] | None = None
    relationships: list[dict[str, Any]] | None = None
    actions: list[str] | None = None
    tags: list[str] | None = None
    metadata: dict[str, Any] | None = None
    is_active: bool | None = None


class ActionCreateRequest(BaseModel):
    name: str
    object_type: str
    description: str | None = None
    category: str = "mutation"
    parameters: list[dict[str, Any]] = []
    requires_approval: bool = False
    risk_level: str = "NORMAL"
    constraint_rules: list[dict[str, Any]] = []
    metadata: dict[str, Any] = {}
    is_active: bool = True


class ActionUpdateRequest(BaseModel):
    description: str | None = None
    category: str | None = None
    parameters: list[dict[str, Any]] | None = None
    requires_approval: bool | None = None
    risk_level: str | None = None
    constraint_rules: list[dict[str, Any]] | None = None
    metadata: dict[str, Any] | None = None
    is_active: bool | None = None


class PolicyCreateRequest(BaseModel):
    object_type: str
    object_id: str | None = None
    principal_type: str = "user"
    principal_id: str
    permission: str
    granted: bool = True
    conditions: dict[str, Any] | None = None
    description: str | None = None
    expires_at: str | None = None


class PolicyUpdateRequest(BaseModel):
    object_id: str | None = None
    principal_type: str | None = None
    principal_id: str | None = None
    permission: str | None = None
    granted: bool | None = None
    conditions: dict[str, Any] | None = None
    description: str | None = None
    expires_at: str | None = None
    is_active: bool | None = None


class FunctionCreateRequest(BaseModel):
    name: str
    category: str = "object_action"
    object_type: str
    action_name: str
    description: str | None = None
    parameters_schema: dict[str, Any] = {}
    requires_approval: bool = False
    risk_level: str = "NORMAL"
    permission_required: str | None = None
    tags: list[str] = []
    examples: list[dict[str, Any]] = []
    metadata: dict[str, Any] = {}
    is_active: bool = True


class FunctionUpdateRequest(BaseModel):
    description: str | None = None
    parameters_schema: dict[str, Any] | None = None
    requires_approval: bool | None = None
    risk_level: str | None = None
    permission_required: str | None = None
    tags: list[str] | None = None
    examples: list[dict[str, Any]] | None = None
    metadata: dict[str, Any] | None = None
    is_active: bool | None = None


class MappingCreateRequest(BaseModel):
    tool_name: str
    object_type: str
    action_name: str
    requires_approval: bool = False
    risk_level: str = "NORMAL"
    migration_status: str = "not_started"
    custom_handler: str | None = None
    metadata: dict[str, Any] = {}
    is_active: bool = True


class MappingUpdateRequest(BaseModel):
    object_type: str | None = None
    action_name: str | None = None
    requires_approval: bool | None = None
    risk_level: str | None = None
    migration_status: str | None = None
    custom_handler: str | None = None
    metadata: dict[str, Any] | None = None
    is_active: bool | None = None


class ConstraintCreateRequest(BaseModel):
    name: str
    object_type: str
    action_name: str | None = None
    constraint_type: str
    expression: str
    error_message: str | None = None
    severity: str = "NORMAL"
    metadata: dict[str, Any] = {}
    is_active: bool = True


class ConstraintUpdateRequest(BaseModel):
    name: str | None = None
    expression: str | None = None
    error_message: str | None = None
    severity: str | None = None
    metadata: dict[str, Any] | None = None
    is_active: bool | None = None


async def get_db_pool() -> AsyncPooledDatabaseConnection:
    from src.di.container import get_container

    container = get_container()
    return container.async_connection_pool


async def get_service(db_pool: AsyncPooledDatabaseConnection = Depends(get_db_pool)) -> AIPOntologyService:
    return AIPOntologyService(db_pool)


@router.get("/summary")
async def get_ontology_summary(service: AIPOntologyService = Depends(get_service)) -> dict[str, Any]:
    """获取 Ontology 汇总信息"""
    return await service.get_summary()


@router.get("/objects")
async def list_objects(
    include_inactive: bool = False,
    limit: int = Query(default=100, le=500),
    offset: int = 0,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取所有对象定义"""
    objects = await service.get_all_objects(include_inactive, limit, offset)
    return {
        "items": [
            obj.to_dict()
            if hasattr(obj, "to_dict")
            else {
                "id": obj.id,
                "name": obj.name,
                "plural_name": obj.plural_name,
                "description": obj.description,
                "is_abstract": obj.is_abstract,
                "properties": [p.to_dict() if hasattr(p, "to_dict") else p for p in obj.properties],
                "relationships": [r.to_dict() if hasattr(r, "to_dict") else r for r in obj.relationships],
                "actions": obj.actions,
                "tags": obj.tags,
                "metadata": obj.metadata,
                "is_active": obj.is_active,
            }
            for obj in objects
        ],
        "total": len(objects),
        "limit": limit,
        "offset": offset,
    }


@router.get("/objects/{object_id}")
async def get_object(
    object_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取对象定义"""
    obj = await service.get_object_by_id(object_id)
    if not obj:
        obj = await service.get_object_by_name(object_id)
    if not obj:
        raise HTTPException(status_code=404, detail="对象定义不存在")
    return {
        "id": obj.id,
        "name": obj.name,
        "plural_name": obj.plural_name,
        "description": obj.description,
        "is_abstract": obj.is_abstract,
        "properties": [p.to_dict() if hasattr(p, "to_dict") else p for p in obj.properties],
        "relationships": [r.to_dict() if hasattr(r, "to_dict") else r for r in obj.relationships],
        "actions": obj.actions,
        "tags": obj.tags,
        "metadata": obj.metadata,
        "is_active": obj.is_active,
    }


@router.post("/objects", status_code=201)
async def create_object(
    request: ObjectCreateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """创建对象定义"""
    obj = await service.create_object(request.model_dump(exclude_none=True))
    return {
        "id": obj.id,
        "name": obj.name,
        "plural_name": obj.plural_name,
        "description": obj.description,
        "is_abstract": obj.is_abstract,
        "properties": [p.to_dict() if hasattr(p, "to_dict") else p for p in obj.properties],
        "relationships": [r.to_dict() if hasattr(r, "to_dict") else r for r in obj.relationships],
        "actions": obj.actions,
        "tags": obj.tags,
        "metadata": obj.metadata,
        "is_active": obj.is_active,
    }


@router.put("/objects/{object_id}")
async def update_object(
    object_id: str,
    request: ObjectUpdateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """更新对象定义"""
    obj = await service.update_object(object_id, request.model_dump(exclude_none=True))
    if not obj:
        raise HTTPException(status_code=404, detail="对象定义不存在")
    return {
        "id": obj.id,
        "name": obj.name,
        "plural_name": obj.plural_name,
        "description": obj.description,
        "is_abstract": obj.is_abstract,
        "properties": [p.to_dict() if hasattr(p, "to_dict") else p for p in obj.properties],
        "relationships": [r.to_dict() if hasattr(r, "to_dict") else r for r in obj.relationships],
        "actions": obj.actions,
        "tags": obj.tags,
        "metadata": obj.metadata,
        "is_active": obj.is_active,
    }


@router.delete("/objects/{object_id}")
async def delete_object(
    object_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, bool]:
    """删除对象定义"""
    success = await service.delete_object(object_id)
    if not success:
        raise HTTPException(status_code=404, detail="对象定义不存在")
    return {"deleted": True}


@router.get("/actions")
async def list_actions(
    object_type: str | None = None,
    include_inactive: bool = False,
    limit: int = Query(default=100, le=500),
    offset: int = 0,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取所有动作定义"""
    actions = await service.get_all_actions(object_type, include_inactive, limit, offset)
    return {
        "items": [
            {
                "id": a.id,
                "name": a.name,
                "object_type": a.object_type,
                "description": a.description,
                "category": a.category.value if hasattr(a.category, "value") else a.category,
                "parameters": [p.to_dict() if hasattr(p, "to_dict") else p for p in a.parameters],
                "requires_approval": a.requires_approval,
                "risk_level": a.risk_level.value if hasattr(a.risk_level, "value") else a.risk_level,
                "constraint_rules": a.constraint_rules,
                "metadata": a.metadata,
                "is_active": a.is_active,
            }
            for a in actions
        ],
        "total": len(actions),
        "limit": limit,
        "offset": offset,
    }


@router.get("/actions/{action_id}")
async def get_action(
    action_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取动作定义"""
    action = await service.get_action_by_id(action_id)
    if not action:
        raise HTTPException(status_code=404, detail="动作定义不存在")
    return {
        "id": action.id,
        "name": action.name,
        "object_type": action.object_type,
        "description": action.description,
        "category": action.category.value if hasattr(action.category, "value") else action.category,
        "parameters": [p.to_dict() if hasattr(p, "to_dict") else p for p in action.parameters],
        "requires_approval": action.requires_approval,
        "risk_level": action.risk_level.value if hasattr(action.risk_level, "value") else action.risk_level,
        "constraint_rules": action.constraint_rules,
        "metadata": action.metadata,
        "is_active": action.is_active,
    }


@router.post("/actions", status_code=201)
async def create_action(
    request: ActionCreateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """创建动作定义"""
    action = await service.create_action(request.model_dump(exclude_none=True))
    return {
        "id": action.id,
        "name": action.name,
        "object_type": action.object_type,
        "description": action.description,
        "category": action.category.value if hasattr(action.category, "value") else action.category,
        "parameters": [p.to_dict() if hasattr(p, "to_dict") else p for p in action.parameters],
        "requires_approval": action.requires_approval,
        "risk_level": action.risk_level.value if hasattr(action.risk_level, "value") else action.risk_level,
        "constraint_rules": action.constraint_rules,
        "metadata": action.metadata,
        "is_active": action.is_active,
    }


@router.put("/actions/{action_id}")
async def update_action(
    action_id: str,
    request: ActionUpdateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """更新动作定义"""
    action = await service.update_action(action_id, request.model_dump(exclude_none=True))
    if not action:
        raise HTTPException(status_code=404, detail="动作定义不存在")
    return {
        "id": action.id,
        "name": action.name,
        "object_type": action.object_type,
        "description": action.description,
        "category": action.category.value if hasattr(action.category, "value") else action.category,
        "parameters": [p.to_dict() if hasattr(p, "to_dict") else p for p in action.parameters],
        "requires_approval": action.requires_approval,
        "risk_level": action.risk_level.value if hasattr(action.risk_level, "value") else action.risk_level,
        "constraint_rules": action.constraint_rules,
        "metadata": action.metadata,
        "is_active": action.is_active,
    }


@router.delete("/actions/{action_id}")
async def delete_action(
    action_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, bool]:
    """删除动作定义"""
    success = await service.delete_action(action_id)
    if not success:
        raise HTTPException(status_code=404, detail="动作定义不存在")
    return {"deleted": True}


@router.get("/policies")
async def list_policies(
    principal_id: str | None = None,
    object_type: str | None = None,
    limit: int = Query(default=100, le=500),
    offset: int = 0,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取所有策略"""
    policies = await service.get_all_policies(principal_id, object_type, limit, offset)
    return {
        "items": [
            {
                "id": p.id,
                "object_type": p.object_type,
                "object_id": p.object_id,
                "principal_type": p.principal_type.value if hasattr(p.principal_type, "value") else p.principal_type,
                "principal_id": p.principal_id,
                "permission": p.permission.value if hasattr(p.permission, "value") else p.permission,
                "granted": p.granted,
                "conditions": p.conditions,
                "description": p.description,
                "expires_at": p.expires_at.isoformat() if p.expires_at else None,
            }
            for p in policies
        ],
        "total": len(policies),
        "limit": limit,
        "offset": offset,
    }


@router.get("/policies/{policy_id}")
async def get_policy(
    policy_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取策略"""
    policy = await service.get_policy_by_id(policy_id)
    if not policy:
        raise HTTPException(status_code=404, detail="策略不存在")
    return {
        "id": policy.id,
        "object_type": policy.object_type,
        "object_id": policy.object_id,
        "principal_type": policy.principal_type.value
        if hasattr(policy.principal_type, "value")
        else policy.principal_type,
        "principal_id": policy.principal_id,
        "permission": policy.permission.value if hasattr(policy.permission, "value") else policy.permission,
        "granted": policy.granted,
        "conditions": policy.conditions,
        "description": policy.description,
        "expires_at": policy.expires_at.isoformat() if policy.expires_at else None,
    }


@router.post("/policies", status_code=201)
async def create_policy(
    request: PolicyCreateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """创建策略"""
    policy = await service.create_policy(request.model_dump(exclude_none=True))
    return {
        "id": policy.id,
        "object_type": policy.object_type,
        "object_id": policy.object_id,
        "principal_type": policy.principal_type.value
        if hasattr(policy.principal_type, "value")
        else policy.principal_type,
        "principal_id": policy.principal_id,
        "permission": policy.permission.value if hasattr(policy.permission, "value") else policy.permission,
        "granted": policy.granted,
        "conditions": policy.conditions,
        "description": policy.description,
        "expires_at": policy.expires_at.isoformat() if policy.expires_at else None,
    }


@router.put("/policies/{policy_id}")
async def update_policy(
    policy_id: str,
    request: PolicyUpdateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """更新策略"""
    policy = await service.update_policy(policy_id, request.model_dump(exclude_none=True))
    if not policy:
        raise HTTPException(status_code=404, detail="策略不存在")
    return {
        "id": policy.id,
        "object_type": policy.object_type,
        "object_id": policy.object_id,
        "principal_type": policy.principal_type.value
        if hasattr(policy.principal_type, "value")
        else policy.principal_type,
        "principal_id": policy.principal_id,
        "permission": policy.permission.value if hasattr(policy.permission, "value") else policy.permission,
        "granted": policy.granted,
        "conditions": policy.conditions,
        "description": policy.description,
        "expires_at": policy.expires_at.isoformat() if policy.expires_at else None,
    }


@router.delete("/policies/{policy_id}")
async def delete_policy(
    policy_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, bool]:
    """删除策略"""
    success = await service.delete_policy(policy_id)
    if not success:
        raise HTTPException(status_code=404, detail="策略不存在")
    return {"deleted": True}


@router.get("/functions")
async def list_functions(
    object_type: str | None = None,
    category: str | None = None,
    include_inactive: bool = False,
    limit: int = Query(default=100, le=500),
    offset: int = 0,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取所有函数"""
    functions = await service.get_all_functions(object_type, category, include_inactive, limit, offset)
    return {
        "items": [
            {
                "id": f.id,
                "name": f.name,
                "category": f.category,
                "object_type": f.object_type,
                "action_name": f.action_name,
                "description": f.description,
                "parameters_schema": f.parameters_schema,
                "requires_approval": f.requires_approval,
                "risk_level": f.risk_level.value if hasattr(f.risk_level, "value") else f.risk_level,
                "permission_required": f.permission_required,
                "tags": f.tags,
                "examples": f.examples,
                "metadata": f.metadata,
                "is_active": f.is_active,
            }
            for f in functions
        ],
        "total": len(functions),
        "limit": limit,
        "offset": offset,
    }


@router.get("/functions/{function_id}")
async def get_function(
    function_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取函数"""
    func = await service.get_function_by_id(function_id)
    if not func:
        func = await service.get_function_by_name(function_id)
    if not func:
        raise HTTPException(status_code=404, detail="函数不存在")
    return {
        "id": func.id,
        "name": func.name,
        "category": func.category,
        "object_type": func.object_type,
        "action_name": func.action_name,
        "description": func.description,
        "parameters_schema": func.parameters_schema,
        "requires_approval": func.requires_approval,
        "risk_level": func.risk_level.value if hasattr(func.risk_level, "value") else func.risk_level,
        "permission_required": func.permission_required,
        "tags": func.tags,
        "examples": func.examples,
        "metadata": func.metadata,
        "is_active": func.is_active,
    }


@router.post("/functions", status_code=201)
async def create_function(
    request: FunctionCreateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """创建函数"""
    func = await service.create_function(request.model_dump(exclude_none=True))
    return {
        "id": func.id,
        "name": func.name,
        "category": func.category,
        "object_type": func.object_type,
        "action_name": func.action_name,
        "description": func.description,
        "parameters_schema": func.parameters_schema,
        "requires_approval": func.requires_approval,
        "risk_level": func.risk_level.value if hasattr(func.risk_level, "value") else func.risk_level,
        "permission_required": func.permission_required,
        "tags": func.tags,
        "examples": func.examples,
        "metadata": func.metadata,
        "is_active": func.is_active,
    }


@router.put("/functions/{function_id}")
async def update_function(
    function_id: str,
    request: FunctionUpdateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """更新函数"""
    func = await service.update_function(function_id, request.model_dump(exclude_none=True))
    if not func:
        raise HTTPException(status_code=404, detail="函数不存在")
    return {
        "id": func.id,
        "name": func.name,
        "category": func.category,
        "object_type": func.object_type,
        "action_name": func.action_name,
        "description": func.description,
        "parameters_schema": func.parameters_schema,
        "requires_approval": func.requires_approval,
        "risk_level": func.risk_level.value if hasattr(func.risk_level, "value") else func.risk_level,
        "permission_required": func.permission_required,
        "tags": func.tags,
        "examples": func.examples,
        "metadata": func.metadata,
        "is_active": func.is_active,
    }


@router.delete("/functions/{function_id}")
async def delete_function(
    function_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, bool]:
    """删除函数"""
    success = await service.delete_function(function_id)
    if not success:
        raise HTTPException(status_code=404, detail="函数不存在")
    return {"deleted": True}


@router.get("/mappings")
async def list_mappings(
    object_type: str | None = None,
    migration_status: str | None = None,
    include_inactive: bool = False,
    limit: int = Query(default=100, le=500),
    offset: int = 0,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取所有工具映射"""
    mappings = await service.get_all_mappings(object_type, migration_status, include_inactive, limit, offset)
    return {
        "items": [
            {
                "id": m.id,
                "tool_name": m.tool_name,
                "object_type": m.object_type,
                "action_name": m.action_name,
                "requires_approval": m.requires_approval,
                "risk_level": m.risk_level.value if hasattr(m.risk_level, "value") else m.risk_level,
                "migration_status": m.migration_status.value
                if hasattr(m.migration_status, "value")
                else m.migration_status,
                "custom_handler": m.custom_handler,
                "metadata": m.metadata,
                "is_active": m.is_active,
            }
            for m in mappings
        ],
        "total": len(mappings),
        "limit": limit,
        "offset": offset,
    }


@router.get("/mappings/{mapping_id}")
async def get_mapping(
    mapping_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取工具映射"""
    mapping = await service.get_mapping_by_id(mapping_id)
    if not mapping:
        raise HTTPException(status_code=404, detail="映射不存在")
    return {
        "id": mapping.id,
        "tool_name": mapping.tool_name,
        "object_type": mapping.object_type,
        "action_name": mapping.action_name,
        "requires_approval": mapping.requires_approval,
        "risk_level": mapping.risk_level.value if hasattr(mapping.risk_level, "value") else mapping.risk_level,
        "migration_status": mapping.migration_status.value
        if hasattr(mapping.migration_status, "value")
        else mapping.migration_status,
        "custom_handler": mapping.custom_handler,
        "metadata": mapping.metadata,
        "is_active": mapping.is_active,
    }


@router.post("/mappings", status_code=201)
async def create_mapping(
    request: MappingCreateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """创建工具映射"""
    mapping = await service.create_mapping(request.model_dump(exclude_none=True))
    return {
        "id": mapping.id,
        "tool_name": mapping.tool_name,
        "object_type": mapping.object_type,
        "action_name": mapping.action_name,
        "requires_approval": mapping.requires_approval,
        "risk_level": mapping.risk_level.value if hasattr(mapping.risk_level, "value") else mapping.risk_level,
        "migration_status": mapping.migration_status.value
        if hasattr(mapping.migration_status, "value")
        else mapping.migration_status,
        "custom_handler": mapping.custom_handler,
        "metadata": mapping.metadata,
        "is_active": mapping.is_active,
    }


@router.put("/mappings/{mapping_id}")
async def update_mapping(
    mapping_id: str,
    request: MappingUpdateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """更新工具映射"""
    mapping = await service.update_mapping(mapping_id, request.model_dump(exclude_none=True))
    if not mapping:
        raise HTTPException(status_code=404, detail="映射不存在")
    return {
        "id": mapping.id,
        "tool_name": mapping.tool_name,
        "object_type": mapping.object_type,
        "action_name": mapping.action_name,
        "requires_approval": mapping.requires_approval,
        "risk_level": mapping.risk_level.value if hasattr(mapping.risk_level, "value") else mapping.risk_level,
        "migration_status": mapping.migration_status.value
        if hasattr(mapping.migration_status, "value")
        else mapping.migration_status,
        "custom_handler": mapping.custom_handler,
        "metadata": mapping.metadata,
        "is_active": mapping.is_active,
    }


@router.delete("/mappings/{mapping_id}")
async def delete_mapping(
    mapping_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, bool]:
    """删除工具映射"""
    success = await service.delete_mapping(mapping_id)
    if not success:
        raise HTTPException(status_code=404, detail="映射不存在")
    return {"deleted": True}


@router.get("/constraints")
async def list_constraints(
    object_type: str | None = None,
    action_name: str | None = None,
    include_inactive: bool = False,
    limit: int = Query(default=100, le=500),
    offset: int = 0,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取所有约束"""
    constraints = await service.get_all_constraints(object_type, action_name, include_inactive, limit, offset)
    return {
        "items": [
            {
                "id": c.id,
                "name": c.name,
                "object_type": c.object_type,
                "action_name": c.action_name,
                "constraint_type": c.constraint_type.value
                if hasattr(c.constraint_type, "value")
                else c.constraint_type,
                "expression": c.expression,
                "error_message": c.error_message,
                "severity": c.severity.value if hasattr(c.severity, "value") else c.severity,
                "metadata": c.metadata,
                "is_active": c.is_active,
            }
            for c in constraints
        ],
        "total": len(constraints),
        "limit": limit,
        "offset": offset,
    }


@router.get("/constraints/{constraint_id}")
async def get_constraint(
    constraint_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """获取约束"""
    constraint = await service.get_constraint_by_id(constraint_id)
    if not constraint:
        raise HTTPException(status_code=404, detail="约束不存在")
    return {
        "id": constraint.id,
        "name": constraint.name,
        "object_type": constraint.object_type,
        "action_name": constraint.action_name,
        "constraint_type": constraint.constraint_type.value
        if hasattr(constraint.constraint_type, "value")
        else constraint.constraint_type,
        "expression": constraint.expression,
        "error_message": constraint.error_message,
        "severity": constraint.severity.value if hasattr(constraint.severity, "value") else constraint.severity,
        "metadata": constraint.metadata,
        "is_active": constraint.is_active,
    }


@router.post("/constraints", status_code=201)
async def create_constraint(
    request: ConstraintCreateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """创建约束"""
    constraint = await service.create_constraint(request.model_dump(exclude_none=True))
    return {
        "id": constraint.id,
        "name": constraint.name,
        "object_type": constraint.object_type,
        "action_name": constraint.action_name,
        "constraint_type": constraint.constraint_type.value
        if hasattr(constraint.constraint_type, "value")
        else constraint.constraint_type,
        "expression": constraint.expression,
        "error_message": constraint.error_message,
        "severity": constraint.severity.value if hasattr(constraint.severity, "value") else constraint.severity,
        "metadata": constraint.metadata,
        "is_active": constraint.is_active,
    }


@router.put("/constraints/{constraint_id}")
async def update_constraint(
    constraint_id: str,
    request: ConstraintUpdateRequest,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """更新约束"""
    constraint = await service.update_constraint(constraint_id, request.model_dump(exclude_none=True))
    if not constraint:
        raise HTTPException(status_code=404, detail="约束不存在")
    return {
        "id": constraint.id,
        "name": constraint.name,
        "object_type": constraint.object_type,
        "action_name": constraint.action_name,
        "constraint_type": constraint.constraint_type.value
        if hasattr(constraint.constraint_type, "value")
        else constraint.constraint_type,
        "expression": constraint.expression,
        "error_message": constraint.error_message,
        "severity": constraint.severity.value if hasattr(constraint.severity, "value") else constraint.severity,
        "metadata": constraint.metadata,
        "is_active": constraint.is_active,
    }


@router.delete("/constraints/{constraint_id}")
async def delete_constraint(
    constraint_id: str,
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, bool]:
    """删除约束"""
    success = await service.delete_constraint(constraint_id)
    if not success:
        raise HTTPException(status_code=404, detail="约束不存在")
    return {"deleted": True}


@router.post("/reload", summary="热更新 Ontology 配置", tags=["AIP Ontology"])
async def reload_ontology(
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """
    热更新 Ontology 配置

    在修改数据库配置后调用此接口，使配置变更立即生效。
    这会让 AI 立即感知到 Ontology 的变化。

    Returns:
        更新后的汇总信息
    """
    try:
        from src.infrastructure.ai.ontology.data_loader import get_ontology_data_loader

        data_loader = get_ontology_data_loader()
        if data_loader:
            await data_loader.reload()
            return {"success": True, "message": "Ontology 配置已热更新", "summary": await service.get_summary()}
        else:
            return {
                "success": True,
                "message": "Data loader not initialized, but configuration is saved",
                "summary": await service.get_summary(),
            }
    except Exception as exc:
        logger.error("ontology_config_hot_reload_failed", exc_info=exc)
        return {
            "success": False,
            "error": "internal_error",
            "message": "配置已保存到数据库，但热更新失败。请重启服务使配置生效。",
        }


@router.get("/llm-context", summary="获取 LLM 上下文", tags=["AIP Ontology"])
async def get_llm_context(
    object_types: str | None = Query(default=None, description="逗号分隔的对象类型"),
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """
    获取 LLM 可理解的 Ontology 上下文

    这个接口用于：
    1. 验证 Ontology 配置是否正确
    2. 预览 AI 将看到的上下文
    3. 调试 Ontology 配置

    Returns:
        LLM 可理解的 Ontology 描述
    """
    try:
        from src.infrastructure.ai.ontology.data_loader import get_ontology_data_loader

        data_loader = get_ontology_data_loader()
        if data_loader:
            types = object_types.split(",") if object_types else None
            return data_loader.build_llm_context(types)
        else:
            return {"error": "Data loader not initialized"}
    except Exception as exc:
        logger.error("ontology_route_failed", exc_info=exc)
        raise HTTPException(status_code=500, detail="Internal server error") from None


@router.get("/tool-schemas", summary="获取 Tool Schemas", tags=["AIP Ontology"])
async def get_tool_schemas(
    object_types: str | None = Query(default=None, description="逗号分隔的对象类型"),
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """
    获取 OpenAI function calling 格式的 Tool Schemas

    这个接口返回的 schemas 可以直接用于 OpenAI API。
    当你修改 Ontology 配置后，这里的输出会相应变化。

    Returns:
        OpenAI function calling 格式的 schemas
    """
    try:
        from src.infrastructure.ai.ontology.data_loader import get_ontology_data_loader

        data_loader = get_ontology_data_loader()
        if data_loader:
            types = object_types.split(",") if object_types else None
            schemas = data_loader.generate_tool_schemas(types)
            return {"schemas": schemas, "count": len(schemas)}
        else:
            return {"error": "Data loader not initialized", "schemas": [], "count": 0}
    except Exception as exc:
        logger.error("ontology_route_failed", exc_info=exc)
        raise HTTPException(status_code=500, detail="Internal server error") from None


@router.post("/validate-action", summary="验证动作参数", tags=["AIP Ontology"])
async def validate_action(
    object_type: str = Query(..., description="对象类型"),
    action_name: str = Query(..., description="动作名称"),
    parameters: dict[str, Any] = Query(..., description="动作参数"),
    service: AIPOntologyService = Depends(get_service),
) -> dict[str, Any]:
    """
    验证动作参数是否符合 Ontology 定义

    这个接口用于：
    1. 在执行前验证参数
    2. 获取参数的提示信息
    3. 调试参数问题

    Returns:
        验证结果
    """
    try:
        from src.infrastructure.ai.ontology.data_loader import get_ontology_data_loader

        data_loader = get_ontology_data_loader()
        if data_loader:
            return data_loader.validate_action(object_type, action_name, parameters)
        else:
            return {"valid": True, "errors": [], "message": "Data loader not initialized"}
    except Exception as exc:
        logger.error("ontology_route_failed", exc_info=exc)
        raise HTTPException(status_code=500, detail="Internal server error") from None
