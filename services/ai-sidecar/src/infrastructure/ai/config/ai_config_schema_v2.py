"""AI 配置 v2 Schema - Capability-driven configuration"""

from enum import StrEnum
from typing import Any

from pydantic import BaseModel, ConfigDict, Field, model_validator


class ModalityType(StrEnum):
    """输入/输出模态类型"""

    TEXT = "text"
    IMAGE = "image"
    AUDIO = "audio"
    FILE = "file"


class ApiFormat(StrEnum):
    """API 格式"""

    CHAT_COMPLETIONS = "chat_completions"
    RESPONSES = "responses"


class ModelCapabilities(BaseModel):
    """模型能力声明"""

    tool_calling: bool = Field(default=True, description="是否支持工具调用")
    parallel_tool_calls: bool = Field(default=False, description="是否支持并行工具调用")
    streaming: bool = Field(default=True, description="是否支持流式输出")
    structured_output: bool = Field(default=False, description="是否支持结构化输出")
    prompt_cache: bool = Field(default=False, description="是否支持 provider prompt cache")


class ModelModalities(BaseModel):
    """模型模态声明"""

    input: list[ModalityType] = Field(default=[ModalityType.TEXT], description="支持的输入模态")
    output: list[ModalityType] = Field(default=[ModalityType.TEXT], description="支持的输出模态")


class ModelCost(BaseModel):
    """模型成本元数据"""

    currency: str = Field(default="USD", description="货币单位")
    input_per_1k: float = Field(default=0.0, description="每 1K 输入 token 成本")
    output_per_1k: float = Field(default=0.0, description="每 1K 输出 token 成本")
    cached_input_per_1k: float = Field(default=0.0, description="每 1K 缓存输入 token 成本")


class ModelConfigV2(BaseModel):
    """模型配置 v2"""

    provider_model: str = Field(..., description="provider 端的模型 ID")
    provider_ref: str = Field(default="default", description="指向 providers 的键，默认 'default'")
    api_format: ApiFormat = Field(default=ApiFormat.CHAT_COMPLETIONS, description="API 格式")
    context_window: int = Field(default=128000, description="上下文窗口大小")
    max_output_tokens: int = Field(default=4096, description="最大输出 token 数")
    modalities: ModelModalities = Field(default_factory=ModelModalities, description="模态声明")
    capabilities: ModelCapabilities = Field(default_factory=ModelCapabilities, description="能力声明")
    cost: ModelCost = Field(default_factory=ModelCost, description="成本元数据")


class ProviderConfigV2(BaseModel):
    """Provider 配置 v2"""

    type: str = Field(default="openai_compatible", description="provider 类型")
    base_url: str = Field(..., description="API 基础 URL")
    api_key: str = Field(default="", description="API 密钥（加密存储）")
    api_format: ApiFormat = Field(default=ApiFormat.CHAT_COMPLETIONS, description="默认 API 格式")
    timeout: float = Field(default=30.0, description="超时时间（秒）")
    max_retries: int = Field(default=3, description="最大重试次数")
    retry_delay: float = Field(default=0.5, description="重试延迟（秒）")


class ModelRouting(BaseModel):
    """模型路由 - 按用途选择模型"""

    default: str = Field(default="gpt-4o", description="默认模型")
    chat: str | None = Field(default=None, description="聊天模型")
    summary: str | None = Field(default=None, description="摘要模型")
    vision: str | None = Field(default=None, description="视觉模型")
    audio_transcription: str | None = Field(default=None, description="语音转写模型")
    audio_speech: str | None = Field(default=None, description="语音合成模型")
    embedding: str | None = Field(default=None, description="嵌入模型")


class WriteActionPolicy(StrEnum):
    """写动作策略"""

    PROPOSAL_ONLY = "proposal_only"  # 只生成提案
    DIRECT_EXECUTE = "direct_execute"  # 直接执行（未来）


class ToolingConfig(BaseModel):
    """工具配置"""

    enabled: bool = Field(default=True, description="是否启用工具")
    max_rounds: int = Field(default=5, description="最大工具调用轮数")
    allow_parallel: bool = Field(default=False, description="是否允许并行工具调用")
    allowed_tool_sources: list[str] = Field(default=["builtin"], description="允许的工具来源")
    allowed_tool_categories: list[str] = Field(default_factory=list, description="允许的工具类别")
    allowed_tools: list[str] | None = Field(default=None, description="允许的工具列表（null=全部）")
    denied_tools: list[str] = Field(default_factory=list, description="拒绝的工具列表")
    write_action_policy: WriteActionPolicy = Field(default=WriteActionPolicy.PROPOSAL_ONLY, description="写动作策略")


