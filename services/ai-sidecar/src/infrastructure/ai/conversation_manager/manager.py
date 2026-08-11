"""
对话管理器 - 管理器实现

包含对话管理器抽象基类、内存实现、Redis 实现，以及默认管理器工厂函数。
"""

import asyncio
import json
import time
from abc import ABC, abstractmethod
from collections.abc import AsyncIterator
from datetime import datetime
from typing import Any

from src.infrastructure.common.exceptions import LLM_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

from ..context_manager import Context, ContextManager, ContextNotFoundError, ContextType
from ..llm_stream_runner import LLMStreamRunner
from ..openai_client import AiGateway, ChatCompletionResponse, Message, MessageRole
from ..token_counter import count_messages_tokens, get_model_context_window
from .models import (
    Conversation,
    ConversationMetadata,
    ConversationNotFoundError,
    ConversationStatus,
    _build_context_metadata,
)

logger = get_logger(__name__)

_DEFAULT_CONVERSATION_TTL_SECONDS = 86400
_DEFAULT_MAX_CONVERSATIONS = 2000
_DEFAULT_MAX_CONTEXT_MESSAGES = 1000
# LLM 摘要功能配置
_DEFAULT_LLM_SUMMARY_ENABLED = False  # 默认关闭，保持向后兼容
_DEFAULT_SUMMARY_WATERMARK_RATIO = 0.8  # 上下文窗口的 80% 时触发摘要


class ConversationManager(ABC):
    """
    对话管理器抽象基类

    定义对话会话管理的标准接口。
    """

    @abstractmethod
    async def create_conversation(
        self,
        title: str | None = None,
        user_id: str | None = None,
        user_name: str | None = None,
        user_info: dict[str, Any] | None = None,
        session_id: str | None = None,
        model: str = "gpt-3.5-turbo",
        temperature: float = 0.7,
        max_tokens: int = 0,
        system_prompt: str | None = None,
        tags: list[str] | None = None,
        custom_data: dict[str, Any] | None = None,
        parent_id: str | None = None,
        enable_llm_summary: bool | None = None,
    ) -> Conversation:
        """
        创建新对话

        Args:
            title: 对话标题
            user_id: 用户 ID
            user_name: 用户名
            user_info: 用户信息
            session_id: 会话 ID
            model: AI 模型
            temperature: 温度参数
            max_tokens: 最大令牌数
            system_prompt: 系统提示词
            tags: 标签列表
            custom_data: 自定义数据
            parent_id: 父对话 ID
            enable_llm_summary: 是否启用 LLM 摘要功能（覆盖全局默认）

        Returns:
            创建的对话对象
        """

    @abstractmethod
    async def get_conversation(self, conversation_id: str) -> Conversation:
        """
        获取对话

        Args:
            conversation_id: 对话 ID

        Returns:
            对话对象

        Raises:
            ConversationNotFoundError: 如果对话不存在
        """

    @abstractmethod
    async def update_conversation(
        self,
        conversation_id: str,
        title: str | None = None,
        status: ConversationStatus | None = None,
        user_info: dict[str, Any] | None = None,
        tags: list[str] | None = None,
        custom_data: dict[str, Any] | None = None,
        system_prompt: str | None = None,
        enable_llm_summary: bool | None = None,
    ) -> Conversation:
        """
        更新对话

        Args:
            conversation_id: 对话 ID
            title: 新标题
            status: 新状态
            user_info: 用户信息
            tags: 标签列表
            custom_data: 自定义数据
            system_prompt: 系统提示词
            enable_llm_summary: 是否启用 LLM 摘要功能

        Returns:
            更新后的对话对象
        """

    @abstractmethod
    async def merge_custom_data(
        self,
        conversation_id: str,
        patch: dict[str, Any],
    ) -> Conversation:
        """
        合并 custom_data（一级 key 粒度 dict 合并）。

        与 update_conversation(custom_data=...) 的整体替换不同，
        本方法只覆盖 patch 中出现的顶层 key，不丢失已有的其他 key。

        对于 value 为 dict 的 key，做浅层合并（dict.update）；
        其他类型直接覆盖。

        Args:
            conversation_id: 对话 ID
            patch: 要合并的 key-value 字典

        Returns:
            更新后的对话对象
        """

    @abstractmethod
    async def delete_conversation(self, conversation_id: str) -> bool:
        """
        删除对话（软删除，标记为已结束）

        Args:
            conversation_id: 对话 ID

        Returns:
            是否成功
        """

    @abstractmethod
    async def hard_delete_conversation(self, conversation_id: str) -> bool:
        """
        硬删除对话（从存储中完全移除）

        Args:
            conversation_id: 对话 ID

        Returns:
            是否成功
        """

    @abstractmethod
    async def add_message(
        self,
        conversation_id: str,
        role: MessageRole,
        content: str,
        name: str | None = None,
        tool_calls: list[dict] | None = None,
        tool_call_id: str | None = None,
        metadata: dict[str, Any] | None = None,
        update_activity: bool = True,
    ) -> Message:
        """
        向对话添加消息

        Args:
            conversation_id: 对话 ID
            role: 消息角色
            content: 消息内容
            name: 名称（可选）
            tool_calls: 工具调用列表（可选）
            tool_call_id: 工具调用 ID（可选）
            metadata: 消息元数据（可视化提示、结构化数据等）
            update_activity: 是否更新活动时间

        Returns:
            创建的消息对象
        """

    @abstractmethod
    async def get_messages(
        self, conversation_id: str, limit: int | None = None, offset: int = 0, role_filter: MessageRole | None = None
    ) -> list[Message]:
        """
        获取对话消息

        Args:
            conversation_id: 对话 ID
            limit: 返回消息数量限制
            offset: 偏移量
            role_filter: 按角色过滤

        Returns:
            消息列表
        """

    @abstractmethod
    async def get_conversation_context(self, conversation_id: str, max_tokens: int | None = None) -> list[Message]:
        """
        获取对话上下文（用于 AI 请求）

        Args:
            conversation_id: 对话 ID
            max_tokens: 最大令牌数限制（自动修剪）

        Returns:
            消息列表
        """

    @abstractmethod
    async def stream_chat_completion(
        self,
        conversation_id: str,
        user_message: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        **kwargs,
    ) -> AsyncIterator[dict]:
        """
        流式聊天补全（集成 OpenAI 客户端）

        Args:
            conversation_id: 对话 ID
            user_message: 用户消息
            temperature: 温度参数
            max_tokens: 最大令牌数
            **kwargs: 其他参数

        Yields:
            流式响应块
        """

    @abstractmethod
    async def chat_completion(
        self,
        conversation_id: str,
        user_message: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        **kwargs,
    ) -> ChatCompletionResponse:
        """
        聊天补全（集成 OpenAI 客户端）

        Args:
            conversation_id: 对话 ID
            user_message: 用户消息
            temperature: 温度参数
            max_tokens: 最大令牌数
            **kwargs: 其他参数

        Returns:
            AI 响应
        """

    @abstractmethod
    async def list_conversations(
        self,
        user_id: str | None = None,
        status: ConversationStatus | None = None,
        tag: str | None = None,
        start_time: datetime | None = None,
        end_time: datetime | None = None,
        limit: int | None = None,
        offset: int = 0,
    ) -> list[Conversation]:
        """
        列出对话（支持过滤和分页）

        Args:
            user_id: 用户 ID 过滤
            status: 状态过滤
            tag: 标签过滤
            start_time: 开始时间过滤
            end_time: 结束时间过滤
            limit: 返回数量限制
            offset: 偏移量

        Returns:
            对话列表
        """

    @abstractmethod
    async def search_conversations(
        self,
        query: str,
        user_id: str | None = None,
        status: ConversationStatus | None = None,
        search_fields: list[str] | None = None,
        limit: int | None = None,
    ) -> list[Conversation]:
        """
        搜索对话（基于内容和元数据）

        Args:
            query: 搜索查询
            user_id: 用户 ID 过滤
            status: 状态过滤
            search_fields: 搜索字段列表（title, content, tags, user_name 等）
            limit: 返回数量限制

        Returns:
            匹配的对话列表
        """

    @abstractmethod
    async def cleanup_expired_conversations(self, ttl_seconds: int = 86400, batch_size: int = 100) -> int:
        """
        清理过期对话

        Args:
            ttl_seconds: 生存时间（秒）
            batch_size: 批处理大小

        Returns:
            清理的对话数量
        """

    @abstractmethod
    async def get_conversation_stats(self, conversation_id: str) -> dict[str, Any]:
        """
        获取对话统计信息

        Args:
            conversation_id: 对话 ID

        Returns:
            统计信息字典
        """

    @abstractmethod
    async def archive_conversation(self, conversation_id: str) -> Conversation:
        """
        归档对话

        Args:
            conversation_id: 对话 ID

        Returns:
            归档后的对话对象
        """

    @abstractmethod
    async def branch_conversation(
        self, conversation_id: str, from_message_id: str | None = None, title: str | None = None
    ) -> Conversation:
        """
        创建对话分支

        Args:
            conversation_id: 源对话 ID
            from_message_id: 从此消息开始分支（可选，表示从该点开始）
            title: 新对话标题

        Returns:
            新的分支对话
        """


