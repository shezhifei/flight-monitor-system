"""验证 PostgresAIConfigStore 使用 ConfigEncryptor 而非内联加密逻辑。"""

import inspect

from src.infrastructure.ai.postgres_config_store import PostgresAIConfigStore


def test_postgres_config_store_uses_config_encryptor():
    init_source = inspect.getsource(PostgresAIConfigStore.__init__)
    assert "_encryptor" in init_source or "encryptor" in init_source


def test_postgres_config_store_has_no_inline_fernet():
    assert not hasattr(PostgresAIConfigStore, "_init_fernet")
    assert not hasattr(PostgresAIConfigStore, "_encrypt_config")
    assert not hasattr(PostgresAIConfigStore, "_decrypt_config")
