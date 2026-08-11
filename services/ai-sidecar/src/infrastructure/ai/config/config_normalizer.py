"""AI 配置标准化器 - 旧配置到 v2 的迁移"""

import logging
from copy import deepcopy
from typing import Any

logger = logging.getLogger(__name__)


def normalize_config_to_v2(raw_config: dict[str, Any]) -> dict[str, Any]:
    """
    将旧配置标准化为 v2 格式。

    如果配置已有 config_version=2，直接返回。
    否则执行 v1 -> v2 的迁移。

    Args:
        raw_config: 原始配置字典

    Returns:
        v2 格式的配置字典
    """
    config = deepcopy(raw_config)

    # 如果已经是 v2，直接返回
    if config.get("config_version") == 2:
        return _ensure_v2_defaults(config)

    logger.info("Migrating config from v1 to v2 format")

    # 提取旧字段
    api_key = config.get("api_key", "")
    base_url = config.get("base_url", "https://api.openai.com/v1")
    default_model = config.get("default_model", "gpt-3.5-turbo")
    api_format = config.get("api_format", "chat_completions")
    timeout = config.get("timeout", 30.0)
    max_retries = config.get("max_retries", 3)
    retry_delay = config.get("retry_delay", 0.5)
    context_window = config.get("context_window", 128000)
    cost_per_1k_input = config.get("cost_per_1k_input", 0.0015)
    cost_per_1k_output = config.get("cost_per_1k_output", 0.002)
    system_prompt = config.get("system_prompt", "你是一个航班监控系统的 AI 助手。")
    task_template = config.get("task_template")

    # 旧配置只有单一 provider，迁移后放入 providers['default']，
    # 并保留顶层 provider 字段作为其镜像（向后兼容）。
    default_provider = {
        "type": "openai_compatible",
        "base_url": base_url,
        "api_key": api_key,
        "api_format": api_format,
        "timeout": timeout,
        "max_retries": max_retries,
        "retry_delay": retry_delay,
    }

    # 构建 v2 结构
    v2_config = {
        "config_version": 2,
        "provider": deepcopy(default_provider),
        "providers": {"default": deepcopy(default_provider)},
        "model_routing": {
            "default": default_model,
            "chat": default_model,
            "summary": _infer_summary_model(default_model),
            "vision": None,
            "audio_transcription": None,
            "audio_speech": None,
            "embedding": None,
        },
        "models": _build_models_v2(
            config, default_model, api_format, context_window, cost_per_1k_input, cost_per_1k_output
        ),
        "tooling": _build_tooling_v2(config),
        "mcp": {
            "enabled": False,
            "servers": [],
            "tool_name_prefix": "mcp",
            "discovery_cache_ttl_seconds": 300,
            "fail_closed": False,
        },
        "skills": {
            "enabled": False,
            "allowlist": [],
            "bindings": [],
            "fail_closed": False,
        },
        "subagents": {
            "enabled": False,
            "mode": "entity_handoff",
            "allowed_entity_ids": [],
            "max_depth": 1,
            "max_concurrency": 2,
            "inherit_parent_context": True,
            "require_tool_calling_capability": True,
            "handoff_prompt": None,
        },
        "context_policy": _build_context_policy_v2(config),
        "cache_policy": _build_cache_policy_v2(config),
        "security": _build_security_v2(config),
        "system_prompt": system_prompt,
        "task_template": task_template,
    }

    return v2_config


def _ensure_v2_defaults(config: dict[str, Any]) -> dict[str, Any]:
    """确保 v2 配置包含所有必需的默认值"""
    default_provider = {
        "type": "openai_compatible",
        "base_url": "https://api.openai.com/v1",
        "api_key": "",
        "api_format": "chat_completions",
        "timeout": 30.0,
        "max_retries": 3,
        "retry_delay": 0.5,
    }
    defaults = {
        "config_version": 2,
        "provider": deepcopy(default_provider),
        "providers": {"default": deepcopy(default_provider)},
        "model_routing": {"default": "gpt-4o"},
        "models": {},
        "tooling": {
            "enabled": True,
            "max_rounds": 5,
            "allow_parallel": False,
            "allowed_tool_sources": ["builtin"],
            "allowed_tool_categories": [],
            "allowed_tools": None,
            "denied_tools": [],
            "write_action_policy": "proposal_only",
        },
        "mcp": {"enabled": False, "servers": []},
        "skills": {"enabled": False, "allowlist": [], "bindings": []},
        "subagents": {"enabled": False, "allowed_entity_ids": []},
        "context_policy": {
            "strategy": "hybrid",
            "max_context_tokens": 64000,
            "compression_threshold_tokens": 48000,
            "preserve_recent_messages": 12,
        },
        "cache_policy": {"enabled": True},
        "security": {"mask_sensitive": True, "log_prompts": False},
        "system_prompt": "你是一个航班监控系统的 AI 助手。",
        "task_template": None,
    }

    # 深度合并，保留已有值
    merged = _deep_merge(defaults, config)

    # 既有 v2 配置可能只有顶层 provider 而无 providers：
    # 若 config 未显式提供 providers，则 providers['default'] 必须镜像实际的
    # 顶层 provider（而非默认占位 provider），保证单-provider 配置零行为变化。
    if "providers" not in config:
        merged["providers"] = {"default": deepcopy(merged["provider"])}
    elif "default" not in merged.get("providers", {}):
        merged["providers"]["default"] = deepcopy(merged["provider"])

    # 确保每个 model 都带有 provider_ref（默认 'default'）。
    for model_config in merged.get("models", {}).values():
        if isinstance(model_config, dict):
            model_config.setdefault("provider_ref", "default")

    return merged


