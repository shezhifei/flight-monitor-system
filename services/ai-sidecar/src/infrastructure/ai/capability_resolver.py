"""AI 能力解析器 - 运行时配置快照生成

在每次 LLM run 前解析实体配置，生成不可变的 ResolvedAiRuntimeConfig。
"""

from __future__ import annotations

import hashlib
import json
import logging
from dataclasses import dataclass, field
from typing import Any, Literal

from src.infrastructure.ai.mcp.annotations import normalize_mcp_tool_annotations
from src.infrastructure.common.exceptions import JSON_EXCEPTIONS
from src.infrastructure.common.runtime_utils import decode_jsonb_or_raise, is_production_environment, parse_json_field

logger = logging.getLogger(__name__)


@dataclass
class ResolvedModelConfig:
    """解析后的模型配置"""

    model_id: str
    provider_model: str
    api_format: Literal["chat_completions", "responses"]
    context_window: int
    max_output_tokens: int
    input_modalities: list[str]
    output_modalities: list[str]
    capabilities: dict[str, Any]
    cost: dict[str, Any]


@dataclass
class ResolvedToolConfig:
    """解析后的工具配置"""

    source: Literal["builtin", "mcp", "skill"]
    name: str
    display_name: str
    description: str
    parameters: dict[str, Any]
    risk_level: str = "low"
    cacheable: bool = False
    side_effect: bool = False

    def to_schema(self) -> dict[str, Any]:
        """Convert to OpenAI-compatible function schema."""
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        }


@dataclass
class ResolvedMcpConfig:
    """解析后的 MCP 配置"""

    enabled: bool
    server_count: int
    tool_count: int
    resource_count: int
    binding_ids: list[str] = field(default_factory=list)


@dataclass
class ResolvedSkillBinding:
    """解析后的单个 Skill binding 详情"""

    binding_id: str
    skill_slug: str
    version: str
    activation_policy: str
    max_instruction_tokens: int
    priority: int
    allowed_task_types: list[str] = field(default_factory=list)


@dataclass
class ResolvedSkillConfig:
    """解析后的 Skill 配置"""

    enabled: bool
    skill_count: int
    binding_ids: list[str] = field(default_factory=list)
    fail_closed: bool = False
    bindings: list[ResolvedSkillBinding] = field(default_factory=list)


@dataclass
class ResolvedSubagentsConfig:
    """解析后的子代理配置"""

    enabled: bool
    allowed_entity_ids: list[str]
    max_depth: int
    max_concurrency: int
    inherit_parent_context: bool


@dataclass
class ResolvedContextPolicy:
    """解析后的上下文策略"""

    strategy: str
    max_context_tokens: int
    compression_threshold_tokens: int
    preserve_recent_messages: int
    summary_model: str | None
    summary_max_tokens: int
    persist_summaries: bool


@dataclass
class ResolvedCachePolicy:
    """解析后的缓存策略"""

    enabled: bool
    provider_prompt_cache_enabled: bool
    provider_prompt_cache_retention: str | None
    provider_prompt_cache_namespace: str | None
    context_cache_enabled: bool
    context_cache_ttl: int
    tool_result_cache_enabled: bool
    tool_result_cache_ttl: int
    tool_result_cacheable_tools: list[str]
    mcp_resource_cache_enabled: bool
    mcp_resource_cache_ttl: int


