"""AI模块配置定义"""
from typing import Dict, Any, Optional, List
from pydantic import BaseModel, Field, model_validator, ConfigDict
from enum import Enum


class AIProvider(str, Enum):
    """AI提供商枚举"""
    OPENAI = "openai"
    AZURE_OPENAI = "azure_openai"
    ANTHROPIC = "anthropic"
    LOCAL = "local"


class ContextStrategy(str, Enum):
    """上下文管理策略"""
    SLIDING_WINDOW = "sliding_window"
    SUMMARY_COMPRESSION = "summary_compression"
    HYBRID = "hybrid"


class ConversationStatus(str, Enum):
    """对话状态枚举"""
    ACTIVE = "active"
    PAUSED = "paused"
    COMPLETED = "completed"
    EXPIRED = "expired"
    ARCHIVED = "archived"


class ModelConfig(BaseModel):
    """模型配置"""
    name: str = Field(..., description="模型名称")
    max_tokens: int = Field(default=128000, description="最大令牌数")
    cost_per_1k_input: float = Field(default=0.005, description="每1K输入令牌成本")
    cost_per_1k_output: float = Field(default=0.015, description="每1K输出令牌成本")
    context_window: int = Field(default=128000, description="上下文窗口大小")
    supports_function_calling: bool = Field(default=True, description="是否支持函数调用")
    supports_streaming: bool = Field(default=True, description="是否支持流式输出")
    supports_vision: bool = Field(default=False, description="是否支持视觉输入")
    supports_audio_speech: bool = Field(default=False, description="是否支持文本转语音")
    supports_audio_transcription: bool = Field(default=False, description="是否支持语音转文本")


class ProviderConfig(BaseModel):
    """提供商配置"""
    api_key: str = Field(..., description="API密钥")
    base_url: Optional[str] = Field(default=None, description="基础URL")
    api_version: Optional[str] = Field(default=None, description="API版本")
    default_model: str = Field(default="gpt-4o", description="默认模型")
    timeout: float = Field(default=30.0, description="超时时间(秒)")
    max_retries: int = Field(default=3, description="最大重试次数")
    retry_delay: float = Field(default=1.0, description="重试延迟(秒)")
    rate_limit_requests: int = Field(default=60, description="每分钟请求限制")
    rate_limit_window: int = Field(default=60, description="速率限制窗口(秒)")


class ContextConfig(BaseModel):
    """上下文配置"""
    max_tokens: int = Field(default=8000, description="最大上下文令牌数")
    strategy: ContextStrategy = Field(default=ContextStrategy.SLIDING_WINDOW, description="上下文管理策略")
    persistence: str = Field(default="memory", description="持久化方式: memory/redis/database")
    redis_url: Optional[str] = Field(default=None, description="Redis URL")
    compression_threshold: int = Field(default=4000, description="压缩阈值")
    summary_model: Optional[str] = Field(default=None, description="摘要模型")


class ConversationConfig(BaseModel):
    """对话配置"""
    ttl_seconds: int = Field(default=86400, description="对话生存时间(秒)")
    cleanup_interval: int = Field(default=3600, description="清理间隔(秒)")
    max_messages: int = Field(default=1000, description="最大消息数")
    max_concurrent: int = Field(default=10, description="最大并发对话数")
    auto_archive: bool = Field(default=True, description="自动归档过期对话")


class ClientConfig(BaseModel):
    """客户端配置"""
    timeout: float = Field(default=30.0, description="请求超时时间")
    max_retries: int = Field(default=3, description="最大重试次数")
    retry_backoff_factor: float = Field(default=2.0, description="重试退避因子")
    max_retry_delay: float = Field(default=60.0, description="最大重试延迟")
    connection_pool_size: int = Field(default=10, description="连接池大小")
    keep_alive: bool = Field(default=True, description="保持连接")


class SecurityConfig(BaseModel):
    """安全配置"""
    log_prompts: bool = Field(default=False, description="是否记录提示")
    mask_sensitive_data: bool = Field(default=True, description="是否屏蔽敏感数据")
    allowed_domains: List[str] = Field(default_factory=list, description="允许的域名")
    max_prompt_length: int = Field(default=10000, description="最大提示长度")
    rate_limit_enabled: bool = Field(default=True, description="是否启用速率限制")
    content_filter_enabled: bool = Field(default=True, description="是否启用内容过滤")


class MonitoringConfig(BaseModel):
    """监控配置"""
    enable_metrics: bool = Field(default=True, description="是否启用指标")
    metrics_prefix: str = Field(default="ai", description="指标前缀")
    log_level: str = Field(default="INFO", description="日志级别")
    trace_requests: bool = Field(default=False, description="是否跟踪请求")
    performance_monitoring: bool = Field(default=True, description="是否启用性能监控")


class CacheConfig(BaseModel):
    """缓存配置"""
    enabled: bool = Field(default=True, description="是否启用缓存")
    ttl_seconds: int = Field(default=300, description="缓存生存时间")
    max_size: int = Field(default=1000, description="最大缓存大小")
    redis_url: Optional[str] = Field(default=None, description="Redis URL")
    compression: bool = Field(default=False, description="是否启用压缩")


class AIPMode(str, Enum):
    """AIP 运行模式"""
    AIP_ONLY = "aip_only"
    LEGACY_ONLY = "legacy_only"
    DUAL = "dual"