def _deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    """深度合并两个字典"""
    result = base.copy()
    for key, value in override.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = _deep_merge(result[key], value)
        else:
            result[key] = value
    return result


def _infer_summary_model(default_model: str) -> str:
    """根据默认模型推断摘要模型"""
    # 常见的 mini 模型映射
    mini_mappings = {
        "gpt-4o": "gpt-4o-mini",
        "gpt-4.1": "gpt-4.1-mini",
        "gpt-4": "gpt-4o-mini",
        "deepseek-chat": "deepseek-chat",
    }
    return mini_mappings.get(default_model, default_model)


def _build_models_v2(
    config: dict[str, Any],
    default_model: str,
    api_format: str,
    context_window: int,
    cost_per_1k_input: float,
    cost_per_1k_output: float,
) -> dict[str, Any]:
    """构建 v2 models 配置"""
    models = {}

    # 从旧配置推断模型能力
    supports_vision = config.get("supports_vision", False)
    supports_audio = config.get("supports_audio_transcription", False)

    # 构建默认模型配置
    input_modalities = ["text"]
    if supports_vision:
        input_modalities.append("image")
    if supports_audio:
        input_modalities.append("audio")

    models[default_model] = {
        "provider_model": default_model,
        "provider_ref": "default",
        "api_format": api_format,
        "context_window": context_window,
        "max_output_tokens": config.get("max_tokens", 4096),
        "modalities": {
            "input": input_modalities,
            "output": ["text"],
        },
        "capabilities": {
            "tool_calling": config.get("supports_function_calling", True),
            "parallel_tool_calls": False,
            "streaming": config.get("supports_streaming", True),
            "structured_output": False,
            "prompt_cache": config.get("prompt_cache", {}).get("enabled", False),
        },
        "cost": {
            "currency": "USD",
            "input_per_1k": cost_per_1k_input,
            "output_per_1k": cost_per_1k_output,
            "cached_input_per_1k": 0.0,
        },
    }

    # 合并旧的 models 配置
    old_models = config.get("models", {})
    for model_name, model_config in old_models.items():
        if isinstance(model_config, dict):
            models[model_name] = _deep_merge(
                models.get(default_model, {}),
                {
                    "provider_model": model_config.get("name", model_name),
                    "context_window": model_config.get("context_window", context_window),
                    "max_output_tokens": model_config.get("max_tokens", 4096),
                },
            )

    return models


def _build_tooling_v2(config: dict[str, Any]) -> dict[str, Any]:
    """构建 v2 tooling 配置"""
    allowed_categories = config.get("allowed_tool_categories", [])
    allowed_tools = config.get("allowed_tools")
    denied_tools = config.get("denied_tools", [])

    return {
        "enabled": True,
        "max_rounds": 5,
        "allow_parallel": False,
        "allowed_tool_sources": ["builtin"],
        "allowed_tool_categories": allowed_categories,
        "allowed_tools": allowed_tools,
        "denied_tools": denied_tools,
        "write_action_policy": "proposal_only",
    }


def _build_context_policy_v2(config: dict[str, Any]) -> dict[str, Any]:
    """构建 v2 context_policy 配置"""
    context = config.get("context", {})

    return {
        "strategy": context.get("strategy", "hybrid"),
        "max_context_tokens": context.get("max_tokens", 64000),
        "compression_threshold_tokens": context.get("compression_threshold", 48000),
        "preserve_recent_messages": 12,
        "summary_model": context.get("summary_model"),
        "summary_max_tokens": 1200,
        "persist_summaries": True,
    }


def _build_cache_policy_v2(config: dict[str, Any]) -> dict[str, Any]:
    """构建 v2 cache_policy 配置"""
    prompt_cache = config.get("prompt_cache", {})
    cache = config.get("cache", {})

    return {
        "enabled": cache.get("enabled", True),
        "provider_prompt_cache": {
            "enabled": prompt_cache.get("enabled", False),
            "retention": prompt_cache.get("retention", "24h") or "24h",
            "key_namespace": prompt_cache.get("namespace", "flight_monitor"),
        },
        "context_cache": {
            "backend": "redis",
            "ttl_seconds": 86400,
        },
        "tool_result_cache": {
            "enabled": False,
            "ttl_seconds": 60,
            "cacheable_tools": [],
        },
        "mcp_resource_cache": {
            "enabled": False,
            "ttl_seconds": 300,
        },
    }


def _build_security_v2(config: dict[str, Any]) -> dict[str, Any]:
    """构建 v2 security 配置"""
    monitoring = config.get("monitoring", {})

    return {
        "mask_sensitive": monitoring.get("mask_sensitive", True),
        "log_prompts": monitoring.get("log_prompts", False),
        "max_input_bytes": 26214400,
        "allowed_input_mime_types": ["text/plain", "image/png", "image/jpeg", "audio/wav"],
    }


def get_config_version(config: dict[str, Any]) -> int:
    """获取配置版本号"""
    return config.get("config_version", 1)


def needs_migration(config: dict[str, Any]) -> bool:
    """检查配置是否需要迁移"""
    return get_config_version(config) < 2


__all__ = [
    "get_config_version",
    "needs_migration",
    "normalize_config_to_v2",
]