@dataclass
class ResolvedAiRuntimeConfig:
    """解析后的 AI 运行时配置快照

    这是每次 LLM run 使用的不可变配置。所有配置在 run 开始前解析完成，
    运行中不反查动态配置。
    """

    entity_id: str
    config_version: int
    config_revision: int

    # 模型
    model_id: str
    model: ResolvedModelConfig

    # Provider
    provider_type: str
    base_url: str
    api_format: str
    timeout: float
    max_retries: int
    retry_delay: float

    # 工具
    tools: list[ResolvedToolConfig]
    tool_policy: dict[str, Any]

    # MCP
    mcp: ResolvedMcpConfig

    # Skills
    skills: ResolvedSkillConfig

    # 子代理
    subagents: ResolvedSubagentsConfig

    # 上下文策略
    context_policy: ResolvedContextPolicy

    # 缓存策略
    cache_policy: ResolvedCachePolicy

    # 安全
    security: dict[str, Any]

    # 提示词
    system_prompt: str
    task_template: str | None

    # Provider 凭据（按 provider_ref 解析；ASR/TTS 等旁路调用需要）。
    # 默认空串：当 entity 配置未携带 key 时，调用方回退到与主 LLM 同源的 env 凭据。
    api_key: str = ""

    # 快照哈希
    snapshot_hash: str = ""

    def __post_init__(self):
        if not self.snapshot_hash:
            self.snapshot_hash = self._compute_hash()

    def _compute_hash(self) -> str:
        """计算快照哈希，用于缓存键和审计"""
        hash_input = json.dumps(
            {
                "entity_id": self.entity_id,
                "config_version": self.config_version,
                "config_revision": self.config_revision,
                "model_id": self.model_id,
                "api_format": self.api_format,
                "tool_count": len(self.tools),
                "mcp_enabled": self.mcp.enabled,
                "skills_enabled": self.skills.enabled,
                "skill_bindings": [{"slug": b.skill_slug, "version": b.version} for b in self.skills.bindings],
            },
            sort_keys=True,
        )
        return hashlib.sha256(hash_input.encode()).hexdigest()[:16]


def is_tool_allowed(
    tool: dict[str, Any],
    tooling_config: dict[str, Any],
) -> bool:
    """Single authority for tool ACL checks — shared by resolver (build-time)
    and executor (run-time) to prevent logic drift.

    Checks denied_tools → allowed_tools → allowed_tool_categories in order.
    Deny lists take precedence over allow lists.

    Args:
        tool: Tool descriptor dict with "name" and optionally "category" keys.
        tooling_config: Entity tooling config with optional "denied_tools",
            "allowed_tools" (None = no restriction, list = whitelist),
            "allowed_tool_categories" keys.

    Returns:
        True if the tool is allowed, False if denied.
    """
    tool_name = tool.get("name", "")
    tool_category = tool.get("category", "")

    denied = tooling_config.get("denied_tools", [])
    if tool_name in denied:
        return False

    allowed_tools = tooling_config.get("allowed_tools")
    if allowed_tools is not None and tool_name not in allowed_tools:
        return False

    allowed_categories = tooling_config.get("allowed_tool_categories", [])
    return not (allowed_categories and tool_category not in allowed_categories)


def normalized_mcp_binding_tool_acl(binding: dict[str, Any]) -> dict[str, Any]:
    """Normalize security-sensitive MCP binding ACL fields.

    Security JSON parse failures raise instead of falling back to an empty
    allowlist, because empty lists can weaken entity-level ACLs.
    """
    normalized = dict(binding)
    for field_name in ("allowed_tools", "denied_tools", "allowed_resources"):
        if field_name in normalized:
            normalized[field_name] = decode_jsonb_or_raise(normalized[field_name], field_name)
    if "allowed_tool_categories" in normalized:
        normalized["allowed_tool_categories"] = parse_json_field(
            normalized["allowed_tool_categories"],
            default=[],
        )
    return normalized


