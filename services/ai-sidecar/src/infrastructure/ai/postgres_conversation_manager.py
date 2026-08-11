"""
PostgreSQL 对话管理器

提供基于 PostgreSQL 的持久化对话管理，支持分布式环境下的对话状态共享。
"""

import asyncio
import json
from collections.abc import AsyncIterator
from datetime import datetime
from typing import Any

from psycopg.rows import dict_row

from src.domain.utils.time_utils import utc_now
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.database.connection import AsyncDatabaseConnectionInterface
from src.infrastructure.database.query_builder import (
    ComparisonOperator,
    DeleteBuilder,
    InsertBuilder,
    OrderDirection,
    QueryBuilder,
    UpdateBuilder,
)
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

from .context_manager import Context, ContextError, ContextManager, ContextNotFoundError, ContextType
from .conversation_manager import (
    Conversation,
    ConversationManager,
    ConversationMetadata,
    ConversationNotFoundError,
    ConversationStatus,
    _build_context_metadata,
)
from .openai_client import AiGateway, ChatCompletionResponse, Message, MessageRole
from .token_counter import count_messages_tokens

logger = get_logger(__name__)


class PostgresConversationManager(ConversationManager):
    """
    基于 PostgreSQL 的对话管理器实现

    支持持久化存储和多实例环境下的对话状态共享。
    """

    def __init__(
        self,
        db_connection: AsyncDatabaseConnectionInterface,
        context_manager: ContextManager | None = None,
        ai_client: AiGateway | None = None,
        default_model: str = "gpt-3.5-turbo",
    ):
        """
        初始化 PostgreSQL 对话管理器

        Args:
            db_connection: 异步数据库连接接口
            context_manager: 上下文管理器
            ai_client: AI 客户端
            default_model: 默认模型
        """
        self._db_connection = db_connection
        self._context_manager = context_manager
        self._ai_client = ai_client
        self._default_model = default_model
        self._lock = asyncio.Lock()
        self._initialized = False

        logger.info(f"PostgresConversationManager 初始化完成，默认模型: {default_model}")

    async def _ensure_initialized(self):
        """确保表结构已初始化"""
        if self._initialized:
            return

        async with self._lock:
            if self._initialized:
                return
            await self._init_tables()
            self._initialized = True

    async def _init_tables(self):
        """初始化数据库表结构"""
        try:
            # 使用 transaction_context 确保 DDL 操作的原子性
            async with self._db_connection.transaction_context() as conn:  # noqa: SIM117 - explicit transaction/cursor scopes
                async with conn.cursor() as cursor:
                    # 创建对话表
                    await cursor.execute("""
                        CREATE TABLE IF NOT EXISTS ai_conversations (
                            id VARCHAR(255) PRIMARY KEY,
                            entity_id VARCHAR(255),
                            title VARCHAR(500),
                            status VARCHAR(50) NOT NULL,
                            context_id VARCHAR(255),
                            model VARCHAR(100) NOT NULL,
                            temperature FLOAT NOT NULL DEFAULT 0.7,
                            max_tokens INTEGER NOT NULL DEFAULT 0,
                            system_prompt TEXT,
                            parent_id VARCHAR(255),

                            -- 元数据字段
                            user_id VARCHAR(255),
                            user_name VARCHAR(255),
                            user_info JSONB,
                            session_id VARCHAR(255),
                            client_info JSONB,
                            tags TEXT[],
                            custom_data JSONB,

                            -- 时间戳
                            created_at TIMESTAMP NOT NULL,
                            updated_at TIMESTAMP NOT NULL,
                            last_activity_at TIMESTAMP NOT NULL,
                            ended_at TIMESTAMP,

                            -- 统计信息
                            message_count INTEGER NOT NULL DEFAULT 0,
                            total_tokens BIGINT NOT NULL DEFAULT 0,
                            total_cost FLOAT NOT NULL DEFAULT 0.0,

                            -- 扩展字段
                            metadata JSONB,
                            extensions JSONB
                        )
                    """)

                    # 兼容历史库：补齐缺失列（旧版本 ai_conversations 字段较少）
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS entity_id VARCHAR(255)")
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS status VARCHAR(50) DEFAULT 'active'"
                    )
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS context_id VARCHAR(255)"
                    )
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS model VARCHAR(100) DEFAULT 'gpt-3.5-turbo'"
                    )
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS temperature FLOAT DEFAULT 0.7"
                    )
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS max_tokens INTEGER DEFAULT 0"
                    )
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS system_prompt TEXT")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS parent_id VARCHAR(255)")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS user_id VARCHAR(255)")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS user_name VARCHAR(255)")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS user_info JSONB")
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS session_id VARCHAR(255)"
                    )
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS client_info JSONB")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS tags TEXT[]")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS custom_data JSONB")
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMP"
                    )
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS ended_at TIMESTAMP")
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS message_count INTEGER DEFAULT 0"
                    )
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS total_tokens BIGINT DEFAULT 0"
                    )
                    await cursor.execute(
                        "ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS total_cost FLOAT DEFAULT 0.0"
                    )
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS metadata JSONB")
                    await cursor.execute("ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS extensions JSONB")

                    # 旧库字段允许为空时，补齐默认时间
                    await cursor.execute(
                        "UPDATE ai_conversations SET created_at = CURRENT_TIMESTAMP WHERE created_at IS NULL"
                    )
                    await cursor.execute(
                        "UPDATE ai_conversations SET updated_at = CURRENT_TIMESTAMP WHERE updated_at IS NULL"
                    )
                    await cursor.execute(
                        "UPDATE ai_conversations SET last_activity_at = COALESCE(last_activity_at, updated_at, created_at, CURRENT_TIMESTAMP)"
                    )
                    await cursor.execute("UPDATE ai_conversations SET status = 'active' WHERE status IS NULL")

                    # 创建索引
                    await cursor.execute("""
                        CREATE INDEX IF NOT EXISTS idx_conversations_user_id
                        ON ai_conversations(user_id)
                    """)
                    await cursor.execute("""
                        CREATE INDEX IF NOT EXISTS idx_conversations_status
                        ON ai_conversations(status)
                    """)
                    await cursor.execute("""
                        CREATE INDEX IF NOT EXISTS idx_conversations_created_at
                        ON ai_conversations(created_at DESC)
                    """)
                    await cursor.execute("""
                        CREATE INDEX IF NOT EXISTS idx_ai_conversations_entity_id
                        ON ai_conversations(entity_id)
                    """)
                    await cursor.execute("""
                        CREATE INDEX IF NOT EXISTS idx_conversations_tags
                        ON ai_conversations USING GIN(tags)
                    """)

                    logger.info("PostgresConversationManager 表初始化成功")

        except Exception as e:
            logger.error(f"初始化对话表时发生错误: {e}")
            raise

    def _conversation_to_dict(self, conversation: Conversation) -> dict[str, Any]:
        """将 Conversation 对象转换为字典"""
        return {
            "id": conversation.id,
            "title": conversation.title,
            "status": conversation.status.value,
            "context_id": conversation.context_id,
            "model": conversation.model,
            "temperature": conversation.temperature,
            "max_tokens": conversation.max_tokens,
            "system_prompt": conversation.system_prompt,
            "parent_id": conversation.parent_id,
            "user_id": conversation.metadata.user_id,
            "user_name": conversation.metadata.user_name,
            "user_info": json.dumps(conversation.metadata.user_info) if conversation.metadata.user_info else None,
            "session_id": conversation.metadata.session_id,
            "client_info": json.dumps(conversation.metadata.client_info) if conversation.metadata.client_info else None,
            "tags": conversation.metadata.tags,
            "custom_data": json.dumps(conversation.metadata.custom_data) if conversation.metadata.custom_data else None,
            "created_at": datetime.fromtimestamp(conversation.metadata.created_at),
            "updated_at": datetime.fromtimestamp(conversation.metadata.updated_at),
            "last_activity_at": datetime.fromtimestamp(conversation.metadata.last_activity_at),
            "ended_at": datetime.fromtimestamp(conversation.metadata.ended_at)
            if conversation.metadata.ended_at
            else None,
            "message_count": conversation.metadata.message_count,
            "total_tokens": conversation.metadata.total_tokens,
            "total_cost": conversation.metadata.total_cost,
            "extensions": json.dumps(conversation.extensions) if conversation.extensions else None,
        }

    @staticmethod
    def _decode_json_field(value: Any, default: Any) -> Any:
        """兼容解析 JSON 字段（支持 dict/list 或 JSON 字符串）。"""
        if value is None:
            return default

        # psycopg/asyncpg 读取 JSONB 时可能直接返回 dict/list
        if isinstance(value, (dict, list)):
            return value

        if isinstance(value, (bytes, bytearray)):
            value = value.decode("utf-8", errors="replace")

        if isinstance(value, str):
            try:
                return json.loads(value)
            except json.JSONDecodeError:
                logger.warning(f"JSON 字段解析失败，使用默认值。原始值类型: {type(value).__name__}")
                return default

        logger.warning(f"JSON 字段类型不受支持，使用默认值。类型: {type(value).__name__}")
        return default

    @classmethod
    def _decode_json_dict_field(cls, value: Any, default: dict[str, Any] | None) -> dict[str, Any] | None:
        decoded = cls._decode_json_field(value, default)
        if decoded is None:
            return None
        if isinstance(decoded, dict):
            return decoded
        logger.warning(f"JSON 字段预期为对象(dict)，实际为 {type(decoded).__name__}，使用默认值")
        return default

    def _dict_to_conversation(self, row: dict[str, Any]) -> Conversation:
        """将数据库行转换为 Conversation 对象"""
        metadata = ConversationMetadata(
            user_id=row.get("user_id"),
            user_name=row.get("user_name"),
            user_info=self._decode_json_dict_field(row.get("user_info"), None),
            session_id=row.get("session_id"),
            client_info=self._decode_json_dict_field(row.get("client_info"), None),
            tags=row.get("tags", []),
            custom_data=self._decode_json_dict_field(row.get("custom_data"), {}) or {},
            created_at=row["created_at"].timestamp(),
            updated_at=row["updated_at"].timestamp(),
            last_activity_at=row["last_activity_at"].timestamp(),
            ended_at=row["ended_at"].timestamp() if row.get("ended_at") else None,
            message_count=row.get("message_count", 0),
            total_tokens=row.get("total_tokens", 0),
            total_cost=row.get("total_cost", 0.0),
        )

        return Conversation(
            id=row["id"],
            title=row.get("title"),
            status=ConversationStatus(row["status"]),
            metadata=metadata,
            context_id=row.get("context_id"),
            model=row["model"],
            temperature=row["temperature"],
            max_tokens=row["max_tokens"],
            system_prompt=row.get("system_prompt"),
            parent_id=row.get("parent_id"),
            extensions=self._decode_json_dict_field(row.get("extensions"), {}) or {},
        )

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
    ) -> Conversation:
        """创建新对话"""
        await self._ensure_initialized()

        conversation_id = generate_id()
        context_id = f"conv_{conversation_id}"

        metadata = ConversationMetadata(
            user_id=user_id,
            user_name=user_name,
            user_info=user_info,
            session_id=session_id,
            tags=tags or [],
            custom_data=custom_data or {},
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

        # 保存到数据库
        try:
            # 使用 transaction_context 自动提交
            async with self._db_connection.transaction_context() as conn, conn.cursor() as cursor:
                conv_dict = self._conversation_to_dict(conversation)
                columns = list(conv_dict.keys())
                values = list(conv_dict.values())

                query, params = InsertBuilder().into_table("ai_conversations").columns(*columns).values(*values).build()

                await cursor.execute(query, params)

        except Exception as e:
            logger.error(f"Failed to create conversation in database: {e}")
            raise

        # 如果提供了上下文管理器，创建对应的上下文
        if self._context_manager:
            context = Context(
                id=context_id,
                type=ContextType.CONVERSATION,
                metadata=_build_context_metadata(
                    user_id=user_id,
                    user_info=user_info,
                    custom_data=custom_data,
                    max_tokens=max_tokens,
                    last_activity_at=metadata.last_activity_at,
                ),
                model=model,
                max_tokens=max_tokens,
            )
            if system_prompt:
                context.add_message(Message(role=MessageRole.SYSTEM, content=system_prompt))
            try:
                await self._context_manager.save_context(context_id, context)
            except ContextError as e:
                logger.warning(
                    f"Failed to initialize conversation context '{context_id}', "
                    f"continue without context persistence: {e}"
                )

        logger.info(f"Created conversation {conversation_id} for user {user_id} with model {model}")
        return conversation

    async def get_conversation(self, conversation_id: str) -> Conversation:
        """获取对话"""
        await self._ensure_initialized()

        try:
            # 使用 connection_context 进行查询
            async with self._db_connection.connection_context() as conn:  # noqa: SIM117 - explicit connection/cursor scopes
                async with conn.cursor(row_factory=dict_row) as cursor:
                    query, params = (
                        QueryBuilder()
                        .select("*")
                        .from_table("ai_conversations")
                        .where("id", ComparisonOperator.EQ, conversation_id)
                        .build()
                    )

                    await cursor.execute(query, params)
                    row_dict = await cursor.fetchone()

                    if not row_dict:
                        raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")

                    return self._dict_to_conversation(row_dict)

        except ConversationNotFoundError:
            raise
        except Exception as e:
            logger.error(f"Failed to get conversation '{conversation_id}': {e}")
            raise

    async def update_conversation(
        self,
        conversation_id: str,
        title: str | None = None,
        status: ConversationStatus | None = None,
        user_info: dict[str, Any] | None = None,
        tags: list[str] | None = None,
        custom_data: dict[str, Any] | None = None,
        system_prompt: str | None = None,
    ) -> Conversation:
        """更新对话"""
        await self._ensure_initialized()

        try:
            # 构建更新字段
            update_fields = {"updated_at": utc_now()}

            if title is not None:
                update_fields["title"] = title
            if status is not None:
                update_fields["status"] = status.value
                if status == ConversationStatus.ENDED:
                    update_fields["ended_at"] = utc_now()
            if user_info is not None:
                update_fields["user_info"] = json.dumps(user_info)
            if tags is not None:
                update_fields["tags"] = tags
            if custom_data is not None:
                update_fields["custom_data"] = json.dumps(custom_data)
            if system_prompt is not None:
                update_fields["system_prompt"] = system_prompt

            async with self._db_connection.transaction_context() as conn, conn.cursor() as cursor:
                # 构建 UPDATE 查询
                builder = UpdateBuilder().table("ai_conversations")
                for field, value in update_fields.items():
                    builder = builder.set(field, value)
                builder = builder.where("id", ComparisonOperator.EQ, conversation_id)

                query, params = builder.build()
                await cursor.execute(query, params)

                if cursor.rowcount == 0:
                    raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")

            logger.info(f"Updated conversation {conversation_id}")
            return await self.get_conversation(conversation_id)

        except ConversationNotFoundError:
            raise
        except Exception as e:
            logger.error(f"Failed to update conversation '{conversation_id}': {e}")
            raise

    async def merge_custom_data(
        self,
        conversation_id: str,
        patch: dict[str, Any],
    ) -> Conversation:
        """合并 custom_data（一级 key 粒度，读改写）"""
        await self._ensure_initialized()

        try:
            async with self._db_connection.transaction_context() as conn, conn.cursor() as cursor:
                # 1. 读取现有 custom_data
                query_get, params_get = (
                    QueryBuilder()
                    .select("custom_data")
                    .from_table("ai_conversations")
                    .where("id", ComparisonOperator.EQ, conversation_id)
                    .build()
                )
                await cursor.execute(query_get, params_get)
                row = await cursor.fetchone()
                if row is None:
                    raise ConversationNotFoundError(f"Conversation '{conversation_id}' not found")

                raw_custom_data = row[0]
                if isinstance(raw_custom_data, str):
                    existing: dict[str, Any] = json.loads(raw_custom_data) if raw_custom_data else {}
                elif isinstance(raw_custom_data, dict):
                    existing = raw_custom_data
                else:
                    existing = {}

                # 2. 一级 key 合并
                for key, value in patch.items():
                    if isinstance(value, dict) and isinstance(existing.get(key), dict):
                        existing[key] = {**existing[key], **value}
                    else:
                        existing[key] = value

                # 3. 写回
                builder = UpdateBuilder().table("ai_conversations")
                builder = builder.set("custom_data", json.dumps(existing, ensure_ascii=False))
                builder = builder.set("updated_at", utc_now())
                builder = builder.where("id", ComparisonOperator.EQ, conversation_id)
                query, params = builder.build()
                await cursor.execute(query, params)

            logger.info(f"Merged custom_data for conversation {conversation_id}")
            return await self.get_conversation(conversation_id)

        except ConversationNotFoundError:
            raise
        except Exception as e:
            logger.error(f"Failed to merge custom_data for '{conversation_id}': {e}")
            raise

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
        await self._ensure_initialized()

        try:
            async with self._db_connection.transaction_context() as conn, conn.cursor() as cursor:
                # 先获取对话以获取 context_id
                query_get, params_get = (
                    QueryBuilder()
                    .select("context_id")
                    .from_table("ai_conversations")
                    .where("id", ComparisonOperator.EQ, conversation_id)
                    .build()
                )
                await cursor.execute(query_get, params_get)
                row = await cursor.fetchone()

                context_id = row[0] if row else None

                # 删除数据库记录
                query, params = (
                    DeleteBuilder()
                    .from_table("ai_conversations")
                    .where("id", ComparisonOperator.EQ, conversation_id)
                    .build()
                )

                await cursor.execute(query, params)
                deleted_count = cursor.rowcount

                # 删除关联的上下文
                if self._context_manager and context_id:
                    try:
                        await self._context_manager.delete_context(context_id)
                    except Exception as e:  # noqa: BLE001 - context manager cleanup must catch all failures
                        logger.warning(f"Failed to delete context {context_id}: {e}")

                logger.info(f"Hard deleted conversation {conversation_id}")
                return deleted_count > 0
        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"Failed to hard delete conversation '{conversation_id}': {e}")
            return False

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
        await self._ensure_initialized()

        # 先获取对话（不持有锁）
        conversation = await self.get_conversation(conversation_id)

        message = Message(
            role=role,
            content=content,
            name=name,
            tool_calls=tool_calls,
            tool_call_id=tool_call_id,
            metadata=metadata,
        )

        # 预先计算 token（在锁外，减少锁持有时间）
        token_count = count_messages_tokens([message], conversation.model) if self._context_manager else 0

        try:
            async with self._db_connection.transaction_context() as conn, conn.cursor() as cursor:
                update_fields = {
                    "message_count": conversation.metadata.message_count + 1,
                    "total_tokens": conversation.metadata.total_tokens + token_count,
                }

                if update_activity:
                    update_fields["last_activity_at"] = utc_now()
                    update_fields["updated_at"] = utc_now()

                builder = UpdateBuilder().table("ai_conversations")
                for field, value in update_fields.items():
                    builder = builder.set(field, value)
                builder = builder.where("id", ComparisonOperator.EQ, conversation_id)

                query, params = builder.build()
                await cursor.execute(query, params)

        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"Failed to update conversation statistics: {e}")

        # 上下文管理（在锁外执行，避免阻塞）
        if self._context_manager:
            try:
                await self._context_manager.add_message(conversation.context_id, message)
            except ContextNotFoundError:
                context = Context(
                    id=conversation.context_id,
                    type=ContextType.CONVERSATION,
                    metadata=_build_context_metadata(
                        user_id=conversation.metadata.user_id,
                        user_info=conversation.metadata.user_info,
                        custom_data=conversation.metadata.custom_data,
                        max_tokens=conversation.max_tokens,
                        last_activity_at=conversation.metadata.last_activity_at,
                    ),
                    model=conversation.model,
                    max_tokens=conversation.max_tokens,
                )
                if conversation.system_prompt:
                    context.add_message(Message(role=MessageRole.SYSTEM, content=conversation.system_prompt))
                context.add_message(message)
                try:
                    await self._context_manager.save_context(conversation.context_id, context)
                except ContextError as e:
                    logger.warning(
                        f"Failed to rebuild context '{conversation.context_id}', "
                        f"message persisted in conversation only: {e}"
                    )
            except ContextError as e:
                logger.warning(
                    f"Failed to append message into context '{conversation.context_id}', "
                    f"message persisted in conversation only: {e}"
                )

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
            messages = context.messages

            # 如果指定了最大令牌数，进行修剪
            if max_tokens and max_tokens > 0:
                from .token_counter import truncate_messages_to_fit

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
        # 实现保持不变，因为 get_conversation 和 add_message 已经是 async 的了
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
        from .llm_stream_runner import LLMStreamRunner

        runner = LLMStreamRunner(self._ai_client)
        result = await runner.run_chat(
            messages=context_messages,
            model=conversation.model,
            temperature=temperature or conversation.temperature,
            max_tokens=max_tokens or conversation.max_tokens,
            **kwargs,
        )

        # 重建 ChatCompletionResponse 以保持向后兼容
        import time as _time

        response = ChatCompletionResponse(
            id=f"stream-{conversation_id}",
            object="chat.completion",
            created=int(_time.time()),
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
            await self.add_message(conversation_id, MessageRole.ASSISTANT, result.text)

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
        """列出对话（支持过滤和分页）"""
        await self._ensure_initialized()

        try:
            async with self._db_connection.connection_context() as conn:  # noqa: SIM117 - explicit connection/cursor scopes
                async with conn.cursor(row_factory=dict_row) as cursor:
                    builder = QueryBuilder().select("*").from_table("ai_conversations")

                    # 应用过滤条件
                    if user_id:
                        builder = builder.where("user_id", ComparisonOperator.EQ, user_id)
                    if status:
                        builder = builder.where("status", ComparisonOperator.EQ, status.value)
                    if tag:
                        # PostgreSQL 数组包含查询
                        builder = builder.where_raw("%s = ANY(tags)", [tag])
                    if start_time:
                        builder = builder.where("created_at", ComparisonOperator.GTE, start_time)
                    if end_time:
                        builder = builder.where("created_at", ComparisonOperator.LTE, end_time)

                    # 排序
                    builder = builder.order_by("created_at", OrderDirection.DESC)

                    # 分页
                    if limit:
                        builder = builder.limit(limit)
                    if offset:
                        builder = builder.offset(offset)

                    query, params = builder.build()
                    await cursor.execute(query, params)
                    rows = await cursor.fetchall()

                    return [self._dict_to_conversation(row) for row in rows]

        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"Failed to list conversations: {e}")
            return []

    async def search_conversations(
        self,
        query: str,
        user_id: str | None = None,
        status: ConversationStatus | None = None,
        search_fields: list[str] | None = None,
        limit: int | None = None,
    ) -> list[Conversation]:
        """搜索对话（基于内容和元数据）"""
        await self._ensure_initialized()

        # 当前实现基于标题搜索
        try:
            async with self._db_connection.connection_context() as conn:  # noqa: SIM117 - explicit connection/cursor scopes
                async with conn.cursor(row_factory=dict_row) as cursor:
                    # 确保query已经过简单的清洁，虽然使用了参数化查询，但避免传入过大字符串
                    clean_query = query.strip()[:200]
                    builder = (
                        QueryBuilder()
                        .select("*")
                        .from_table("ai_conversations")
                        .where_raw("title ILIKE %s", [f"%{clean_query}%"])
                    )

                    if user_id:
                        builder = builder.where("user_id", ComparisonOperator.EQ, user_id)
                    if status:
                        builder = builder.where("status", ComparisonOperator.EQ, status.value)

                    builder = builder.order_by("created_at", OrderDirection.DESC)

                    if limit:
                        builder = builder.limit(limit)

                    query_sql, params = builder.build()
                    await cursor.execute(query_sql, params)
                    rows = await cursor.fetchall()

                    return [self._dict_to_conversation(row) for row in rows]

        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"Failed to search conversations: {e}")
            return []

    async def cleanup_expired_conversations(self, ttl_seconds: int = 86400, batch_size: int = 100) -> int:
        """清理过期对话"""
        await self._ensure_initialized()

        try:
            async with self._db_connection.transaction_context() as conn, conn.cursor() as cursor:
                # 计算过期时间
                expiry_time = utc_now().timestamp() - ttl_seconds
                expiry_datetime = datetime.fromtimestamp(expiry_time)

                # 更新过期对话状态
                query, params = (
                    UpdateBuilder()
                    .table("ai_conversations")
                    .set("status", ConversationStatus.EXPIRED.value)
                    .set("ended_at", utc_now())
                    .where("last_activity_at", ComparisonOperator.LT, expiry_datetime)
                    .where("status", ComparisonOperator.EQ, ConversationStatus.ACTIVE.value)
                    .build()
                )

                await cursor.execute(query, params)

                count = cursor.rowcount
                if count > 0:
                    logger.info(f"Cleaned up {count} expired conversations")

                return count

        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"Failed to cleanup expired conversations: {e}")
            return 0

    async def get_conversation_stats(self, conversation_id: str) -> dict[str, Any]:
        """获取对话统计信息"""
        conversation = await self.get_conversation(conversation_id)

        return {
            "conversation_id": conversation.id,
            "message_count": conversation.metadata.message_count,
            "total_tokens": conversation.metadata.total_tokens,
            "total_cost": conversation.metadata.total_cost,
            "status": conversation.status.value,
            "created_at": conversation.metadata.created_at,
            "last_activity_at": conversation.metadata.last_activity_at,
            "duration_seconds": conversation.metadata.last_activity_at - conversation.metadata.created_at,
        }

    async def archive_conversation(self, conversation_id: str) -> Conversation:
        """归档对话"""
        return await self.update_conversation(conversation_id, status=ConversationStatus.ENDED)

    async def branch_conversation(
        self, conversation_id: str, from_message_id: str | None = None, title: str | None = None
    ) -> Conversation:
        """创建对话分支"""
        if from_message_id:
            raise ValueError("from_message_id is not supported: conversation messages do not expose stable message IDs")

        # 获取源对话
        source_conversation = await self.get_conversation(conversation_id)

        # 创建新对话（作为分支）
        branch = await self.create_conversation(
            title=title or f"{source_conversation.title} (分支)",
            user_id=source_conversation.metadata.user_id,
            user_name=source_conversation.metadata.user_name,
            user_info=source_conversation.metadata.user_info,
            session_id=source_conversation.metadata.session_id,
            model=source_conversation.model,
            temperature=source_conversation.temperature,
            max_tokens=source_conversation.max_tokens,
            system_prompt=source_conversation.system_prompt,
            tags=source_conversation.metadata.tags,
            custom_data=source_conversation.metadata.custom_data,
            parent_id=conversation_id,
        )

        # 如果指定了起始消息，复制消息历史
        if self._context_manager and source_conversation.context_id:
            try:
                # 获取源上下文
                source_context = await self._context_manager.get_context(source_conversation.context_id)
                messages_to_copy = source_context.messages

                # 添加到新上下文
                for msg in messages_to_copy:
                    await self._context_manager.add_message(branch.context_id, msg)

            except Exception as e:  # noqa: BLE001 - context manager branch copy must catch all failures
                logger.warning(f"Failed to copy context for branch: {e}")

        return branch
