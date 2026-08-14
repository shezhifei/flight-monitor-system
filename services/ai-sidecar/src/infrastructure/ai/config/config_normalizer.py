"""AI entity document: one stored shape, lift inbound aliases at the boundary."""

from copy import deepcopy
from typing import Any

_REVISION_KEYS = ("_config_revision", "config_revision", "configRevision")

_CONNECTION_KEYS = (
    "base_url",
    "api_key",
    "api_format",
    "timeout",
    "max_retries",
    "retry_delay",
)

_TOOLING_ALIAS_KEYS = ("allowed_tool_categories", "allowed_tools", "denied_tools")

_DOCUMENT_ALIASES = (
    *_CONNECTION_KEYS,
    *_TOOLING_ALIAS_KEYS,
    "default_model",
    "provider",
    "type",
    "asr_model",
    "tts_model",
    "tts_voice",
    "realtime_audio_enabled",
    "prompt_cache",
)

_DEFAULT_PROVIDER: dict[str, Any] = {
    "type": "openai_compatible",
    "base_url": "https://api.openai.com/v1",
    "api_key": "",
    "api_format": "chat_completions",
    "timeout": 30.0,
    "max_retries": 3,
    "retry_delay": 0.5,
}


def default_entity_document() -> dict[str, Any]:
    """Current entity document. Callers get an isolated copy."""
    return {
        "config_version": 2,
        "providers": {"default": dict(_DEFAULT_PROVIDER)},
        "model_routing": {"default": "gpt-4o", "chat": "gpt-4o"},
        "models": {},
        "temperature": 0.7,
        "max_tokens": 2000,
        "top_p": 0.95,
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "cost_per_1k_input": 0.0015,
        "cost_per_1k_output": 0.002,
        "context_window": 128000,
        "tools": {
            "timeout": 30,
            "max_retries": 3,
            "retry_delay": 1.0,
            "auto_execute": True,
        },
        "monitoring": {
            "metrics_enabled": True,
            "trace_enabled": False,
            "log_prompts": False,
            "mask_sensitive": True,
        },
        "media": {
            "asr": {"model": "whisper-1", "language": None, "response_format": "json"},
            "tts": {"model": "tts-1", "voice": "alloy", "response_format": "mp3", "speed": 1.0},
            "realtime": {
                "enabled": False,
                "provider": None,
                "asr_streaming_model": None,
                "tts_streaming_model": None,
                "input_sample_rate_hz": 16000,
                "output_sample_rate_hz": 24000,
                "chunk_ms": 40,
                "latency_budget_ms": 800,
                "vad_enabled": True,
                "barge_in_enabled": True,
                "max_session_seconds": 300,
                "max_frame_bytes": 65536,
            },
        },
        "endpoints": {"chat": None, "vision": None, "asr": None, "tts": None},
        "tooling": {
            "enabled": True,
            "max_rounds": 5,
            "allow_parallel": False,
            "allowed_tool_sources": ["builtin"],
            # Read-only categories the default entity grants out of the box
            # (query catalog, flight adapter, anomaly read tools); task
            # templates narrow further per task_type (Task A4).
            "allowed_tool_categories": [
                "flight",
                "flight_event",
                "query",
                "anomaly",
                "todo",
                "business_case",
            ],
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
        "cache_policy": {
            "enabled": True,
            "provider_prompt_cache": {
                "enabled": False,
                "retention": None,
                "key_namespace": "flight_monitor",
            },
        },
        "security": {"mask_sensitive": True, "log_prompts": False},
        "todo_agent_graph_enabled": False,
        "todo_agent_graph_runtime_enabled": False,
        "graph_runtime_enabled": False,
        "system_prompt": "你是一个航班监控系统的AI助手，可以帮助用户查询航班信息、管理航班事件和待办事项。",
        "task_template": None,
    }


def normalize_config(raw_config: dict[str, Any]) -> dict[str, Any]:
    """Return the current entity document. Inbound aliases are lifted then dropped."""
    revisions = {key: raw_config[key] for key in _REVISION_KEYS if key in raw_config}
    config = _deep_merge(default_entity_document(), deepcopy(raw_config))
    for key in _REVISION_KEYS:
        config.pop(key, None)
    _lift_aliases(config)
    _strip_aliases(config)
    config["config_version"] = 2
    config.update(revisions)
    return config


def connection_settings(config: dict[str, Any]) -> dict[str, Any]:
    providers = config.get("providers")
    if not isinstance(providers, dict):
        return {}
    default = providers.get("default")
    return default if isinstance(default, dict) else {}


def default_model_id(config: dict[str, Any], fallback: str = "gpt-4o") -> str:
    routing = config.get("model_routing")
    if isinstance(routing, dict):
        model = routing.get("default")
        if isinstance(model, str) and model.strip():
            return model.strip()
    return fallback


def tooling_policy(config: dict[str, Any]) -> dict[str, Any]:
    tooling = config.get("tooling")
    return tooling if isinstance(tooling, dict) else {}


def document_has_api_key(config: dict[str, Any]) -> bool:
    api_key = connection_settings(config).get("api_key")
    return bool(isinstance(api_key, str) and api_key.strip())


def get_config_version(config: dict[str, Any]) -> int:
    try:
        return int(config.get("config_version") or 2)
    except (TypeError, ValueError):
        return 2


def _lift_aliases(config: dict[str, Any]) -> None:
    _lift_connection(config)
    _lift_model_routing(config)
    _lift_tooling(config)
    _lift_media(config)
    _lift_prompt_cache(config)


def _lift_connection(config: dict[str, Any]) -> None:
    providers = config.get("providers")
    if not isinstance(providers, dict):
        providers = {}
        config["providers"] = providers

    single = config.get("provider")
    if isinstance(single, dict) and "default" not in providers:
        providers["default"] = deepcopy(single)

    default = providers.get("default")
    if not isinstance(default, dict):
        default = dict(_DEFAULT_PROVIDER)
        providers["default"] = default

    if isinstance(single, dict):
        for key, value in single.items():
            if not _is_blank(value):
                default[key] = value

    if isinstance(config.get("type"), str) and config["type"].strip():
        default["type"] = config["type"]

    for key in _CONNECTION_KEYS:
        value = config.get(key)
        if not _is_blank(value):
            default[key] = value


def _lift_model_routing(config: dict[str, Any]) -> None:
    routing = config.get("model_routing")
    if not isinstance(routing, dict):
        routing = {}
        config["model_routing"] = routing
    model = config.get("default_model")
    if isinstance(model, str) and model.strip():
        routing["default"] = model.strip()


def _lift_tooling(config: dict[str, Any]) -> None:
    tooling = config.get("tooling")
    if not isinstance(tooling, dict):
        tooling = {}
        config["tooling"] = tooling
    for key in _TOOLING_ALIAS_KEYS:
        if key in config:
            tooling[key] = config[key]


def _lift_media(config: dict[str, Any]) -> None:
    media = config.get("media")
    if not isinstance(media, dict):
        media = {}
        config["media"] = media
    routing = config.get("model_routing")
    if not isinstance(routing, dict):
        routing = {}
        config["model_routing"] = routing

    asr = media.get("asr")
    if not isinstance(asr, dict):
        asr = {}
        media["asr"] = asr
    tts = media.get("tts")
    if not isinstance(tts, dict):
        tts = {}
        media["tts"] = tts
    realtime = media.get("realtime")
    if not isinstance(realtime, dict):
        realtime = {}
        media["realtime"] = realtime

    asr_model = config.get("asr_model")
    if isinstance(asr_model, str) and asr_model.strip():
        asr["model"] = asr_model.strip()
        routing["audio_transcription"] = asr_model.strip()

    tts_model = config.get("tts_model")
    if isinstance(tts_model, str) and tts_model.strip():
        tts["model"] = tts_model.strip()
        routing["audio_speech"] = tts_model.strip()

    tts_voice = config.get("tts_voice")
    if isinstance(tts_voice, str) and tts_voice.strip():
        tts["voice"] = tts_voice.strip()

    if "realtime_audio_enabled" in config:
        realtime["enabled"] = bool(config.get("realtime_audio_enabled"))


def _lift_prompt_cache(config: dict[str, Any]) -> None:
    prompt_cache = config.get("prompt_cache")
    if not isinstance(prompt_cache, dict):
        return
    cache_policy = config.get("cache_policy")
    if not isinstance(cache_policy, dict):
        cache_policy = {}
        config["cache_policy"] = cache_policy
    provider_cache = cache_policy.get("provider_prompt_cache")
    if not isinstance(provider_cache, dict):
        provider_cache = {}
        cache_policy["provider_prompt_cache"] = provider_cache
    if "enabled" in prompt_cache:
        provider_cache["enabled"] = bool(prompt_cache.get("enabled"))
    if prompt_cache.get("retention") is not None:
        provider_cache["retention"] = prompt_cache.get("retention")
    if prompt_cache.get("namespace"):
        provider_cache["key_namespace"] = prompt_cache.get("namespace")


def _strip_aliases(config: dict[str, Any]) -> None:
    for key in _DOCUMENT_ALIASES:
        config.pop(key, None)


def _is_blank(value: Any) -> bool:
    if value is None:
        return True
    if isinstance(value, str):
        return not value.strip()
    return False


def _deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    result = deepcopy(base)
    for key, value in override.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = _deep_merge(result[key], value)
        else:
            result[key] = value
    return result


__all__ = [
    "connection_settings",
    "default_entity_document",
    "default_model_id",
    "document_has_api_key",
    "get_config_version",
    "normalize_config",
    "tooling_policy",
]
