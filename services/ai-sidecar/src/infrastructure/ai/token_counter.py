"""
令牌计数工具

提供对消息和文本进行令牌计数的功能。
使用 tiktoken 进行精确计数（项目标准依赖）。
"""

import hashlib
import math
import time
from collections import deque
from functools import lru_cache
from typing import Any

try:
    import tiktoken
except ModuleNotFoundError:  # pragma: no cover - exercised in dependency-minimal environments
    tiktoken = None  # optional dependency

from src.infrastructure.logging.core import get_logger

from .openai_client import ContentPart, ContentPartType, Message, MessageRole

logger = get_logger(__name__)

# ---------------------------------------------------------------------------
# 编码器缓存
# ---------------------------------------------------------------------------
_encoder_cache: dict[str, Any] = {}
_fallback_warning_emitted = False

# ---------------------------------------------------------------------------
# 消息级缓存 (TTL: 5 分钟, 最大 1000 条)
# ---------------------------------------------------------------------------
_message_cache: dict[str, tuple[int, float]] = {}
_MESSAGE_CACHE_MAXSIZE = 1000
_MESSAGE_CACHE_TTL = 300  # 5 分钟


class _ApproximateEncoding:
    """Fallback tokenizer when tiktoken is unavailable.

    Uses a conservative heuristic that accounts for CJK characters
    (~1-2 tokens/char) vs ASCII (~4 chars/token) to avoid severe undercount.
    """

    @staticmethod
    def encode(text: str) -> list[int]:
        if not text:
            return []
        cjk_chars = sum(
            1
            for ch in text
            if "\u4e00" <= ch <= "\u9fff"
            or "\u3040" <= ch <= "\u309f"
            or "\u30a0" <= ch <= "\u30ff"
            or "\uac00" <= ch <= "\ud7af"
        )
        non_cjk_chars = len(text) - cjk_chars
        approximate_tokens = max(1, math.ceil(cjk_chars * 1.5 + non_cjk_chars / 4))
        return [0] * approximate_tokens


def _build_message_cache_key(messages: list[Message], model: str) -> str:
    """构建消息缓存键（基于消息内容摘要和模型）。"""
    parts = []
    for msg in messages:
        role = msg.role.value if isinstance(msg.role, MessageRole) else msg.role
        content = ""
        if isinstance(msg.content, str):
            content = msg.content[:100]
        elif isinstance(msg.content, list):
            for part in msg.content:
                if isinstance(part, dict) and part.get("type") == "text":
                    content = part.get("text", "")[:100]
                    break
                elif isinstance(part, ContentPart) and part.type == ContentPartType.TEXT:
                    content = (part.text or "")[:100]
                    break
        parts.append(f"{role}:{content}")
    raw = f"{model}|{'||'.join(parts)}"
    return hashlib.md5(raw.encode()).hexdigest()


def _get_from_message_cache(key: str) -> int | None:
    """从消息缓存获取值，过期则删除。"""
    if key in _message_cache:
        value, ts = _message_cache[key]
        if time.monotonic() - ts < _MESSAGE_CACHE_TTL:
            return value
        del _message_cache[key]
    return None


def _put_to_message_cache(key: str, value: int) -> None:
    """写入消息缓存，超限时清理最旧条目。"""
    if len(_message_cache) >= _MESSAGE_CACHE_MAXSIZE:
        oldest_key = min(_message_cache, key=lambda k: _message_cache[k][1])
        del _message_cache[oldest_key]
    _message_cache[key] = (value, time.monotonic())


def _get_encoder(model: str) -> Any:
    """获取 tiktoken 编码器（带缓存）。"""
    global _fallback_warning_emitted
    if model in _encoder_cache:
        return _encoder_cache[model]

    if tiktoken is None:
        if not _fallback_warning_emitted:
            logger.warning("tiktoken is not installed; TokenCounter is using approximate tokenizer fallback")
            _fallback_warning_emitted = True
        encoding = _ApproximateEncoding()
        _encoder_cache[model] = encoding
        return encoding

    try:
        encoding = tiktoken.encoding_for_model(model)
    except KeyError:
        logger.warning(f"Model '{model}' not found in tiktoken, using cl100k_base encoding")
        encoding = tiktoken.get_encoding("cl100k_base")

    _encoder_cache[model] = encoding
    return encoding