class AIPConfig(BaseModel):
    """AIP (Artificial Intelligence Platform) 配置"""
    enabled: bool = Field(default=False, description="是否启用 AIP 模式")
    mode: AIPMode = Field(default=AIPMode.DUAL, description="运行模式: aip_only/legacy_only/dual")
    legacy_fallback: bool = Field(default=True, description="AIP 执行失败时是否回退到 Legacy")
    migration_progress: float = Field(default=0.0, ge=0.0, le=1.0, description="迁移进度 0.0-1.0")

    action_approval_default: bool = Field(default=True, description="Action 是否默认需要审批")
    max_action_depth: int = Field(default=3, description="最大 Action 嵌套深度")
    max_context_objects: int = Field(default=10, description="最大上下文对象数量")

    ontology_enabled: bool = Field(default=True, description="是否启用 Ontology")
    object_acl_enabled: bool = Field(default=True, description="是否启用对象级 ACL")
    hitl_enabled: bool = Field(default=True, description="是否启用 Human-in-the-Loop")

    cache_enabled: bool = Field(default=True, description="是否启用 AIP 缓存")
    cache_ttl_seconds: int = Field(default=300, description="缓存生存时间")

    auto_register_handlers: bool = Field(default=True, description="是否自动注册 Action Handlers")
    auto_load_legacy_tools: bool = Field(default=True, description="是否自动加载 Legacy 工具")

    metrics_enabled: bool = Field(default=True, description="是否启用 AIP 指标")
    trace_mode: bool = Field(default=False, description="是否启用跟踪模式")

    class Config:
        use_enum_values = True



class EndpointConfig(BaseModel):
    """端点配置 - 用于能力导向的配置"""
    base_url: str = Field(..., description="API 基础 URL")
    api_key: str = Field(..., description="API 密钥")
    model: str = Field(..., description="使用的模型名称")
    timeout: float = Field(default=30.0, description="超时时间")
    max_retries: int = Field(default=3, description="重试次数")


class AIConfig(BaseModel):
    """AI模块主配置"""
    enabled: bool = Field(default=True, description="是否启用AI模块")
    
    # AIP 配置 (v7.0 新增)
    aip: AIPConfig = Field(default_factory=AIPConfig, description="AIP 模式配置")
    
    # 传统提供商配置 (保留兼容)
    providers: Dict[str, ProviderConfig] = Field(default_factory=dict, description="提供商配置")
    
    # 能力端点配置 (v6.0 新增)
    chat_endpoint: Optional[EndpointConfig] = Field(default=None, description="聊天/推理端点 (DeepSeek)")
    vision_endpoint: Optional[EndpointConfig] = Field(default=None, description="视觉理解端点 (GLM-4V)")
    asr_endpoint: Optional[EndpointConfig] = Field(default=None, description="语音转写端点 (SenseVoice)")
    tts_endpoint: Optional[EndpointConfig] = Field(default=None, description="语音合成端点 (CosyVoice)")
    models: Dict[str, ModelConfig] = Field(default_factory=dict, description="模型配置")
    context: ContextConfig = Field(default_factory=ContextConfig, description="上下文配置")
    conversation: ConversationConfig = Field(default_factory=ConversationConfig, description="对话配置")
    client: ClientConfig = Field(default_factory=ClientConfig, description="客户端配置")
    security: SecurityConfig = Field(default_factory=SecurityConfig, description="安全配置")
    monitoring: MonitoringConfig = Field(default_factory=MonitoringConfig, description="监控配置")
    cache: CacheConfig = Field(default_factory=CacheConfig, description="缓存配置")

    model_config = ConfigDict(
        use_enum_values=True,
        validate_assignment=True,
    )

    @model_validator(mode='after')
    def validate_and_set_defaults(self):
        """验证并设置默认值"""
        if not self.models:
            self.models = {
                "gpt-4o": ModelConfig(
                    name="gpt-4o",
                    max_tokens=128000,
                    cost_per_1k_input=0.005,
                    cost_per_1k_output=0.015,
                    context_window=128000
                ),
                "gpt-3.5-turbo": ModelConfig(
                    name="gpt-3.5-turbo",
                    max_tokens=16384,
                    cost_per_1k_input=0.0015,
                    cost_per_1k_output=0.002,
                    context_window=16384
                )
            }

        return self


# 默认配置实例
default_ai_config = AIConfig()

# 配置工厂函数
def create_ai_config(
    api_key: Optional[str] = None,
    provider: AIProvider = AIProvider.OPENAI,
    model: str = "gpt-4o",
    **kwargs
) -> AIConfig:
    """
    创建AI配置

    Args:
        api_key: API密钥
        provider: AI提供商
        model: 默认模型
        **kwargs: 其他配置参数

    Returns:
        AIConfig: AI配置实例
    """
    providers = {}
    if api_key:
        provider_key = provider.value
        providers[provider_key] = ProviderConfig(
            api_key=api_key,
            default_model=model,
            **kwargs
        )

    config_data = {
        "providers": providers,
        **kwargs
    }

    return AIConfig(**config_data)


# 配置验证函数
def validate_ai_config(config: AIConfig) -> bool:
    """
    验证AI配置

    Args:
        config: AI配置实例

    Returns:
        bool: 验证是否通过
    """
    try:
        config.model_dump()
        return True
    except Exception:
        return False


# 配置合并函数
def merge_ai_configs(base_config: AIConfig, override_config: Dict[str, Any]) -> AIConfig:
    """
    合并AI配置

    Args:
        base_config: 基础配置
        override_config: 覆盖配置

    Returns:
        AIConfig: 合并后的配置
    """
    base_dict = base_config.model_dump()
    
    def deep_merge(base: Dict[str, Any], override: Dict[str, Any]) -> Dict[str, Any]:
        """深度合并两个字典"""
        result = base.copy()
        for key, value in override.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
                result[key] = deep_merge(result[key], value)
            else:
                result[key] = value
        return result
    
    merged_dict = deep_merge(base_dict, override_config)
    return AIConfig(**merged_dict)
