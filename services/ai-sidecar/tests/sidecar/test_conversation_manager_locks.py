import pytest

from src.infrastructure.ai.conversation_manager import MemoryConversationManager


@pytest.mark.asyncio
async def test_cleanup_expired_conversations_prunes_orphaned_locks() -> None:
    manager = MemoryConversationManager()
    conversation = await manager.create_conversation(user_id="user-1")

    assert conversation.id in manager._conversation_locks

    async with manager._global_lock:
        manager._conversations.pop(conversation.id)

    deleted_count = await manager.cleanup_expired_conversations()

    assert deleted_count == 0
    assert conversation.id not in manager._conversation_locks