class McpServerRef(BaseModel):
    """MCP 服务器引用"""

    server_id: str = Field(..., description="服务器 ID")
    enabled: bool = Field(default=True, description="是否启用")


class McpConfig(BaseModel):
    """MCP 配置"""

    enabled: bool = Field(default=False, description="是否启用 MCP")
    servers: list[McpServerRef] = Field(default_factory=list, description="MCP 服务器列表")
    tool_name_prefix: str = Field(default="mcp", description="工具名前缀")
    discovery_cache_ttl_seconds: int = Field(default=300, description="discovery 缓存 TTL")
    fail_closed: bool = Field(default=False, description="失败时是否关闭")


class SkillBindingRef(BaseModel):
    """Skill 绑定引用"""

    skill_slug: str = Field(..., description="skill slug")
    version: str | None = Field(default=None, description="skill 版本")
    enabled: bool = Field(default=True, description="是否启用")


class SkillsConfig(BaseModel):
    """Agent Skills 配置"""

    enabled: bool = Field(default=False, description="是否启用 skills")
    allowlist: list[str] = Field(default_factory=list, description="允许的 skill slug 列表")
    bindings: list[SkillBindingRef] = Field(default_factory=list, description="skill 绑定列表")
    fail_closed: bool = Field(default=False, description="失败时是否关闭")


class SubagentsMode(StrEnum):
    """子代理模式"""

    ENTITY_HANDOFF = "entity_handoff"  # 实体委派


class SubagentsConfig(BaseModel):
    """子代理配置"""

    enabled: bool = Field(default=False, description="是否启用子代理")
    mode: SubagentsMode = Field(default=SubagentsMode.ENTITY_HANDOFF, description="子代理模式")
    allowed_entity_ids: list[str] = Field(default_factory=list, description="允许委派的实体 ID")
    max_depth: int = Field(default=1, description="最大委派深度")
    max_concurrency: int = Field(default=2, description="最大并发数")
    inherit_parent_context: bool = Field(default=True, description="是否继承父上下文")
    require_tool_calling_capability: bool = Field(default=True, description="是否要求工具调用能力")
    handoff_prompt: str | None = Field(default=None, description="委派提示词")


class ContextStrategy(StrEnum):
    """上下文策略"""

    SLIDING_WINDOW = "sliding_window"
    SUMMARY_COMPRESSION = "summary_compression"
    HYBRID = "hybrid"


class ContextPolicyConfig(BaseModel):
    """上下文策略配置"""

    strategy: ContextStrategy = Field(default=ContextStrategy.HYBRID, description="压缩策略")
    max_context_tokens: int = Field(default=64000, description="最大上下文 token 数")
    compression_threshold_tokens: int = Field(default=48000, description="压缩阈值 token 数")
    preserve_recent_messages: int = Field(default=12, description="保留最近消息数")
    summary_model: str | None = Field(default=None, description="摘要模型")
    summary_max_tokens: int = Field(default=1200, description="摘要最大 token 数")
    persist_summaries: bool = Field(default=True, description="是否持久化摘要")


class ProviderPromptCacheConfig(BaseModel):
    """Provider prompt cache 配置"""

    enabled: bool = Field(default=False, description="是否启用")
    retention: str = Field(default="24h", description="保留时间")
    key_namespace: str = Field(default="flight_monitor", description="key 命名空间")


class ContextCacheConfig(BaseModel):
    """上下文缓存配置"""

    backend: str = Field(default="redis", description="缓存后端")
    ttl_seconds: int = Field(default=86400, description="TTL 秒数")


class ToolResultCacheConfig(BaseModel):
    """工具结果缓存配置"""

    enabled: bool = Field(default=False, description="是否启用")
    ttl_seconds: int = Field(default=60, description="TTL 秒数")
    cacheable_tools: list[str] = Field(default_factory=list, description="可缓存的工具列表")


class McpResourceCacheConfig(BaseModel):
    """MCP 资源缓存配置"""

    enabled: bool = Field(default=False, description="是否启用")
    ttl_seconds: int = Field(default=300, description="TTL 秒数")