@lru_cache(maxsize=2048)
def _cached_encode_length(text: str, model: str) -> int:
    """缓存 token 编码结果，返回 token 数量"""
    encoder = _get_encoder(model)
    return len(encoder.encode(text))


# ---------------------------------------------------------------------------
# 模型上下文窗口查询
# ---------------------------------------------------------------------------
# 仅保留无法通过 tiktoken 自动解析且项目中实际使用的非 OpenAI 模型。
# OpenAI 模型直接查 tiktoken 或使用保守默认值。
_NON_OPENAI_CONTEXT_WINDOWS = {
    "claude-3-opus": 200_000,
    "claude-3-sonnet": 200_000,
    "claude-3-haiku": 200_000,
    "claude-2": 100_000,
    "claude-instant": 100_000,
    "gemini-1.5-pro": 1_000_000,
    "gemini-1.5-flash": 1_000_000,
    "gemini-1.0-pro": 30_720,
}


def get_model_context_window(model: str) -> int:
    """获取模型的上下文窗口大小（令牌数）。

    对 OpenAI 模型通过 tiktoken 内置映射推断；
    对非 OpenAI 模型从维护表中查找；
    未知模型返回 4096 保守默认值。
    """
    # 非 OpenAI 模型精确匹配
    if model in _NON_OPENAI_CONTEXT_WINDOWS:
        return _NON_OPENAI_CONTEXT_WINDOWS[model]

    # 非 OpenAI 前缀匹配
    for key, value in _NON_OPENAI_CONTEXT_WINDOWS.items():
        if model.startswith(key + "-"):
            return value

    # OpenAI 模型：基于已知模式给出合理值
    if "gpt-4o" in model or "gpt-4-turbo" in model or model.startswith("o1") or model.startswith("o3"):
        return 128_000
    if "gpt-4" in model:
        return 8_192
    if "gpt-3.5" in model:
        return 16_385
    if "claude" in model:
        return 100_000
    if "gemini" in model:
        return 30_720

    logger.warning(f"Unknown model '{model}', using default context window of 4096 tokens")
    return 4_096


# ---------------------------------------------------------------------------
# TokenCounter
# ---------------------------------------------------------------------------


