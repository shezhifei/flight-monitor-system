"""AI 对话上下文 Redis 缓存

存储用户对话历史，支持长对话的快速恢复
"""

import orjson

from src.infrastructure.common.exceptions import JSON_EXCEPTIONS, REDIS_EXCEPTIONS
from src.infrastructure.database.redis_connection import get_redis_client, is_redis_available
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AIContextCache:
    """AI 上下文缓存服务"""

    KEY_PATTERN = "ai:context:{user_id}:{conversation_id}"
    DEFAULT_TTL = 86400  # 24小时

    @staticmethod
    def _messages_key(base_key: str) -> str:
        return f"{base_key}:messages"

    def _get_redis_client(self):
        """安全获取异步 Redis 客户端"""
        if not is_redis_available():
            return None
        try:
            return get_redis_client()
        except RuntimeError:
            return None

    async def get_context(self, user_id: str, conversation_id: str) -> list[dict] | None:
        key = self.KEY_PATTERN.format(user_id=user_id, conversation_id=conversation_id)
        messages_key = self._messages_key(key)

        redis_client = self._get_redis_client()
        if not redis_client:
            return None

        try:
            list_items = await redis_client.lrange(messages_key, 0, -1)
            if list_items:
                messages: list[dict] = []
                for raw_item in list_items:
                    try:
                        messages.append(orjson.loads(raw_item))
                    except JSON_EXCEPTIONS as exc:
                        logger.warning("ai_context_cache_message_deserialize_failed", exc_info=exc)
                        continue
                return messages

            data = await redis_client.get(key)
            if data:
                return orjson.loads(data)
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Failed to get AI context: {e}")

        return None

    async def set_context(self, user_id: str, conversation_id: str, messages: list[dict], ttl: int | None = None):
        key = self.KEY_PATTERN.format(user_id=user_id, conversation_id=conversation_id)
        messages_key = self._messages_key(key)

        redis_client = self._get_redis_client()
        if not redis_client:
            return

        try:
            effective_ttl = ttl or self.DEFAULT_TTL
            pipe = redis_client.pipeline(transaction=True)
            await pipe.delete(messages_key)
            serialized_messages = [orjson.dumps(message) for message in (messages or [])]
            if serialized_messages:
                await pipe.rpush(messages_key, *serialized_messages)
                await pipe.expire(messages_key, effective_ttl)
            else:
                await pipe.delete(messages_key)

            # 兼容旧格式读取路径
            await pipe.setex(key, effective_ttl, orjson.dumps(messages or []))
            await pipe.execute()
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Failed to set AI context: {e}")

    async def append_message(self, user_id: str, conversation_id: str, message: dict):
        """追加消息到对话历史"""
        key = self.KEY_PATTERN.format(user_id=user_id, conversation_id=conversation_id)
        messages_key = self._messages_key(key)

        redis_client = self._get_redis_client()
        if not redis_client:
            return

        try:
            payload = orjson.dumps(message or {})
            pipe = redis_client.pipeline(transaction=True)
            await pipe.rpush(messages_key, payload)
            await pipe.expire(messages_key, self.DEFAULT_TTL)
            await pipe.execute()
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Failed to append AI context message: {e}")

    async def clear_context(self, user_id: str, conversation_id: str):
        key = self.KEY_PATTERN.format(user_id=user_id, conversation_id=conversation_id)
        messages_key = self._messages_key(key)

        redis_client = self._get_redis_client()
        if not redis_client:
            return

        try:
            await redis_client.delete(key, messages_key)
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Failed to clear AI context: {e}")

    async def get_user_conversations(self, user_id: str) -> list[str]:
        """获取用户所有对话ID"""
        pattern = f"ai:context:{user_id}:*"

        redis_client = self._get_redis_client()
        if not redis_client:
            return []

        try:
            conversation_ids = set()
            async for raw_key in redis_client.scan_iter(pattern):
                key = raw_key.decode() if isinstance(raw_key, bytes) else str(raw_key)
                parts = key.split(":")
                if len(parts) < 4:
                    continue
                if parts[-1] == "messages":
                    conversation_ids.add(parts[-2])
                else:
                    conversation_ids.add(parts[-1])
            return sorted(conversation_ids)
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Failed to get user conversations: {e}")
            return []

    async def clear_user_contexts(self, user_id: str):
        """清除用户所有对话上下文"""
        pattern = f"ai:context:{user_id}:*"

        redis_client = self._get_redis_client()
        if not redis_client:
            return

        try:
            pipe = redis_client.pipeline(transaction=False)
            pending = 0
            async for key in redis_client.scan_iter(pattern):
                await pipe.delete(key)
                pending += 1
                if pending >= 500:
                    await pipe.execute()
                    pending = 0
            if pending > 0:
                await pipe.execute()
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"Failed to clear user contexts: {e}")