class CachePolicyConfig(BaseModel):
    """缓存策略配置"""

    enabled: bool = Field(default=True, description="是否启用缓存")
    provider_prompt_cache: ProviderPromptCacheConfig = Field(default_factory=ProviderPromptCacheConfig)
    context_cache: ContextCacheConfig = Field(default_factory=ContextCacheConfig)
    tool_result_cache: ToolResultCacheConfig = Field(default_factory=ToolResultCacheConfig)
    mcp_resource_cache: McpResourceCacheConfig = Field(default_factory=McpResourceCacheConfig)


class SecurityConfigV2(BaseModel):
    """安全配置 v2"""

    mask_sensitive: bool = Field(default=True, description="是否屏蔽敏感数据")
    log_prompts: bool = Field(default=False, description="是否记录提示词")
    max_input_bytes: int = Field(default=26214400, description="最大输入字节数 (25MB)")
    allowed_input_mime_types: list[str] = Field(
        default=["text/plain", "image/png", "image/jpeg", "audio/wav"], description="允许的输入 MIME 类型"
    )


class AiEntityConfigV2(BaseModel):
    """AI 实体配置 v2 - 顶层配置结构"""

    config_version: int = Field(default=2, description="配置版本")
    provider: ProviderConfigV2 = Field(default_factory=lambda: ProviderConfigV2(base_url="https://api.openai.com/v1"))
    providers: dict[str, ProviderConfigV2] = Field(
        default_factory=dict,
        description="多 provider 字典（键如 'default'/'asr'/'tts'）；'default' 为 provider 字段的镜像",
    )
    model_routing: ModelRouting = Field(default_factory=ModelRouting)
    models: dict[str, ModelConfigV2] = Field(default_factory=dict, description="模型配置字典")
    tooling: ToolingConfig = Field(default_factory=ToolingConfig)
    mcp: McpConfig = Field(default_factory=McpConfig)
    skills: SkillsConfig = Field(default_factory=SkillsConfig)
    subagents: SubagentsConfig = Field(default_factory=SubagentsConfig)
    context_policy: ContextPolicyConfig = Field(default_factory=ContextPolicyConfig)
    cache_policy: CachePolicyConfig = Field(default_factory=CachePolicyConfig)
    security: SecurityConfigV2 = Field(default_factory=SecurityConfigV2)
    system_prompt: str = Field(default="你是一个航班监控系统的 AI 助手。", description="系统提示词")
    task_template: str | None = Field(default=None, description="任务模板")

    model_config = ConfigDict(use_enum_values=True)

    @model_validator(mode="after")
    def _sync_default_provider(self) -> "AiEntityConfigV2":
        """保持 provider 字段与 providers['default'] 互为镜像。

        - providers 缺少 'default' 时，用顶层 provider 字段填充（向后兼容路径）。
        - providers 显式提供 'default' 时，以其为准并回写顶层 provider 字段，
          使两者始终一致。其它键（如 'asr'/'tts'）原样保留。
        """
        default_provider = self.providers.get("default")
        if default_provider is None:
            self.providers["default"] = self.provider
        else:
            self.provider = default_provider
        return self

    def resolve_provider(self, provider_ref: str = "default") -> ProviderConfigV2:
        """按 provider_ref 取 provider，缺失回退 'default'。"""
        provider = self.providers.get(provider_ref)
        if provider is not None:
            return provider
        return self.providers.get("default", self.provider)

    def resolve_provider_for_model(self, model_key: str) -> ProviderConfigV2:
        """按 models[model_key].provider_ref 取 provider，缺失回退 'default'。"""
        model = self.models.get(model_key)
        provider_ref = model.provider_ref if model is not None else "default"
        return self.resolve_provider(provider_ref)


# === MCP Server 定义（独立存储） ===


class McpTransport(StrEnum):
    """MCP 传输类型"""

    STDIO = "stdio"
    STREAMABLE_HTTP = "streamable_http"


class McpRiskPolicy(BaseModel):
    """MCP 风险策略"""

    network_access: bool = Field(default=False, description="是否允许网络访问")
    filesystem_access: str = Field(default="none", description="文件系统访问级别")
    max_response_bytes: int = Field(default=131072, description="最大响应字节数")


