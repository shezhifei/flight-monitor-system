"""验证 Redis conversation manager 不再使用 pickle。"""
import inspect

from src.infrastructure.ai.conversation_manager import manager as mgr_module


def test_redis_manager_source_has_no_pickle():
    source = inspect.getsource(mgr_module.RedisConversationManager)
    assert "import pickle" not in source
    assert "pickle.loads" not in source
    assert "pickle.dumps" not in source