class MemoryConversationManager(ConversationManager):
    """
    基于内存的对话管理器实现

    将对话存储在内存字典中。适用于单实例、非持久化场景。
    """

    def __init__(
        self,
        context_manager: ContextManager | None = None,
        ai_client: AiGateway | None = None,
        default_model: str = "gpt-3.5-turbo",
        max_conversations: int = _DEFAULT_MAX_CONVERSATIONS,
        cleanup_ttl_seconds: int = _DEFAULT_CONVERSATION_TTL_SECONDS,
        max_context_messages: int = _DEFAULT_MAX_CONTEXT_MESSAGES,
        enable_llm_summary: bool = _DEFAULT_LLM_SUMMARY_ENABLED,
        summary_watermark_ratio: float = _DEFAULT_SUMMARY_WATERMARK_RATIO,
    ):
        self._conversations: dict[str, Conversation] = {}
        self._context_manager = context_manager
        self._ai_client = ai_client
        self._default_model = default_model
        self._max_conversations = max(1, int(max_conversations or _DEFAULT_MAX_CONVERSATIONS))
        self._cleanup_ttl_seconds = max(1, int(cleanup_ttl_seconds or _DEFAULT_CONVERSATION_TTL_SECONDS))
        self._max_context_messages = max(0, int(max_context_messages or 0))
        self._enable_llm_summary = enable_llm_summary
        self._summary_watermark_ratio = max(0.1, min(1.0, summary_watermark_ratio))
        self._global_lock = asyncio.Lock()
        self._conversation_locks: dict[str, asyncio.Lock] = {}
        # 兼容旧实现中可能直接访问 _lock 的调用。
        self._lock = self._global_lock
        logger.info(
            f"MemoryConversationManager 初始化完成，默认模型: {default_model}, LLM摘要: {'启用' if enable_llm_summary else '禁用'}"
        )

    async def _get_conversation_lock(self, conversation_id: str) -> asyncio.Lock:
        lock = self._conversation_locks.get(conversation_id)
        if lock is not None:
            return lock

        async with self._global_lock:
            lock = self._conversation_locks.get(conversation_id)
            if lock is not None:
                return lock
            if conversation_id not in self._conversations:
                return asyncio.Lock()
            lock = asyncio.Lock()
            self._conversation_locks[conversation_id] = lock
            return lock

    async def _prune_orphaned_conversation_locks(self) -> int:
        async with self._global_lock:
            orphaned_ids = [
                conversation_id
                for conversation_id in self._conversation_locks
                if conversation_id not in self._conversations
            ]
            for conversation_id in orphaned_ids:
                self._conversation_locks.pop(conversation_id, None)
            return len(orphaned_ids)

    async def _bounded_cleanup_candidates(
        self,
        ttl_seconds: int,
        batch_size: int,
    ) -> list[str]:
        now = time.time()
        async with self._global_lock:
            items = list(self._conversations.items())

        expired_ids = [
            conv_id
            for conv_id, conversation in items
            if now - float(conversation.metadata.last_activity_at) > ttl_seconds
        ]

        expired_set = set(expired_ids)
        remaining = [(conv_id, conversation) for conv_id, conversation in items if conv_id not in expired_set]
        overflow_ids: list[str] = []
        if self._max_conversations > 0 and len(remaining) > self._max_conversations:
            overflow = len(remaining) - self._max_conversations
            ranked = sorted(
                remaining,
                key=lambda item: (
                    0 if item[1].status == ConversationStatus.ENDED else 1,
                    float(item[1].metadata.last_activity_at),
                    float(item[1].metadata.created_at),
                ),
            )
            overflow_ids = [conv_id for conv_id, _conversation in ranked[:overflow]]

        candidates = list(dict.fromkeys(expired_ids + overflow_ids))
        if batch_size > 0:
            return candidates[:batch_size]
        return candidates

    async def _generate_llm_summary(self, messages: list[Message], model: str) -> str | None:
        """
        使用 LLM 生成对话摘要

        Args:
            messages: 需要摘要的消息列表
            model: 使用的模型

        Returns:
            生成的摘要，如果失败返回 None
        """
        if not self._ai_client or not messages:
            return None

        try:
            # 构建摘要提示
            summary_prompt = """请简洁地总结以下对话历史。
摘要应包含：
1. 对话的主要目的或主题
2. 关键的决策或结论
3. 重要的上下文信息
4. 用户设定的任何特殊约束或格式要求

摘要要简洁但完整，以便模型可以在不访问完整历史的情况下继续对话。
"""
            # 将消息转换为文本
            conversation_text = "\n\n".join(
                [f"{msg.role.value}: {msg.content}" for msg in messages if isinstance(msg.content, str)]
            )

            # 构建请求
            summary_messages = [
                Message(role=MessageRole.SYSTEM, content=summary_prompt),
                Message(role=MessageRole.USER, content=f"请总结以下对话：\n\n{conversation_text}"),
            ]

            # 调用 AI 生成摘要
            response = await self._ai_client.chat_completion(
                messages=summary_messages, model=model, temperature=0.3, max_tokens=1000, stream=False
            )

            if hasattr(response, "choices") and len(response.choices) > 0:
                choice = response.choices[0]
                if "message" in choice and "content" in choice["message"]:
                    return choice["message"]["content"]

            return None
        except LLM_EXCEPTIONS as e:
            logger.warning(f"LLM 摘要生成失败，回退到传统截断: {e}")
            return None

    async def _apply_context_management(self, context: Context, conversation: Conversation) -> list[Message]:
        """
        处理上下文管理，包括 LLM 摘要或传统截断

        策略：
        1. 当总 token 数超过水位线时触发
        2. 保留最近 _SUMMARY_KEEP_RECENT 条消息不摘要
        3. 将较早的消息生成摘要
        4. 如果已有摘要，合并新旧摘要
        5. 失败时降级到传统截断

        Args:
            context: 上下文对象
            conversation: 对话对象

        Returns:
            处理后的消息列表
        """
        messages = context.messages

        # 检查是否启用了 LLM 摘要
        enable_summary = self._enable_llm_summary
        # 检查对话级别的设置
        if "enable_llm_summary" in conversation.metadata.custom_data:
            enable_summary = bool(conversation.metadata.custom_data["enable_llm_summary"])

        if not enable_summary or not self._ai_client:
            return messages

        try:
            model_window = get_model_context_window(conversation.model)
            watermark = int(model_window * self._summary_watermark_ratio)
            current_tokens = count_messages_tokens(messages, conversation.model)

            # 如果不需要摘要，直接返回
            if current_tokens <= watermark:
                return messages

            # 检查是否已经有摘要，且 token 数没有显著增长（避免重复生成）
            existing_summary = conversation.metadata.custom_data.get("history_summary")
            summary_last_token_count = conversation.metadata.custom_data.get("summary_last_token_count", 0)

            # 选择要保留和要摘要的消息
            # 保留最近的消息（系统提示词 + 最近 6 条对话），摘要其余部分
            keep_recent = 6
            # 确保至少保留系统提示词和最近几条消息
            split_point = max(1, len(messages) - keep_recent)
            # 保留最近的消息
            recent_messages = messages[split_point:]

            # 摘要较早的消息
            old_messages = messages[:split_point]

            # 如果没有太多消息需要摘要，直接返回
            if len(old_messages) <= 2:
                return messages

            # 检查是否已经有摘要，且 token 数没有显著增长（避免重复生成）
            existing_summary = conversation.metadata.custom_data.get("history_summary")
            summary_last_token_count = conversation.metadata.custom_data.get("summary_last_token_count", 0)

            # 如果已有摘要且 token 增长不足 20%，直接复用现有摘要
            if existing_summary and summary_last_token_count > 0:
                token_growth_ratio = current_tokens / max(1, summary_last_token_count)
                if token_growth_ratio < 1.2:
                    return self._build_messages_with_summary(conversation, existing_summary, recent_messages)

            # 生成新摘要
            new_summary = await self._generate_llm_summary(old_messages, conversation.model)

            if new_summary:
                # 如果已有摘要，合并新旧摘要
                if existing_summary:
                    merged_summary = await self._merge_summaries(existing_summary, new_summary, conversation.model)
                    if merged_summary:
                        new_summary = merged_summary

                # 保存新摘要
                conversation.metadata.custom_data["history_summary"] = new_summary
                conversation.metadata.custom_data["summary_last_token_count"] = current_tokens
                conversation.update_activity()
                await self.update_conversation(conversation.id, custom_data=conversation.metadata.custom_data)

                # 构建并返回带摘要的消息列表
                return self._build_messages_with_summary(conversation, new_summary, recent_messages)
            else:
                # 摘要生成失败，回退到传统截断
                logger.warning("LLM 摘要生成失败，回退到传统截断")
                from ..token_counter import truncate_messages_to_fit

                # 确保有足够的空间
                safe_max_tokens = int(model_window * 0.7)
                return truncate_messages_to_fit(messages, safe_max_tokens, conversation.model)

        except Exception as e:  # noqa: BLE001 - recovery handler must catch all errors
            logger.warning(f"上下文管理失败，回退到原始消息: {e}")
            return messages

    def _build_messages_with_summary(
        self, conversation: Conversation, summary: str, recent_messages: list[Message]
    ) -> list[Message]:
        """
        构建包含摘要的最终消息列表

        Args:
            conversation: 对话对象
            summary: 历史摘要
            recent_messages: 最近的消息列表

        Returns:
            构建后的消息列表
        """
        final_messages: list[Message] = []

        # 添加系统提示词（如果有）
        if conversation.system_prompt:
            final_messages.append(Message(role=MessageRole.SYSTEM, content=conversation.system_prompt))

        # 添加摘要作为系统消息
        summary_msg = Message(
            role=MessageRole.SYSTEM, content=f"[对话历史摘要]\n{summary}\n\n[以上为历史摘要，以下是最近的对话内容]"
        )
        final_messages.append(summary_msg)

        # 添加最近的消息
        final_messages.extend(recent_messages)

        return final_messages

    async def _merge_summaries(self, existing_summary: str, new_summary: str, model: str) -> str | None:
        """
        合并两个摘要为一个连贯的摘要

        Args:
            existing_summary: 已有摘要
            new_summary: 新摘要
            model: 使用的模型

        Returns:
            合并后的摘要，失败返回 None
        """
        if not self._ai_client:
            return None

        combined_summary_prompt = f"""请将以下两个摘要合并为一个连贯的对话历史摘要：

旧摘要：
{existing_summary}

新内容摘要：
{new_summary}
"""
        combined_messages = [
            Message(role=MessageRole.SYSTEM, content="请合并这些摘要，保持简洁连贯。"),
            Message(role=MessageRole.USER, content=combined_summary_prompt),
        ]
        try:
            combined_response = await self._ai_client.chat_completion(
                messages=combined_messages, model=model, temperature=0.3, max_tokens=1000, stream=False
            )
            if hasattr(combined_response, "choices") and len(combined_response.choices) > 0:
                choice = combined_response.choices[0]
                if "message" in choice and "content" in choice["message"]:
                    return choice["message"]["content"]
        except LLM_EXCEPTIONS as e:
            logger.warning(f"摘要合并失败，使用新摘要: {e}")

        return None

    async def create_conversation(
        self,
        title: str | None = None,
        user_id: str | None = None,
        user_name: str | None = None,
        user_info: dict[str, Any] | None = None,
        session_id: str | None = None,
        model: str = "gpt-3.5-turbo",
        temperature: float = 0.7,
        max_tokens: int = 0,
        system_prompt: str | None = None,
        tags: list[str] | None = None,
        custom_data: dict[str, Any] | None = None,
        parent_id: str | None = None,
        enable_llm_summary: bool | None = None,
    ) -> Conversation:
        """创建新对话"""
        async with self._lock:
            from src.shared.id_generator import generate_id

            conversation_id = generate_id()
            context_id = f"conv_{conversation_id}"

            final_custom_data = custom_data.copy() if custom_data else {}
            if enable_llm_summary is not None:
                final_custom_data["enable_llm_summary"] = enable_llm_summary

            metadata = ConversationMetadata(
                user_id=user_id,
                user_name=user_name,
                user_info=user_info,
                session_id=session_id,
                tags=tags or [],
                custom_data=final_custom_data,
            )

            conversation = Conversation(
                id=conversation_id,
                title=title or f"对话 {conversation_id[:8]}",
                status=ConversationStatus.ACTIVE,
                metadata=metadata,
                context_id=context_id,
                model=model,
                temperature=temperature,
                max_tokens=max_tokens,
                system_prompt=system_prompt,
                parent_id=parent_id,
            )

            self._conversations[conversation_id] = conversation
            self._conversation_locks.setdefault(conversation_id, asyncio.Lock())

            # 如果提供了上下文管理器，创建对应的上下文
            if self._context_manager:
                context = Context(
                    id=context_id,
                    type=ContextType.CONVERSATION,
                    metadata=_build_context_metadata(
                        user_id=user_id,
                        user_info=user_info,
                        custom_data=final_custom_data,
                        max_tokens=max_tokens,
                        last_activity_at=metadata.last_activity_at,
                        max_messages=self._max_context_messages,
                    ),
                    model=model,
                    max_tokens=max_tokens,
                )
                if system_prompt:
                    context.add_message(Message(role=MessageRole.SYSTEM, content=system_prompt))
                await self._context_manager.save_context(context_id, context)

            logger.info(f"Created conversation {conversation_id} for user {user_id} with model {model}")

        await self.cleanup_expired_conversations(
            ttl_seconds=self._cleanup_ttl_seconds,
            batch_size=max(100, self._max_conversations),
        )
        return conversation

    async def get_conversation(self, conversation_id: str) -> Conversation:
        """获取对话"""
        lock = await self._get_conversation_lock(conversation_id)
        async with lock:
            if conversation_id not in self._conversations:
                raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")
            return self._conversations[conversation_id]

    async def update_conversation(
        self,
        conversation_id: str,
        title: str | None = None,
        status: ConversationStatus | None = None,
        user_info: dict[str, Any] | None = None,
        tags: list[str] | None = None,
        custom_data: dict[str, Any] | None = None,
        system_prompt: str | None = None,
        enable_llm_summary: bool | None = None,
    ) -> Conversation:
        """更新对话"""
        lock = await self._get_conversation_lock(conversation_id)
        async with lock:
            # 直接访问字典，避免递归锁
            if conversation_id not in self._conversations:
                raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")

            conversation = self._conversations[conversation_id]

            if title is not None:
                conversation.title = title
            if status is not None:
                conversation.status = status
                if status == ConversationStatus.ENDED:
                    conversation.metadata.ended_at = time.time()
            if user_info is not None:
                conversation.metadata.user_info = user_info
            if tags is not None:
                conversation.metadata.tags = tags
            if custom_data is not None:
                conversation.metadata.custom_data = custom_data
            if enable_llm_summary is not None:
                conversation.metadata.custom_data["enable_llm_summary"] = enable_llm_summary
            if system_prompt is not None:
                conversation.system_prompt = system_prompt

            conversation.update_activity()
            logger.info(f"Updated conversation {conversation_id}")
            return conversation

    async def merge_custom_data(
        self,
        conversation_id: str,
        patch: dict[str, Any],
    ) -> Conversation:
        """合并 custom_data（一级 key 粒度）"""
        lock = await self._get_conversation_lock(conversation_id)
        async with lock:
            if conversation_id not in self._conversations:
                raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")

            conversation = self._conversations[conversation_id]
            existing = conversation.metadata.custom_data

            for key, value in patch.items():
                if isinstance(value, dict) and isinstance(existing.get(key), dict):
                    existing[key] = {**existing[key], **value}
                else:
                    existing[key] = value

            conversation.update_activity()
            return conversation

    async def delete_conversation(self, conversation_id: str) -> bool:
        """软删除对话"""
        try:
            await self.update_conversation(conversation_id, status=ConversationStatus.ENDED)
            logger.info(f"Soft deleted conversation {conversation_id}")
            return True
        except ConversationNotFoundError:
            return False

    async def hard_delete_conversation(self, conversation_id: str) -> bool:
        """硬删除对话"""
        lock = await self._get_conversation_lock(conversation_id)
        async with lock:
            async with self._global_lock:
                conversation = self._conversations.pop(conversation_id, None)

            if not conversation:
                return False

            if self._context_manager and conversation.context_id:
                try:
                    await self._context_manager.delete_context(conversation.context_id)
                except Exception as e:  # noqa: BLE001 - cleanup must not raise
                    logger.warning(f"Failed to delete context {conversation.context_id}: {e}")

            async with self._global_lock:
                self._conversation_locks.pop(conversation_id, None)

            logger.info(f"Hard deleted conversation {conversation_id}")
            return True

    async def add_message(
        self,
        conversation_id: str,
        role: MessageRole,
        content: str,
        name: str | None = None,
        tool_calls: list[dict] | None = None,
        tool_call_id: str | None = None,
        metadata: dict[str, Any] | None = None,
        update_activity: bool = True,
    ) -> Message:
        """添加消息到对话"""
        message = Message(
            role=role,
            content=content,
            name=name,
            tool_calls=tool_calls,
            tool_call_id=tool_call_id,
            metadata=metadata,
        )

        lock = await self._get_conversation_lock(conversation_id)
        async with lock:
            conversation = self._conversations.get(conversation_id)
            if conversation is None:
                raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")

            token_count = count_messages_tokens([message], conversation.model)
            conversation.metadata.message_count += 1
            conversation.metadata.total_tokens += token_count
            if update_activity:
                conversation.update_activity()

            context_id = conversation.context_id
            conversation_model = conversation.model
            conversation_max_tokens = conversation.max_tokens
            system_prompt = conversation.system_prompt
            metadata_snapshot = {
                "user_id": conversation.metadata.user_id,
                "user_info": conversation.metadata.user_info,
                "custom_data": conversation.metadata.custom_data,
                "last_activity_at": conversation.metadata.last_activity_at,
            }

        # 上下文写入在锁外进行（ContextManager 有自己的锁）
        if self._context_manager and context_id:
            try:
                await self._context_manager.add_message(context_id, message)
            except ContextNotFoundError:
                from ..context_manager import Context

                context = Context(
                    id=context_id,
                    type=ContextType.CONVERSATION,
                    metadata=_build_context_metadata(
                        user_id=metadata_snapshot["user_id"],
                        user_info=metadata_snapshot["user_info"],
                        custom_data=metadata_snapshot["custom_data"],
                        max_tokens=conversation_max_tokens,
                        last_activity_at=metadata_snapshot["last_activity_at"],
                        max_messages=self._max_context_messages,
                    ),
                    model=conversation_model,
                    max_tokens=conversation_max_tokens,
                )
                if system_prompt:
                    context.add_message(Message(role=MessageRole.SYSTEM, content=system_prompt))
                context.add_message(message)
                await self._context_manager.save_context(context_id, context)

        logger.debug(f"Added message to conversation {conversation_id}: {role.value}")
        return message

    async def get_messages(
        self, conversation_id: str, limit: int | None = None, offset: int = 0, role_filter: MessageRole | None = None
    ) -> list[Message]:
        """获取对话消息"""
        conversation = await self.get_conversation(conversation_id)

        if not self._context_manager or not conversation.context_id:
            return []

        try:
            context = await self._context_manager.get_context(conversation.context_id)
            messages = context.messages

            # 应用角色过滤
            if role_filter:
                messages = [msg for msg in messages if msg.role == role_filter]

            # 应用分页
            start = offset
            end = offset + limit if limit else None
            return messages[start:end]
        except ContextNotFoundError:
            return []

    async def get_conversation_context(self, conversation_id: str, max_tokens: int | None = None) -> list[Message]:
        """获取对话上下文（用于 AI 请求）"""
        conversation = await self.get_conversation(conversation_id)

        if not self._context_manager or not conversation.context_id:
            return []

        try:
            context = await self._context_manager.get_context(conversation.context_id)

            # 应用上下文管理（LLM 摘要或截断）
            messages = await self._apply_context_management(context, conversation)

            # 如果指定了最大令牌数，进行修剪
            if max_tokens and max_tokens > 0:
                from ..token_counter import truncate_messages_to_fit

                messages = truncate_messages_to_fit(messages, max_tokens, conversation.model)

            return messages
        except ContextNotFoundError:
            return []

    async def stream_chat_completion(
        self,
        conversation_id: str,
        user_message: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        **kwargs,
    ) -> AsyncIterator[dict]:
        """流式聊天补全"""
        if not self._ai_client:
            raise RuntimeError("AI client not configured")

        # 添加用户消息
        await self.add_message(conversation_id, MessageRole.USER, user_message)

        # 获取上下文
        context_messages = await self.get_conversation_context(conversation_id)

        # 获取对话配置
        conversation = await self.get_conversation(conversation_id)

        # 调用 AI 客户端
        response = await self._ai_client.chat_completion(
            messages=context_messages,
            model=conversation.model,
            temperature=temperature or conversation.temperature,
            max_tokens=max_tokens or conversation.max_tokens,
            stream=True,
            **kwargs,
        )

        # 处理流式响应
        full_content = ""
        async for chunk in response:
            yield chunk

            # 累积内容（用于存储）
            if hasattr(chunk, "choices") and chunk.choices:
                delta = chunk.choices[0].get("delta", {})
                if delta.get("content"):
                    full_content += delta["content"]

        # 添加 AI 响应到对话
        if full_content:
            await self.add_message(conversation_id, MessageRole.ASSISTANT, full_content)

    async def chat_completion(
        self,
        conversation_id: str,
        user_message: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        **kwargs,
    ) -> ChatCompletionResponse:
        """聊天补全"""
        if not self._ai_client:
            raise RuntimeError("AI client not configured")

        # 添加用户消息
        await self.add_message(conversation_id, MessageRole.USER, user_message)

        # 获取上下文
        context_messages = await self.get_conversation_context(conversation_id)

        # 获取对话配置
        conversation = await self.get_conversation(conversation_id)

        # 调用 AI 客户端 (stream-first)
        runner = LLMStreamRunner(self._ai_client)
        result = await runner.run_chat(
            messages=context_messages,
            model=conversation.model,
            temperature=temperature or conversation.temperature,
            max_tokens=max_tokens or conversation.max_tokens,
            **kwargs,
        )

        # 重建 ChatCompletionResponse 以保持向后兼容
        response = ChatCompletionResponse(
            id=f"stream-{conversation_id}",
            object="chat.completion",
            created=int(time.time()),
            model=result.model or conversation.model,
            choices=[
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": result.text,
                        **({"tool_calls": result.tool_calls} if result.tool_calls else {}),
                    },
                    "finish_reason": "stop",
                }
            ],
            usage=result.usage or {},
        )

        # 添加 AI 响应到对话
        if result.text:
            await self.add_message(
                conversation_id,
                MessageRole.ASSISTANT,
                result.text,
                tool_calls=result.tool_calls if result.tool_calls else None,
            )

        # 更新成本统计（估算）
        if result.usage:
            prompt_tokens = result.usage.get("prompt_tokens", 0)
            completion_tokens = result.usage.get("completion_tokens", 0)
            # 简单估算：$0.002/1K tokens for gpt-3.5-turbo
            cost_per_1k = 0.002
            estimated_cost = (prompt_tokens + completion_tokens) / 1000 * cost_per_1k
            conversation.metadata.total_cost += estimated_cost

        return response

    async def list_conversations(
        self,
        user_id: str | None = None,
        status: ConversationStatus | None = None,
        tag: str | None = None,
        start_time: datetime | None = None,
        end_time: datetime | None = None,
        limit: int | None = None,
        offset: int = 0,
    ) -> list[Conversation]:
        """列出对话"""
        async with self._lock:
            conversations = list(self._conversations.values())

            # 应用过滤器
            if user_id:
                conversations = [c for c in conversations if c.metadata.user_id == user_id]

            if status:
                conversations = [c for c in conversations if c.status == status]

            if tag:
                conversations = [c for c in conversations if tag in c.metadata.tags]

            if start_time:
                start_timestamp = start_time.timestamp()
                conversations = [c for c in conversations if c.metadata.created_at >= start_timestamp]

            if end_time:
                end_timestamp = end_time.timestamp()
                conversations = [c for c in conversations if c.metadata.created_at <= end_timestamp]

            # 按创建时间降序排序
            conversations.sort(key=lambda c: c.metadata.created_at, reverse=True)

            # 应用分页
            start = offset
            end = offset + limit if limit else None
            return conversations[start:end]

    async def search_conversations(
        self,
        query: str,
        user_id: str | None = None,
        status: ConversationStatus | None = None,
        search_fields: list[str] | None = None,
        limit: int | None = None,
    ) -> list[Conversation]:
        """搜索对话"""
        if search_fields is None:
            search_fields = ["title", "content", "tags", "user_name"]

        # 先获取过滤后的对话列表
        conversations = await self.list_conversations(user_id=user_id, status=status)

        results = []
        query_lower = query.lower()

        for conversation in conversations:
            match_score = 0

            # 搜索标题
            if "title" in search_fields and conversation.title and query_lower in conversation.title.lower():
                match_score += 10

            # 搜索用户信息
            if (
                "user_name" in search_fields
                and conversation.metadata.user_name
                and query_lower in conversation.metadata.user_name.lower()
            ):
                match_score += 5

            # 搜索标签
            if "tags" in search_fields:
                for tag in conversation.metadata.tags:
                    if query_lower in tag.lower():
                        match_score += 3

            # 搜索消息内容（如果需要）
            if "content" in search_fields and self._context_manager and conversation.context_id:
                try:
                    context = await self._context_manager.get_context(conversation.context_id)
                    for msg in context.messages:
                        if isinstance(msg.content, str) and query_lower in msg.content.lower():
                            match_score += 1
                            break  # 只需匹配一次
                except ContextNotFoundError:
                    pass

            if match_score > 0:
                results.append((conversation, match_score))

        # 按匹配度排序
        results.sort(key=lambda x: x[1], reverse=True)

        # 应用限制
        if limit:
            results = results[:limit]

        return [conv for conv, score in results]

    async def cleanup_expired_conversations(self, ttl_seconds: int = 86400, batch_size: int = 100) -> int:
        """清理过期对话"""
        effective_ttl = max(1, int(ttl_seconds or self._cleanup_ttl_seconds))
        to_delete = await self._bounded_cleanup_candidates(effective_ttl, batch_size)

        deleted_count = 0
        for conv_id in to_delete:
            if await self.hard_delete_conversation(conv_id):
                deleted_count += 1

        pruned_lock_count = await self._prune_orphaned_conversation_locks()

        if deleted_count > 0:
            logger.info(f"Cleaned up {deleted_count} conversations")
        if pruned_lock_count > 0:
            logger.info(f"Pruned {pruned_lock_count} orphaned conversation locks")

        return deleted_count

    async def get_conversation_stats(self, conversation_id: str) -> dict[str, Any]:
        """获取对话统计"""
        conversation = await self.get_conversation(conversation_id)

        stats = {
            "id": conversation.id,
            "title": conversation.title,
            "status": conversation.status.value,
            "created_at": conversation.metadata.created_at,
            "updated_at": conversation.metadata.updated_at,
            "last_activity_at": conversation.metadata.last_activity_at,
            "message_count": conversation.metadata.message_count,
            "total_tokens": conversation.metadata.total_tokens,
            "total_cost": conversation.metadata.total_cost,
            "model": conversation.model,
            "temperature": conversation.temperature,
            "tags": conversation.metadata.tags,
            "user_id": conversation.metadata.user_id,
            "user_name": conversation.metadata.user_name,
            "llm_summary_enabled": conversation.metadata.custom_data.get(
                "enable_llm_summary", self._enable_llm_summary
            ),
        }

        # 如果有摘要，添加摘要信息
        if "history_summary" in conversation.metadata.custom_data:
            stats["has_history_summary"] = True

        # 如果有上下文管理器，获取更详细的令牌信息
        if self._context_manager and conversation.context_id:
            try:
                context = await self._context_manager.get_context(conversation.context_id)
                stats["current_tokens"] = context.token_count
                stats["message_count_actual"] = len(context.messages)
            except ContextNotFoundError:
                pass

        return stats

    async def archive_conversation(self, conversation_id: str) -> Conversation:
        """归档对话"""
        return await self.update_conversation(conversation_id, status=ConversationStatus.ENDED)

    async def branch_conversation(
        self, conversation_id: str, from_message_id: str | None = None, title: str | None = None
    ) -> Conversation:
        """创建对话分支"""
        if from_message_id:
            raise ValueError("from_message_id is not supported: conversation messages do not expose stable message IDs")

        source_conversation = await self.get_conversation(conversation_id)

        # 创建新对话
        new_title = title or f"{source_conversation.title} (分支)"
        new_conversation = await self.create_conversation(
            title=new_title,
            user_id=source_conversation.metadata.user_id,
            user_name=source_conversation.metadata.user_name,
            user_info=source_conversation.metadata.user_info,
            session_id=source_conversation.metadata.session_id,
            model=source_conversation.model,
            temperature=source_conversation.temperature,
            max_tokens=source_conversation.max_tokens,
            system_prompt=source_conversation.system_prompt,
            tags=source_conversation.metadata.tags.copy(),
            custom_data=source_conversation.metadata.custom_data.copy(),
            parent_id=conversation_id,
        )

        # 复制消息（如果指定了 from_message_id，则从此消息开始复制）
        if self._context_manager and source_conversation.context_id:
            try:
                source_context = await self._context_manager.get_context(source_conversation.context_id)

                # 复制消息到新对话
                for msg in source_context.messages:
                    await self.add_message(
                        new_conversation.id,
                        msg.role,
                        msg.content,
                        name=msg.name,
                        tool_calls=msg.tool_calls,
                        tool_call_id=msg.tool_call_id,
                        update_activity=False,
                    )

                logger.info(
                    f"Created branch conversation {new_conversation.id} "
                    f"from {conversation_id}, copied {len(source_context.messages)} messages"
                )

            except ContextNotFoundError:
                logger.warning(f"Source context not found for conversation {conversation_id}")

        return new_conversation


