use std::collections::BTreeMap;
use std::sync::OnceLock;

use std::collections::BTreeSet;

use fms_domain::error::DomainError;

pub(super) fn validate_tool_names(names: &[String], known_tools: &BTreeSet<String>) -> Result<(), DomainError> {
    let invalid = names
        .iter()
        .filter(|value| !known_tools.contains(*value))
        .cloned()
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        return Ok(());
    }
    Err(DomainError::ValidationError(format!(
        "包含未注册工具: {}",
        invalid.join(", ")
    )))
}

pub(super) fn normalize_string_list(values: &[serde_json::Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            deduped.push(value);
        }
    }
    deduped
}

pub(super) fn infer_provider(base_url: &str) -> &'static str {
    let host = base_url.to_lowercase();
    if host.contains("openai") {
        "OpenAI"
    } else if host.contains("deepseek") {
        "DeepSeek"
    } else if host.contains("anthropic") {
        "Anthropic"
    } else if host.contains("glm") || host.contains("zhipu") {
        "Zhipu"
    } else if host.contains("qwen") || host.contains("dashscope") || host.contains("aliyun") {
        "Alibaba"
    } else {
        "Unknown"
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) struct ModelCatalogItem {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) provider: &'static str,
}

pub(super) fn available_models() -> Vec<ModelCatalogItem> {
    vec![
        ModelCatalogItem {
            id: "gpt-4",
            name: "GPT-4",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "gpt-4-turbo",
            name: "GPT-4 Turbo",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "gpt-3.5-turbo",
            name: "GPT-3.5 Turbo",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "gpt-4o",
            name: "GPT-4o",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "gpt-4o-mini",
            name: "GPT-4o Mini",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "deepseek-chat",
            name: "DeepSeek Chat",
            provider: "DeepSeek",
        },
        ModelCatalogItem {
            id: "deepseek-reasoner",
            name: "DeepSeek Reasoner",
            provider: "DeepSeek",
        },
        ModelCatalogItem {
            id: "claude-3-opus",
            name: "Claude 3 Opus",
            provider: "Anthropic",
        },
        ModelCatalogItem {
            id: "claude-3-sonnet",
            name: "Claude 3 Sonnet",
            provider: "Anthropic",
        },
        ModelCatalogItem {
            id: "qwen-max",
            name: "Qwen Max",
            provider: "Alibaba",
        },
        ModelCatalogItem {
            id: "glm-4",
            name: "GLM-4",
            provider: "Zhipu",
        },
        ModelCatalogItem {
            id: "whisper-1",
            name: "Whisper ASR",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "tts-1",
            name: "OpenAI TTS",
            provider: "OpenAI",
        },
        ModelCatalogItem {
            id: "tts-1-hd",
            name: "OpenAI TTS HD",
            provider: "OpenAI",
        },
    ]
}

pub(super) struct ToolCatalogItem {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) category: &'static str,
    pub(super) operation_level: &'static str,
    pub(super) side_effect: bool,
    pub(super) parameters: serde_json::Value,
    pub(super) required_params: Vec<&'static str>,
}

pub(super) fn tool_catalog() -> Vec<ToolCatalogItem> {
    vec![
        ToolCatalogItem {
            name: "search_flights_advanced",
            description: "查询航班列表与基础过滤条件",
            category: "flight",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({"flight_no":{"type":"string"},"status":{"type":"string"}}),
            required_params: vec![],
        },
        ToolCatalogItem {
            name: "get_flight_detail",
            description: "读取单个航班详情",
            category: "flight",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({"flight_id":{"type":"string"}}),
            required_params: vec!["flight_id"],
        },
        ToolCatalogItem {
            name: "list_todos",
            description: "查询待办事项",
            category: "todo",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({"status":{"type":"string"}}),
            required_params: vec![],
        },
        ToolCatalogItem {
            name: "get_anomaly_list",
            description: "查询异常监控列表",
            category: "anomaly",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({"severity":{"type":"string"}}),
            required_params: vec![],
        },
        ToolCatalogItem {
            name: "generate_business_case",
            description: "生成业务案例工作流",
            category: "business_case",
            operation_level: "l1_workspace_write",
            side_effect: true,
            parameters: serde_json::json!({"template_code":{"type":"string"}}),
            required_params: vec!["template_code"],
        },
        ToolCatalogItem {
            name: "query_dispatch_orders",
            description: "查询派工单与协同状态",
            category: "dispatch_query",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({"status":{"type":"string"}}),
            required_params: vec![],
        },
        ToolCatalogItem {
            name: "sql_query_readonly",
            description: "执行只读 SQL 查询",
            category: "query",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({"sql":{"type":"string"}}),
            required_params: vec!["sql"],
        },
        ToolCatalogItem {
            name: "transcribe_audio",
            description: "将上传音频转写为文本",
            category: "media",
            operation_level: "l0_read",
            side_effect: false,
            parameters: serde_json::json!({
                "file": {"type": "binary"},
                "language": {"type": "string"},
                "prompt": {"type": "string"}
            }),
            required_params: vec!["file"],
        },
        ToolCatalogItem {
            name: "synthesize_speech",
            description: "将文本合成为语音音频",
            category: "media",
            operation_level: "l1_workspace_write",
            side_effect: true,
            parameters: serde_json::json!({
                "text": {"type": "string"},
                "voice": {"type": "string"},
                "response_format": {"type": "string"}
            }),
            required_params: vec!["text"],
        },
    ]
}

pub(super) fn tool_categories_map() -> &'static BTreeMap<&'static str, &'static str> {
    static TOOL_CATEGORIES: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    TOOL_CATEGORIES.get_or_init(|| {
        BTreeMap::from([
            ("flight", "航班查询"),
            ("flight_event", "航班事件"),
            ("todo", "待办事项"),
            ("system", "系统工具"),
            ("custom", "自定义工具"),
            ("business_case", "业务案例"),
            ("report", "报表工具"),
            ("advisor", "辅助决策"),
            ("anomaly", "异常监控"),
            ("query", "自然语言查询"),
            ("team", "班组管理"),
            ("equipment", "设备管理"),
            ("stand", "机位管理"),
            ("dispatch_query", "派工查询"),
            ("media", "语音与媒体"),
        ])
    })
}

pub(super) fn registry_executor_type_name(category: &str) -> String {
    format!(
        "{}Executor",
        category
            .replace('_', " ")
            .split_whitespace()
            .map(title_case)
            .collect::<String>()
    )
}
