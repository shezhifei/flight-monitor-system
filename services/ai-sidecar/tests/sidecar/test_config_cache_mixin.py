from src.infrastructure.ai.config.cache_mixin import ConfigCacheMixin


class CacheStore(ConfigCacheMixin):
    def __init__(self, ttl: int | None = None):
        self._init_cache(ttl)


def test_cache_returns_cached_entity_until_ttl_expires(monkeypatch):
    now = 100.0
    monkeypatch.setattr("src.infrastructure.ai.config.cache_mixin.time.monotonic", lambda: now)

    store = CacheStore(ttl=5)
    config = {"model": "gpt-4o"}

    store._set_cached("default", config)

    assert store._cache_valid("default") is True
    assert store._get_cached("default") == config

    now = 106.0

    assert store._cache_valid("default") is False


def test_invalidate_cache_can_clear_one_entity_or_all():
    store = CacheStore(ttl=300)
    store._set_cached("default", {"model": "gpt-4o"})
    store._set_cached("pilot", {"model": "gpt-4o-mini"})

    store._invalidate_cache("default")

    assert store._cache_valid("default") is False
    assert store._get_cached("default") is None
    assert store._cache_valid("pilot") is True

    store._invalidate_cache()

    assert store._cache_valid("pilot") is False
    assert store._get_cached("pilot") is None


def test_cache_ttl_can_come_from_environment(monkeypatch):
    now = 10.0
    monkeypatch.setenv("AI_CONFIG_CACHE_TTL", "2")
    monkeypatch.setattr("src.infrastructure.ai.config.cache_mixin.time.monotonic", lambda: now)

    store = CacheStore()
    store._set_cached("default", {"model": "gpt-4o"})

    now = 11.9
    assert store._cache_valid("default") is True

    now = 12.1
    assert store._cache_valid("default") is False
