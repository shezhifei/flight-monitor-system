from src.infrastructure.ai.config.config_normalizer import (
    connection_settings,
    default_model_id,
    document_has_api_key,
    normalize_config,
    tooling_policy,
)


def test_normalize_lifts_flat_aliases_and_drops_them():
    document = normalize_config(
        {
            "base_url": "https://api.example.com/v1",
            "api_key": "sk-test",
            "default_model": "gpt-4.1",
            "allowed_tool_categories": ["flight"],
            "asr_model": "whisper-large-v3",
        }
    )

    assert document["providers"]["default"]["base_url"] == "https://api.example.com/v1"
    assert document["providers"]["default"]["api_key"] == "sk-test"
    assert document["model_routing"]["default"] == "gpt-4.1"
    assert document["tooling"]["allowed_tool_categories"] == ["flight"]
    assert document["media"]["asr"]["model"] == "whisper-large-v3"
    assert document["config_version"] == 2
    for alias in (
        "base_url",
        "api_key",
        "default_model",
        "allowed_tool_categories",
        "asr_model",
    ):
        assert alias not in document


def test_normalize_keeps_current_document_shape():
    document = normalize_config(
        {
            "config_version": 2,
            "providers": {
                "default": {
                    "type": "openai_compatible",
                    "base_url": "https://gw.example/v1",
                    "api_key": "sk-nested",
                    "api_format": "chat_completions",
                    "timeout": 15,
                    "max_retries": 1,
                    "retry_delay": 0.2,
                }
            },
            "model_routing": {"default": "gpt-4o"},
            "tooling": {"allowed_tool_categories": ["todo"]},
        }
    )

    assert document["providers"]["default"]["base_url"] == "https://gw.example/v1"
    assert document["model_routing"]["default"] == "gpt-4o"
    assert document["tooling"]["allowed_tool_categories"] == ["todo"]
    assert "base_url" not in document
    assert "default_model" not in document


def test_runtime_accessors_read_the_document():
    document = normalize_config({"api_key": "sk-live", "default_model": "gpt-4o-mini"})

    assert connection_settings(document)["api_key"] == "sk-live"
    assert default_model_id(document) == "gpt-4o-mini"
    assert document_has_api_key(document) is True
    assert isinstance(tooling_policy(document).get("allowed_tool_categories"), list)


def test_from_document_projects_runtime_client_config():
    from src.infrastructure.ai.ai_entity import AIEntityConfig

    runtime = AIEntityConfig.from_document(
        {
            "providers": {"default": {"base_url": "https://gw.example/v1", "api_key": "sk-x"}},
            "model_routing": {"default": "gpt-4o"},
            "tooling": {"allowed_tool_categories": ["flight"], "denied_tools": ["x"]},
        }
    )

    assert runtime.api_key == "sk-x"
    assert runtime.base_url == "https://gw.example/v1"
    assert runtime.default_model == "gpt-4o"
    assert runtime.allowed_tool_categories == ["flight"]
    assert runtime.denied_tools == ["x"]