class CapabilityResolver:
    """能力解析器

    职责：
    1. 读取实体配置
    2. 将配置标准化为当前 schema
    3. 解析 model_routing，选择本次 run 的模型用途
    4. 校验输入模态是否被目标模型允许
    5. 合并 builtin/MCP tools，并组合 Agent Skill instructions
    6. 计算 prompt cache key 所需的静态前缀指纹
    7. 输出 ResolvedAiRuntimeConfig
    """

    def __init__(
        self,
        config_store=None,
        model_catalog_repo=None,
        mcp_repo=None,
        skill_repo=None,
        builtin_tools: list[dict[str, Any]] | None = None,
    ):
        self._config_store = config_store
        self._model_catalog_repo = model_catalog_repo
        self._mcp_repo = mcp_repo
        self._skill_repo = skill_repo
        self._builtin_tools = builtin_tools or []

    async def resolve(
        self,
        entity_id: str,
        model_purpose: str = "chat",
        input_modalities: list[str] | None = None,
    ) -> ResolvedAiRuntimeConfig:
        """解析实体配置，生成运行时快照

        Args:
            entity_id: 实体 ID
            model_purpose: 模型用途 (chat/summary/vision/asr/tts/embedding)
            input_modalities: 本次请求的输入模态

        Returns:
            ResolvedAiRuntimeConfig: 不可变的运行时配置快照

        Raises:
            ValueError: 配置无效或模态不支持
        """
        # 1. 读取实体配置
        raw_config = await self._load_entity_config(entity_id)

        # 2. 标准化配置
        from src.infrastructure.ai.config.config_normalizer import normalize_config

        config = normalize_config(raw_config)

        config_version = config.get("config_version", 2)
        config_revision = (
            raw_config.get("_config_revision")
            or raw_config.get("config_revision")
            or raw_config.get("configRevision")
            or 1
        )

        # 3. 解析模型路由
        model_routing = config.get("model_routing", {})
        model_id = self._resolve_model_id(model_routing, model_purpose)

        # 4. 解析模型配置
        models_config = config.get("models", {})
        model_config = models_config.get(model_id, {})
        if not model_config:
            # 尝试从模型目录获取
            model_config = await self._get_model_from_catalog(model_id)

        # 5. 校验输入模态
        resolved_model = self._build_model_config(model_id, model_config)
        if input_modalities:
            self._validate_modalities(resolved_model, input_modalities)

        # 6. 解析 Provider
        # 按 model.provider_ref 选择 provider（缺失回退 'default'），
        # 支持 asr/tts 等用途路由到独立 provider。
        provider_ref = model_config.get("provider_ref", "default")
        provider_config = self._resolve_provider(config, provider_ref)

        # 7. 解析工具
        tooling_config = config.get("tooling", {})
        mcp_config = config.get("mcp", {})
        tools, tool_policy = await self._resolve_tools(entity_id, tooling_config, model_id, mcp_config)

        # 8. 解析 MCP
        mcp_config = config.get("mcp", {})
        resolved_mcp = await self._resolve_mcp(entity_id, mcp_config)

        # 9. 解析 Skills
        skills_config = config.get("skills", {})
        resolved_skills = await self._resolve_skills(entity_id, skills_config)

        # 10. 解析子代理
        subagents_config = config.get("subagents", {})
        resolved_subagents = self._resolve_subagents(subagents_config)

        # 11. 解析上下文策略
        context_policy = config.get("context_policy", {})
        resolved_context = self._resolve_context_policy(context_policy)

        # 12. 解析缓存策略
        cache_policy = config.get("cache_policy", {})
        resolved_cache = self._resolve_cache_policy(cache_policy)

        # 13. 解析安全闸门配置（确保 runtime 可读取的字段始终存在）
        security = self._resolve_security(config.get("security", {}))

        # 14. 构建快照
        snapshot = ResolvedAiRuntimeConfig(
            entity_id=entity_id,
            config_version=config_version,
            config_revision=config_revision,
            model_id=model_id,
            model=resolved_model,
            provider_type=provider_config.get("type", "openai_compatible"),
            base_url=provider_config.get("base_url", "https://api.openai.com/v1"),
            api_key=provider_config.get("api_key", ""),
            api_format=provider_config.get("api_format", "chat_completions"),
            timeout=provider_config.get("timeout", 30.0),
            max_retries=provider_config.get("max_retries", 3),
            retry_delay=provider_config.get("retry_delay", 0.5),
            tools=tools,
            tool_policy=tool_policy,
            mcp=resolved_mcp,
            skills=resolved_skills,
            subagents=resolved_subagents,
            context_policy=resolved_context,
            cache_policy=resolved_cache,
            security=security,
            system_prompt=config.get("system_prompt", ""),
            task_template=config.get("task_template"),
        )

        logger.info(
            f"Resolved config for entity={entity_id}, model={model_id}, "
            f"tools={len(tools)}, mcp={resolved_mcp.enabled}, "
            f"skills={resolved_skills.enabled}, hash={snapshot.snapshot_hash}"
        )

        return snapshot

    async def _load_entity_config(self, entity_id: str) -> dict[str, Any]:
        """加载实体配置"""
        if self._config_store:
            config = await self._config_store.get(entity_id)
            if config:
                return config
        return {}

    def _resolve_model_id(
        self,
        model_routing: dict[str, Any],
        purpose: str,
    ) -> str:
        """解析模型 ID"""
        # 按用途查找，回退到 default
        model_id = model_routing.get(purpose)
        if not model_id:
            model_id = model_routing.get("default", "gpt-4o")
        return model_id

    def _resolve_provider(
        self,
        config: dict[str, Any],
        provider_ref: str,
    ) -> dict[str, Any]:
        """按 provider_ref 从 providers 字典取连接配置，缺失回退 default。"""
        providers = config.get("providers") or {}
        provider = providers.get(provider_ref)
        if provider is not None:
            return provider
        default_provider = providers.get("default")
        if default_provider is not None:
            return default_provider
        return {}

    async def _get_model_from_catalog(self, model_id: str) -> dict[str, Any]:
        """从模型目录获取模型配置"""
        if self._model_catalog_repo:
            model = await self._model_catalog_repo.find_by_id(model_id)
            if model:
                return {
                    "provider_model": model.get("provider_model", model_id),
                    "api_format": model.get("api_format", "chat_completions"),
                    "context_window": model.get("context_window", 128000),
                    "max_output_tokens": model.get("max_output_tokens", 4096),
                    "modalities": model.get("input_modalities", ["text"]),
                    "capabilities": model.get("capabilities", {}),
                    "cost": model.get("cost", {}),
                }
        # 返回默认配置
        return {
            "provider_model": model_id,
            "api_format": "chat_completions",
            "context_window": 128000,
            "max_output_tokens": 4096,
        }

    def _build_model_config(
        self,
        model_id: str,
        raw: dict[str, Any],
    ) -> ResolvedModelConfig:
        """构建模型配置"""
        modalities = raw.get("modalities", {})
        if isinstance(modalities, list):
            input_modalities = modalities
            output_modalities = ["text"]
        else:
            input_modalities = modalities.get("input", ["text"])
            output_modalities = modalities.get("output", ["text"])

        return ResolvedModelConfig(
            model_id=model_id,
            provider_model=raw.get("provider_model", model_id),
            api_format=raw.get("api_format", "chat_completions"),
            context_window=raw.get("context_window", 128000),
            max_output_tokens=raw.get("max_output_tokens", 4096),
            input_modalities=input_modalities,
            output_modalities=output_modalities,
            capabilities=raw.get("capabilities", {}),
            cost=raw.get("cost", {}),
        )

    def _validate_modalities(
        self,
        model: ResolvedModelConfig,
        requested: list[str],
    ) -> None:
        """校验输入模态"""
        unsupported = [m for m in requested if m not in model.input_modalities]
        if unsupported:
            raise ValueError(
                f"AI_MODALITY_NOT_SUPPORTED: model {model.model_id} "
                f"does not support {unsupported}. "
                f"Configured input modalities: {model.input_modalities}"
            )

    async def _resolve_tools(
        self,
        entity_id: str,
        tooling_config: dict[str, Any],
        model_id: str,
        mcp_config: dict[str, Any] | None = None,
    ) -> tuple[list[ResolvedToolConfig], dict[str, Any]]:
        """解析工具配置"""
        if not tooling_config.get("enabled", True):
            return [], {"enabled": False}

        tools = []
        mcp_skipped_servers: list[str] = []

        # Builtin tools
        allowed_sources = tooling_config.get("allowed_tool_sources", ["builtin"])
        if "builtin" in allowed_sources:
            for tool in self._builtin_tools:
                if is_tool_allowed(tool, tooling_config):
                    tools.append(
                        ResolvedToolConfig(
                            source="builtin",
                            name=tool.get("name", ""),
                            display_name=tool.get("name", ""),
                            description=tool.get("description", ""),
                            parameters=tool.get("parameters", {}),
                            risk_level=tool.get("risk_level", "low"),
                            cacheable=tool.get("cacheable", False),
                            side_effect=tool.get("side_effect", False),
                        )
                    )

        # MCP tools
        mcp_enabled = (mcp_config or {}).get("enabled", False)
        config_fail_closed = (mcp_config or {}).get("fail_closed", False)
        # Production environment forces fail_closed=True; config can only tighten, not relax
        effective_fail_closed = config_fail_closed or is_production_environment()
        if "mcp" in allowed_sources and self._mcp_repo and mcp_enabled:
            bindings = await self._mcp_repo.find_bindings_by_entity(entity_id)
            enabled_bindings = [b for b in bindings if b.get("enabled", False)]

            for binding in enabled_bindings:
                server_id = binding.get("server_id", "")
                caps = await self._mcp_repo.get_capabilities(server_id)

                if not caps:
                    if effective_fail_closed:
                        raise ValueError(
                            f"MCP_CAPABILITIES_NOT_DISCOVERED: server {server_id} has no discovered capabilities, "
                            f"and mcp.fail_closed=true"
                        )
                    mcp_skipped_servers.append(server_id)
                    continue

                # Parse tools from capabilities
                caps_tools = caps.get("tools", [])
                if isinstance(caps_tools, str):
                    try:
                        caps_tools = json.loads(caps_tools)
                    except JSON_EXCEPTIONS as exc:
                        logger.warning("mcap_caps_tools_json_parse_failed", exc_info=exc)
                        caps_tools = []

                binding = normalized_mcp_binding_tool_acl(binding)
                binding_allowed = binding.get("allowed_tools") or []
                binding_denied = binding.get("denied_tools") or []

                for tool_data in caps_tools:
                    tool_name = tool_data.get("name", "") if isinstance(tool_data, dict) else ""
                    if not tool_name:
                        continue

                    # Apply binding-level allowed/denied filters
                    if binding_allowed and tool_name not in binding_allowed:
                        continue
                    if tool_name in binding_denied:
                        continue

                    # Apply entity-level allowed/denied filters
                    description = tool_data.get("description", "") if isinstance(tool_data, dict) else ""
                    parameters = tool_data.get("inputSchema", {}) if isinstance(tool_data, dict) else {}
                    annotations = tool_data.get("annotations", {}) if isinstance(tool_data, dict) else {}
                    normalized = normalize_mcp_tool_annotations(annotations)
                    side_effect = normalized.side_effect
                    cacheable = normalized.cacheable

                    qualified_name = f"mcp.{server_id}.{tool_name}"

                    tools.append(
                        ResolvedToolConfig(
                            source="mcp",
                            name=qualified_name,
                            display_name=tool_name,
                            description=description,
                            parameters=parameters,
                            risk_level="medium" if side_effect else "low",
                            cacheable=cacheable,
                            side_effect=side_effect,
                        )
                    )

        tool_policy = {
            "enabled": True,
            "max_rounds": tooling_config.get("max_rounds", 5),
            "allow_parallel": tooling_config.get("allow_parallel", False),
            "write_action_policy": tooling_config.get("write_action_policy", "proposal_only"),
            "allowed_tool_sources": allowed_sources,
        }

        if mcp_skipped_servers:
            tool_policy["mcp_skipped_servers"] = mcp_skipped_servers

        return tools, tool_policy

    async def _resolve_mcp(
        self,
        entity_id: str,
        mcp_config: dict[str, Any],
    ) -> ResolvedMcpConfig:
        """解析 MCP 配置"""
        if not mcp_config.get("enabled", False):
            return ResolvedMcpConfig(
                enabled=False,
                server_count=0,
                tool_count=0,
                resource_count=0,
            )

        binding_ids = mcp_config.get("binding_ids", [])
        server_count = 0
        tool_count = 0
        resource_count = 0

        if self._mcp_repo:
            bindings = await self._mcp_repo.find_bindings_by_entity(entity_id)
            for binding in bindings:
                if binding.get("enabled", False):
                    server_count += 1
                    server_id = binding.get("server_id", "")
                    caps = await self._mcp_repo.get_capabilities(server_id)
                    if caps:
                        caps_tools = caps.get("tools", [])
                        if isinstance(caps_tools, str):
                            try:
                                caps_tools = json.loads(caps_tools)
                            except JSON_EXCEPTIONS as exc:
                                logger.warning("mcp_count_caps_tools_json_parse_failed", exc_info=exc)
                                caps_tools = []
                        caps_resources = caps.get("resources", [])
                        if isinstance(caps_resources, str):
                            try:
                                caps_resources = json.loads(caps_resources)
                            except JSON_EXCEPTIONS as exc:
                                logger.warning("mcp_count_caps_resources_json_parse_failed", exc_info=exc)
                                caps_resources = []

                        # Apply binding-level filters for count
                        binding = normalized_mcp_binding_tool_acl(binding)
                        binding_allowed = binding.get("allowed_tools") or []
                        binding_denied = binding.get("denied_tools") or []

                        for t in caps_tools:
                            t_name = t.get("name", "") if isinstance(t, dict) else ""
                            if not t_name:
                                continue
                            if binding_allowed and t_name not in binding_allowed:
                                continue
                            if t_name in binding_denied:
                                continue
                            tool_count += 1

                        resource_count += len(caps_resources)
                    else:
                        # No capabilities; skip tool count for this server
                        pass

        return ResolvedMcpConfig(
            enabled=True,
            server_count=server_count,
            tool_count=tool_count,
            resource_count=resource_count,
            binding_ids=binding_ids,
        )

    async def _resolve_skills(
        self,
        entity_id: str,
        skills_config: dict[str, Any],
    ) -> ResolvedSkillConfig:
        """解析 Skills 配置"""
        if not skills_config.get("enabled", False):
            return ResolvedSkillConfig(
                enabled=False,
                skill_count=0,
            )

        binding_ids = skills_config.get("binding_ids", [])
        fail_closed = skills_config.get("fail_closed", False)
        skill_count = 0
        bindings: list[ResolvedSkillBinding] = []

        if self._skill_repo:
            raw_bindings = await self._skill_repo.find_bindings_by_entity(entity_id)
            for b in raw_bindings:
                if b.get("enabled", False):
                    skill_count += 1
                    bindings.append(
                        ResolvedSkillBinding(
                            binding_id=b.get("binding_id", ""),
                            skill_slug=b.get("skill_slug", ""),
                            version=b.get("version", "0.0.0"),
                            activation_policy=b.get("activation_policy", "task_routed"),
                            max_instruction_tokens=b.get("max_instruction_tokens", 3000),
                            priority=b.get("priority", 100),
                            allowed_task_types=b.get("allowed_task_types", []),
                        )
                    )

        return ResolvedSkillConfig(
            enabled=True,
            skill_count=skill_count,
            binding_ids=binding_ids,
            fail_closed=fail_closed,
            bindings=bindings,
        )

    def _resolve_subagents(
        self,
        subagents_config: dict[str, Any],
    ) -> ResolvedSubagentsConfig:
        """解析子代理配置"""
        return ResolvedSubagentsConfig(
            enabled=subagents_config.get("enabled", False),
            allowed_entity_ids=subagents_config.get("allowed_entity_ids", []),
            max_depth=subagents_config.get("max_depth", 1),
            max_concurrency=subagents_config.get("max_concurrency", 2),
            inherit_parent_context=subagents_config.get("inherit_parent_context", True),
        )

    def _resolve_context_policy(
        self,
        context_config: dict[str, Any],
    ) -> ResolvedContextPolicy:
        """解析上下文策略"""
        return ResolvedContextPolicy(
            strategy=context_config.get("strategy", "hybrid"),
            max_context_tokens=context_config.get("max_context_tokens", 64000),
            compression_threshold_tokens=context_config.get("compression_threshold_tokens", 48000),
            preserve_recent_messages=context_config.get("preserve_recent_messages", 12),
            summary_model=context_config.get("summary_model"),
            summary_max_tokens=context_config.get("summary_max_tokens", 1200),
            persist_summaries=context_config.get("persist_summaries", True),
        )

    def _resolve_cache_policy(
        self,
        cache_config: dict[str, Any],
    ) -> ResolvedCachePolicy:
        """解析缓存策略"""
        provider_cache = cache_config.get("provider_prompt_cache", {})
        context_cache = cache_config.get("context_cache", {})
        tool_cache = cache_config.get("tool_result_cache", {})
        mcp_cache = cache_config.get("mcp_resource_cache", {})

        return ResolvedCachePolicy(
            enabled=cache_config.get("enabled", True),
            provider_prompt_cache_enabled=provider_cache.get("enabled", False),
            provider_prompt_cache_retention=provider_cache.get("retention"),
            provider_prompt_cache_namespace=provider_cache.get("key_namespace"),
            context_cache_enabled=bool(context_cache.get("backend")),
            context_cache_ttl=context_cache.get("ttl_seconds", 86400),
            tool_result_cache_enabled=tool_cache.get("enabled", False),
            tool_result_cache_ttl=tool_cache.get("ttl_seconds", 60),
            tool_result_cacheable_tools=tool_cache.get("cacheable_tools", []),
            mcp_resource_cache_enabled=mcp_cache.get("enabled", False),
            mcp_resource_cache_ttl=mcp_cache.get("ttl_seconds", 300),
        )

    def _resolve_security(
        self,
        security_config: dict[str, Any],
    ) -> dict[str, Any]:
        """解析安全配置

        在透传原有 security 字段的基础上，确保 runtime 输入闸门所需的
        allowed_input_mime_types / max_input_bytes 始终存在（缺则补默认值），
        默认值与 SecurityConfig 保持一致。
        """
        security = dict(security_config or {})
        security.setdefault(
            "allowed_input_mime_types",
            ["text/plain", "image/png", "image/jpeg", "audio/wav"],
        )
        security.setdefault("max_input_bytes", 26214400)
        return security


def generate_prompt_cache_key(
    namespace: str,
    entity_id: str,
    api_format: str,
    model_id: str,
    system_prompt_hash: str,
    tool_schema_hash: str,
    skill_hash: str | None = None,
    output_schema_hash: str | None = None,
) -> str:
    """生成 prompt cache key

    包含稳定的静态前缀，排除用户输入、conversation_id、run_id 等动态字段。
    """
    parts = [
        namespace,
        entity_id,
        api_format,
        model_id,
        system_prompt_hash,
        tool_schema_hash,
    ]
    if skill_hash:
        parts.append(skill_hash)
    if output_schema_hash:
        parts.append(output_schema_hash)

    key_input = "|".join(parts)
    return hashlib.sha256(key_input.encode()).hexdigest()[:32]


__all__ = [
    "CapabilityResolver",
    "ResolvedAiRuntimeConfig",
    "ResolvedCachePolicy",
    "ResolvedContextPolicy",
    "ResolvedMcpConfig",
    "ResolvedModelConfig",
    "ResolvedSkillBinding",
    "ResolvedSkillConfig",
    "ResolvedSubagentsConfig",
    "ResolvedToolConfig",
    "generate_prompt_cache_key",
]