class RedisConversationManager(MemoryConversationManager):
    """
    基于 Redis 的对话管理器（支持持久化和分布式）
    """

    def __init__(
        self,
        redis_url: str = "",
        context_manager: ContextManager | None = None,
        ai_client: AiGateway | None = None,
        default_model: str = "gpt-3.5-turbo",
        max_conversations: int = _DEFAULT_MAX_CONVERSATIONS,
        cleanup_ttl_seconds: int = _DEFAULT_CONVERSATION_TTL_SECONDS,
        max_context_messages: int = _DEFAULT_MAX_CONTEXT_MESSAGES,
        enable_llm_summary: bool = _DEFAULT_LLM_SUMMARY_ENABLED,
        summary_watermark_ratio: float = _DEFAULT_SUMMARY_WATERMARK_RATIO,
    ):
        import redis.asyncio as redis

        if not str(redis_url or "").strip():
            raise ValueError("RedisConversationManager requires an explicit redis_url")
        super().__init__(
            context_manager=context_manager,
            ai_client=ai_client,
            default_model=default_model,
            max_conversations=max_conversations,
            cleanup_ttl_seconds=cleanup_ttl_seconds,
            max_context_messages=max_context_messages,
            enable_llm_summary=enable_llm_summary,
            summary_watermark_ratio=summary_watermark_ratio,
        )
        self.redis_client = redis.from_url(redis_url, decode_responses=True)
        self._key_prefix = "ai:conversation:"
        logger.info(f"RedisConversationManager 初始化完成，Redis URL: {redis_url}")

    def _make_key(self, conversation_id: str) -> str:
        return f"{self._key_prefix}{conversation_id}"

    async def create_conversation(self, **kwargs) -> Conversation:
        """创建对话并保存到 Redis"""
        conversation = await super().create_conversation(**kwargs)
        await self._save_to_redis(conversation)
        return conversation

    async def get_conversation(self, conversation_id: str) -> Conversation:
        """从 Redis 获取对话（JSON 格式）。"""
        key = self._make_key(conversation_id)
        data = await self.redis_client.get(key)

        if data is None:
            return await super().get_conversation(conversation_id)

        try:
            conversation_dict = json.loads(data)
            conversation = Conversation.from_dict(conversation_dict)
            async with self._global_lock:
                self._conversations[conversation_id] = conversation
            return conversation
        except (json.JSONDecodeError, TypeError, ValueError) as e:
            logger.warning(
                "无法解析 Redis 对话数据（请清理遗留 pickle 条目）: %s error=%s",
                conversation_id,
                e,
            )
            return await super().get_conversation(conversation_id)

    async def update_conversation(self, conversation_id: str, **kwargs) -> Conversation:
        """更新对话并保存到 Redis"""
        conversation = await super().update_conversation(conversation_id, **kwargs)
        await self._save_to_redis(conversation)
        return conversation

    async def delete_conversation(self, conversation_id: str) -> bool:
        """软删除对话"""
        result = await super().delete_conversation(conversation_id)
        if result:
            await self._delete_from_redis(conversation_id)
        return result

    async def hard_delete_conversation(self, conversation_id: str) -> bool:
        """硬删除对话"""
        result = await super().hard_delete_conversation(conversation_id)
        if result:
            await self._delete_from_redis(conversation_id)
        return result

    async def _save_to_redis(self, conversation: Conversation) -> None:
        """保存对话到 Redis（使用 JSON 格式）"""
        key = self._make_key(conversation.id)
        try:
            conversation_dict = conversation.to_dict()
            data = json.dumps(conversation_dict, ensure_ascii=False)
            # 设置过期时间（7 天）
            await self.redis_client.setex(key, 604800, data)
            logger.debug(f"Conversation '{conversation.id}' saved to Redis (JSON format)")
        except Exception as e:
            logger.error("保存对话到 Redis 失败: %s", e, exc_info=True)
            raise

    async def _delete_from_redis(self, conversation_id: str) -> None:
        """从 Redis 删除对话"""
        key = self._make_key(conversation_id)
        await self.redis_client.delete(key)
        logger.debug(f"Conversation '{conversation_id}' deleted from Redis")


# 全局默认管理器（内存）
_default_conversation_manager = None


def get_default_conversation_manager() -> ConversationManager:
    """获取默认的对话管理器实例（内存）"""
    global _default_conversation_manager
    from src.infrastructure.runtime.providers import get_runtime_container

    container = get_runtime_container()
    if container is not None:
        _default_conversation_manager = getattr(container, "default_conversation_manager", None)
        if container is not None and _default_conversation_manager is not None:
            container.default_conversation_manager = _default_conversation_manager
        if _default_conversation_manager is not None:
            return _default_conversation_manager
    if _default_conversation_manager is None:
        _default_conversation_manager = MemoryConversationManager()
        if container is not None:
            container.default_conversation_manager = _default_conversation_manager
    return _default_conversation_manager


def set_default_conversation_manager(manager: ConversationManager) -> None:
    """设置默认的对话管理器"""
    global _default_conversation_manager
    _default_conversation_manager = manager
    try:
        from src.infrastructure.runtime.providers import get_runtime_container

        container = get_runtime_container()
        if container is not None:
            container.default_conversation_manager = _default_conversation_manager
    except Exception as e:  # noqa: BLE001 - best-effort side effect must not abort main flow
        logger.debug(f"无法更新容器中的默认管理器: {e}")
