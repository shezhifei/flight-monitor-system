"""AI配置模式定义"""

from typing import Any

from .base_schema import BaseSchema


class AIConfigSchema(BaseSchema):
    """AI配置模式"""

    def get_default_schema(self) -> dict[str, Any]:
        """获取AI配置的默认模式定义"""
        return {
            "type": "object",
            "properties": {
                "enabled": {"type": "boolean", "description": "是否启用AI模块", "default": True},
                "providers": {
                    "type": "object",
                    "description": "AI提供商配置",
                    "patternProperties": {
                        ".*": {
                            "type": "object",
                            "properties": {
                                "api_key": {"type": "string", "description": "API密钥", "minLength": 1},
                                "base_url": {"type": ["string", "null"], "description": "基础URL", "format": "uri"},
                                "api_version": {"type": ["string", "null"], "description": "API版本"},
                                "default_model": {"type": "string", "description": "默认模型", "default": "gpt-4o"},
                                "timeout": {
                                    "type": "number",
                                    "description": "超时时间(秒)",
                                    "minimum": 1,
                                    "maximum": 300,
                                    "default": 30.0,
                                },
                                "max_retries": {
                                    "type": "integer",
                                    "description": "最大重试次数",
                                    "minimum": 0,
                                    "maximum": 10,
                                    "default": 3,
                                },
                                "retry_delay": {
                                    "type": "number",
                                    "description": "重试延迟(秒)",
                                    "minimum": 0.1,
                                    "maximum": 60.0,
                                    "default": 1.0,
                                },
                                "rate_limit_requests": {
                                    "type": "integer",
                                    "description": "每分钟请求限制",
                                    "minimum": 1,
                                    "maximum": 1000,
                                    "default": 60,
                                },
                                "rate_limit_window": {
                                    "type": "integer",
                                    "description": "速率限制窗口(秒)",
                                    "minimum": 1,
                                    "maximum": 3600,
                                    "default": 60,
                                },
                            },
                            "required": ["api_key", "default_model"],
                        }
                    },
                    "minProperties": 1,
                },
                "models": {
                    "type": "object",
                    "description": "模型配置",
                    "patternProperties": {
                        ".*": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string", "description": "模型名称", "minLength": 1},
                                "max_tokens": {
                                    "type": "integer",
                                    "description": "最大令牌数",
                                    "minimum": 1,
                                    "maximum": 1000000,
                                    "default": 128000,
                                },
                                "cost_per_1k_input": {
                                    "type": "number",
                                    "description": "每1K输入令牌成本",
                                    "minimum": 0,
                                    "default": 0.005,
                                },
                                "cost_per_1k_output": {
                                    "type": "number",
                                    "description": "每1K输出令牌成本",
                                    "minimum": 0,
                                    "default": 0.015,
                                },
                                "context_window": {
                                    "type": "integer",
                                    "description": "上下文窗口大小",
                                    "minimum": 1,
                                    "maximum": 1000000,
                                    "default": 128000,
                                },
                                "supports_function_calling": {
                                    "type": "boolean",
                                    "description": "是否支持函数调用",
                                    "default": True,
                                },
                                "supports_streaming": {
                                    "type": "boolean",
                                    "description": "是否支持流式输出",
                                    "default": True,
                                },
                            },
                            "required": ["name"],
                        }
                    },
                },
                "storage": {
                    "type": "object",
                    "description": "存储配置",
                    "properties": {
                        "context_backend": {
                            "type": "string",
                            "description": "上下文存储后端",
                            "enum": ["memory", "redis"],
                            "default": "memory",
                        },
                        "context_redis_url": {
                            "type": ["string", "null"],
                            "description": "Context Redis URL",
                            "format": "uri",
                        },
                        "context_ttl_seconds": {
                            "type": "integer",
                            "description": "上下文 TTL (秒)",
                            "minimum": 60,
                            "maximum": 2592000,
                            "default": 604800,
                        },
                        "conversation_backend": {
                            "type": "string",
                            "description": "对话存储后端",
                            "enum": ["memory", "postgres"],
                            "default": "memory",
                        },
                    },
                },
                "context": {
                    "type": "object",
                    "description": "上下文配置",
                    "properties": {
                        "max_tokens": {
                            "type": "integer",
                            "description": "最大上下文令牌数",
                            "minimum": 100,
                            "maximum": 100000,
                            "default": 8000,
                        },
                        "strategy": {
                            "type": "string",
                            "description": "上下文管理策略",
                            "enum": ["sliding_window", "summary_compression", "hybrid"],
                            "default": "sliding_window",
                        },
                        "compression_threshold": {
                            "type": "integer",
                            "description": "压缩阈值",
                            "minimum": 100,
                            "maximum": 50000,
                            "default": 4000,
                        },
                        "summary_model": {"type": ["string", "null"], "description": "摘要模型"},
                    },
                },
                "conversation": {
                    "type": "object",
                    "description": "对话配置",
                    "properties": {
                        "ttl_seconds": {
                            "type": "integer",
                            "description": "对话生存时间(秒)",
                            "minimum": 60,
                            "maximum": 2592000,  # 30天
                            "default": 86400,
                        },
                        "cleanup_interval": {
                            "type": "integer",
                            "description": "清理间隔(秒)",
                            "minimum": 60,
                            "maximum": 86400,
                            "default": 3600,
                        },
                        "max_messages": {
                            "type": "integer",
                            "description": "最大消息数",
                            "minimum": 1,
                            "maximum": 10000,
                            "default": 1000,
                        },
                        "max_concurrent": {
                            "type": "integer",
                            "description": "最大并发对话数",
                            "minimum": 1,
                            "maximum": 1000,
                            "default": 10,
                        },
                        "auto_archive": {"type": "boolean", "description": "自动归档过期对话", "default": True},
                    },
                },
                "client": {
                    "type": "object",
                    "description": "客户端配置",
                    "properties": {
                        "timeout": {
                            "type": "number",
                            "description": "请求超时时间",
                            "minimum": 1,
                            "maximum": 300,
                            "default": 30.0,
                        },
                        "max_retries": {
                            "type": "integer",
                            "description": "最大重试次数",
                            "minimum": 0,
                            "maximum": 10,
                            "default": 3,
                        },
                        "retry_backoff_factor": {
                            "type": "number",
                            "description": "重试退避因子",
                            "minimum": 1,
                            "maximum": 5,
                            "default": 2.0,
                        },
                        "max_retry_delay": {
                            "type": "number",
                            "description": "最大重试延迟",
                            "minimum": 1,
                            "maximum": 300,
                            "default": 60.0,
                        },
                        "connection_pool_size": {
                            "type": "integer",
                            "description": "连接池大小",
                            "minimum": 1,
                            "maximum": 100,
                            "default": 10,
                        },
                        "keep_alive": {"type": "boolean", "description": "保持连接", "default": True},
                    },
                },
                "security": {
                    "type": "object",
                    "description": "安全配置",
                    "properties": {
                        "log_prompts": {"type": "boolean", "description": "是否记录提示", "default": False},
                        "mask_sensitive_data": {"type": "boolean", "description": "是否屏蔽敏感数据", "default": True},
                        "allowed_domains": {
                            "type": "array",
                            "description": "允许的域名",
                            "items": {"type": "string", "format": "hostname"},
                            "default": [],
                        },
                        "max_prompt_length": {
                            "type": "integer",
                            "description": "最大提示长度",
                            "minimum": 1,
                            "maximum": 100000,
                            "default": 10000,
                        },
                        "rate_limit_enabled": {"type": "boolean", "description": "是否启用速率限制", "default": True},
                        "content_filter_enabled": {
                            "type": "boolean",
                            "description": "是否启用内容过滤",
                            "default": True,
                        },
                    },
                },
                "monitoring": {
                    "type": "object",
                    "description": "监控配置",
                    "properties": {
                        "enable_metrics": {"type": "boolean", "description": "是否启用指标", "default": True},
                        "metrics_prefix": {"type": "string", "description": "指标前缀", "default": "ai"},
                        "log_level": {
                            "type": "string",
                            "description": "日志级别",
                            "enum": ["DEBUG", "INFO", "WARNING", "ERROR"],
                            "default": "INFO",
                        },
                        "trace_requests": {"type": "boolean", "description": "是否跟踪请求", "default": False},
                        "performance_monitoring": {
                            "type": "boolean",
                            "description": "是否启用性能监控",
                            "default": True,
                        },
                    },
                },
                "cache": {
                    "type": "object",
                    "description": "缓存配置",
                    "properties": {
                        "enabled": {"type": "boolean", "description": "是否启用缓存", "default": True},
                        "ttl_seconds": {
                            "type": "integer",
                            "description": "缓存生存时间",
                            "minimum": 1,
                            "maximum": 86400,
                            "default": 300,
                        },
                        "max_size": {
                            "type": "integer",
                            "description": "最大缓存大小",
                            "minimum": 1,
                            "maximum": 10000,
                            "default": 1000,
                        },
                        "redis_url": {"type": ["string", "null"], "description": "Redis URL", "format": "uri"},
                        "compression": {"type": "boolean", "description": "是否启用压缩", "default": False},
                    },
                },
            },
            "required": ["enabled", "providers"],
            "additionalProperties": False,
        }

    def validate(self, config_data: dict[str, Any]) -> bool:
        """
        验证AI配置数据

        Args:
            config_data: 配置数据

        Returns:
            验证是否通过
        """
        # 这里会调用外部验证器进行实际的JSON Schema验证
        # 当前返回True，实际验证将在ConfigValidator中完成
        return True