class McpCacheConfig(BaseModel):
    """MCP 缓存配置"""

    tools_ttl_seconds: int = Field(default=300, description="工具 TTL")
    resources_ttl_seconds: int = Field(default=300, description="资源 TTL")


class McpServerDefinition(BaseModel):
    """MCP 服务器定义"""

    id: str = Field(..., description="服务器 ID")
    enabled: bool = Field(default=True, description="是否启用")
    display_name: str = Field(..., description="显示名称")
    description: str | None = Field(default=None, description="描述")
    transport: McpTransport = Field(default=McpTransport.STDIO, description="传输类型")
    command_ref: str | None = Field(default=None, description="命令引用（映射到 allowlist）")
    endpoint_url: str | None = Field(default=None, description="HTTP 端点 URL")
    args: list[str] = Field(default_factory=list, description="命令参数")
    env_secret_refs: list[str] = Field(default_factory=list, description="环境变量 secret 引用")
    timeout_seconds: int = Field(default=10, description="超时秒数")
    startup_timeout_seconds: int = Field(default=5, description="启动超时秒数")
    max_concurrency: int = Field(default=4, description="最大并发数")
    cache: McpCacheConfig = Field(default_factory=McpCacheConfig)
    risk: McpRiskPolicy = Field(default_factory=McpRiskPolicy)


class McpServerCapabilitySnapshot(BaseModel):
    """MCP 服务器能力快照"""

    server_id: str = Field(..., description="服务器 ID")
    protocol_version: str | None = Field(default=None, description="协议版本")
    tools: list[dict[str, Any]] = Field(default_factory=list, description="工具列表")
    resources: list[dict[str, Any]] = Field(default_factory=list, description="资源列表")
    prompts: list[dict[str, Any]] = Field(default_factory=list, description="提示词列表")
    schema_hash: str = Field(..., description="schema hash")
    discovered_at: str = Field(..., description="发现时间")
    expires_at: str | None = Field(default=None, description="过期时间")


class McpEntityBinding(BaseModel):
    """MCP 实体绑定"""

    binding_id: str = Field(..., description="绑定 ID")
    entity_id: str = Field(..., description="实体 ID")
    server_id: str = Field(..., description="服务器 ID")
    enabled: bool = Field(default=False, description="是否启用")
    allowed_tools: list[str] = Field(default_factory=list, description="允许的工具")
    denied_tools: list[str] = Field(default_factory=list, description="拒绝的工具")
    allowed_resources: list[str] = Field(default_factory=list, description="允许的资源")
    allowed_prompts: list[str] = Field(default_factory=list, description="允许的提示词")
    tool_defaults: dict[str, Any] = Field(default_factory=dict, description="工具默认值")


# === Agent Skill 定义（独立存储） ===


class SkillActivationPolicy(StrEnum):
    """Skill 激活策略"""

    ALWAYS = "always"
    TASK_ROUTED = "task_routed"
    MANUAL = "manual"


class SkillRegistryEntry(BaseModel):
    """Skill 注册表条目"""

    skill_slug: str = Field(..., description="skill slug")
    version: str = Field(..., description="版本")
    name: str = Field(..., description="名称")
    description: str | None = Field(default=None, description="描述")
    source: str = Field(..., description="来源")
    canonical_path: str = Field(..., description="规范路径")
    entry_file: str = Field(default="SKILL.md", description="入口文件")
    frontmatter: dict[str, Any] = Field(default_factory=dict, description="frontmatter")
    content_hash: str = Field(..., description="内容 hash")
    status: str = Field(default="draft", description="状态")


class SkillEntityBinding(BaseModel):
    """Skill 实体绑定"""

    binding_id: str = Field(..., description="绑定 ID")
    entity_id: str = Field(..., description="实体 ID")
    skill_slug: str = Field(..., description="skill slug")
    version: str = Field(..., description="版本")
    enabled: bool = Field(default=False, description="是否启用")
    priority: int = Field(default=100, description="优先级")
    activation_policy: SkillActivationPolicy = Field(default=SkillActivationPolicy.TASK_ROUTED)
    allowed_task_types: list[str] = Field(default_factory=list, description="允许的任务类型")
    allowed_reference_paths: list[str] = Field(default_factory=list, description="允许的引用路径")
    allow_scripts: bool = Field(default=False, description="是否允许脚本")
    max_instruction_tokens: int = Field(default=3000, description="最大指令 token 数")