class TokenCounter:
    """令牌计数器，使用 tiktoken 进行精确计数。"""

    def __init__(self, default_model: str = "gpt-3.5-turbo"):
        self.default_model = default_model

    # -- 文本计数 --

    def count_tokens(self, text: str, model: str | None = None) -> int:
        """计算给定文本的令牌数。"""
        if not text or not text.strip():
            return 0

        encoder = _get_encoder(model or self.default_model)
        return len(encoder.encode(text))

    # -- 图像令牌 (OpenAI Vision 计费规则) --

    @staticmethod
    def _calculate_image_tokens(width: int, height: int, detail: str = "auto") -> int:
        """计算图像令牌数 (OpenAI Vision 计费规则)。"""
        if detail == "low":
            return 85

        # 缩放：长边最大 2048px
        if width > 2048 or height > 2048:
            ratio = min(2048 / width, 2048 / height)
            width = int(width * ratio)
            height = int(height * ratio)

        # 缩放：短边最大 768px
        if width > 768 and height > 768:
            ratio = 768 / min(width, height)
            width = int(width * ratio)
            height = int(height * ratio)

        # 512px 网格
        h_grids = math.ceil(width / 512)
        v_grids = math.ceil(height / 512)
        return h_grids * v_grids * 170 + 85

    # -- 消息计数 --

    def count_message_tokens(self, message: Message, model: str | None = None) -> int:
        """计算单条消息的令牌数，包括角色、内容和元数据。"""
        model = model or self.default_model

        # 角色
        role_text = message.role.value if isinstance(message.role, MessageRole) else message.role
        tokens = self.count_tokens(role_text, model)

        # 内容
        if isinstance(message.content, str):
            tokens += self.count_tokens(message.content, model)
        elif isinstance(message.content, list):
            for part in message.content:
                if isinstance(part, dict):
                    part_type = part.get("type")
                    if part_type == "text":
                        tokens += self.count_tokens(part.get("text", ""), model)
                    elif part_type == "image_url":
                        detail = part.get("image_url", {}).get("detail", "auto")
                        tokens += 85 if detail == "low" else 255
                elif isinstance(part, ContentPart):
                    if part.type == ContentPartType.TEXT and part.text:
                        tokens += self.count_tokens(part.text, model)
                    elif part.type == ContentPartType.IMAGE_URL:
                        detail = part.image_url.get("detail", "auto") if part.image_url else "auto"
                        tokens += 85 if detail == "low" else 255

        # name
        if message.name:
            tokens += self.count_tokens(message.name, model)

        # tool_calls / tool_call_id 开销
        if message.tool_calls:
            tokens += len(message.tool_calls) * 10
        if message.tool_call_id:
            tokens += 5

        return tokens

    def count_messages_tokens(self, messages: list[Message], model: str | None = None) -> int:
        """计算消息列表的总令牌数。"""
        model = model or self.default_model
        cache_key = _build_message_cache_key(messages, model)

        cached = _get_from_message_cache(cache_key)
        if cached is not None:
            return cached

        total = sum(self.count_message_tokens(msg, model) for msg in messages)
        _put_to_message_cache(cache_key, total)
        return total

    # -- 消息截断 --

    def truncate_messages_to_fit(
        self,
        messages: list[Message],
        max_tokens: int,
        model: str | None = None,
        strategy: str = "remove_oldest",
    ) -> list[Message]:
        """截断消息列表以适配最大令牌限制。

        策略:
          - "remove_oldest": 从开头移除最旧的消息（默认）
          - "remove_newest": 从末尾移除最新的消息
          - "remove_system_last": 保留系统消息，移除其他消息
        """
        if not messages:
            return messages

        model = model or self.default_model

        per_msg_tokens = [self.count_message_tokens(m, model) for m in messages]
        total_tokens = sum(per_msg_tokens)

        if total_tokens <= max_tokens:
            return messages

        if strategy == "remove_oldest":
            truncated = deque(messages)
            token_iter = iter(per_msg_tokens)
            while truncated and total_tokens > max_tokens:
                truncated.popleft()
                total_tokens -= next(token_iter, 0)
            return list(truncated)

        elif strategy == "remove_newest":
            truncated = list(messages)
            while truncated and total_tokens > max_tokens:
                total_tokens -= per_msg_tokens.pop()
                truncated.pop()
            return truncated

        elif strategy == "remove_system_last":
            system_msgs = [msg for msg in messages if msg.role == MessageRole.SYSTEM]
            other_msgs = [msg for msg in messages if msg.role != MessageRole.SYSTEM]
            remaining_budget = max_tokens - self.count_messages_tokens(system_msgs, model)
            truncated_other = self.truncate_messages_to_fit(other_msgs, remaining_budget, model, "remove_oldest")
            return system_msgs + truncated_other

        else:
            logger.warning(f"Unknown truncation strategy '{strategy}', using 'remove_oldest'")
            return self.truncate_messages_to_fit(messages, max_tokens, model, "remove_oldest")


# ---------------------------------------------------------------------------
# 全局默认实例 & 便利函数
# ---------------------------------------------------------------------------
_default_counter = TokenCounter()


def count_tokens(text: str, model: str | None = None) -> int:
    """计算给定文本的令牌数。"""
    if not text or not text.strip():
        return 0
    return _cached_encode_length(text, model or _default_counter.default_model)


def count_message_tokens(message: Message, model: str | None = None) -> int:
    """计算单条消息的令牌数"""
    return _default_counter.count_message_tokens(message, model)


def count_messages_tokens(messages: list[Message], model: str | None = None) -> int:
    """计算消息列表的总令牌数"""
    return _default_counter.count_messages_tokens(messages, model)


def truncate_messages_to_fit(
    messages: list[Message],
    max_tokens: int,
    model: str | None = None,
    strategy: str = "remove_oldest",
) -> list[Message]:
    """截断消息列表以适配最大令牌限制"""
    return _default_counter.truncate_messages_to_fit(messages, max_tokens, model, strategy)


def clear_token_cache() -> None:
    """清除 token 计数缓存"""
    _cached_encode_length.cache_clear()
    _message_cache.clear()
