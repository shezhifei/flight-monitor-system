"""
AI 基础设施模块

提供与外部 AI 服务交互的客户端和工具。

注意：为避免循环导入和重依赖问题，__init__ 只导出轻量符号。
重依赖模块（如 openai_client, context_manager 等）请直接从子模块导入。
"""

# 轻量级符号 - 安全立即导入
# 注意：ai_entity 也依赖 openai_client，因此不在这里导出

__all__ = [
    # 请直接从子模块导入所需类：
    # from src.infrastructure.ai.openai_client import OpenAIClient
    # from src.infrastructure.ai.service_identity import require_service_identity
]
