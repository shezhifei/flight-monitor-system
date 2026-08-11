"""性能监控模块

提供系统性能监控和指标收集功能
"""

import asyncio
import contextlib
import time
from collections import deque
from dataclasses import dataclass, field
from functools import wraps
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


@dataclass
class PerformanceMetrics:
    """性能指标"""

    timestamp: float = field(default_factory=time.time)
    cpu_percent: float = 0.0
    memory_percent: float = 0.0
    memory_used_mb: float = 0.0
    active_connections: int = 0
    requests_per_second: float = 0.0
    avg_response_time_ms: float = 0.0
    p95_response_time_ms: float = 0.0
    p99_response_time_ms: float = 0.0
    error_rate: float = 0.0


@dataclass
class RequestMetrics:
    """请求指标"""

    path: str
    method: str
    status_code: int
    duration_ms: float
    timestamp: float = field(default_factory=time.time)


class PerformanceMonitor:
    """性能监控器"""

    def __init__(
        self,
        max_samples: int = 1000,
        collection_interval: float = 60.0,
    ):
        """
        初始化性能监控器

        Args:
            max_samples: 最大样本数
            collection_interval: 收集间隔（秒）
        """
        self._max_samples = max_samples
        self._collection_interval = collection_interval
        self._request_samples: deque[RequestMetrics] = deque(maxlen=max_samples)
        self._performance_samples: deque[PerformanceMetrics] = deque(maxlen=max_samples)
        self._collection_task: asyncio.Task | None = None
        self._started = False

    async def start(self) -> None:
        """启动性能监控"""
        if self._started:
            return

        self._started = True
        self._collection_task = asyncio.create_task(self._collection_loop())
        logger.info("Performance monitor started")

    async def stop(self) -> None:
        """停止性能监控"""
        if self._collection_task:
            self._collection_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._collection_task

        self._started = False
        logger.info("Performance monitor stopped")

    async def _collection_loop(self) -> None:
        """定期收集性能指标"""
        while True:
            try:
                await asyncio.sleep(self._collection_interval)
                await self._collect_metrics()
            except asyncio.CancelledError:
                break
            except Exception as e:  # noqa: BLE001 - background metrics loop must not die on any error
                logger.error(f"Performance metrics collection error: {e}")

    async def _collect_metrics(self) -> None:
        """收集性能指标"""
        try:
            import psutil

            # 收集系统指标
            cpu_percent = psutil.cpu_percent(interval=1)
            memory = psutil.virtual_memory()

            # 收集请求指标
            request_stats = self._calculate_request_stats()

            metrics = PerformanceMetrics(
                cpu_percent=cpu_percent,
                memory_percent=memory.percent,
                memory_used_mb=memory.used / (1024 * 1024),
                **request_stats,
            )

            self._performance_samples.append(metrics)

            logger.debug(
                f"Performance metrics collected: "
                f"CPU={cpu_percent}%, "
                f"Memory={memory.percent}%, "
                f"RPS={request_stats['requests_per_second']:.2f}"
            )
        except ImportError:
            logger.warning("psutil not installed, skipping system metrics collection")
        except Exception as e:  # noqa: BLE001 - psutil metrics collection may raise arbitrary errors
            logger.error(f"Failed to collect performance metrics: {e}")

    def _calculate_request_stats(self) -> dict[str, float]:
        """计算请求统计"""
        if not self._request_samples:
            return {
                "requests_per_second": 0.0,
                "avg_response_time_ms": 0.0,
                "p95_response_time_ms": 0.0,
                "p99_response_time_ms": 0.0,
                "error_rate": 0.0,
            }

        # 计算最近一分钟的统计
        now = time.time()
        recent_requests = [r for r in self._request_samples if now - r.timestamp < 60]

        if not recent_requests:
            return {
                "requests_per_second": 0.0,
                "avg_response_time_ms": 0.0,
                "p95_response_time_ms": 0.0,
                "p99_response_time_ms": 0.0,
                "error_rate": 0.0,
            }

        # 计算请求速率
        time_span = now - recent_requests[0].timestamp
        requests_per_second = len(recent_requests) / max(time_span, 1)

        # 计算响应时间统计
        durations = [r.duration_ms for r in recent_requests]
        avg_duration = sum(durations) / len(durations)

        sorted_durations = sorted(durations)
        p95_idx = int(len(sorted_durations) * 0.95)
        p99_idx = int(len(sorted_durations) * 0.99)
        p95_duration = sorted_durations[p95_idx]
        p99_duration = sorted_durations[p99_idx]

        # 计算错误率
        error_count = sum(1 for r in recent_requests if r.status_code >= 400)
        error_rate = error_count / len(recent_requests)

        return {
            "requests_per_second": requests_per_second,
            "avg_response_time_ms": avg_duration,
            "p95_response_time_ms": p95_duration,
            "p99_response_time_ms": p99_duration,
            "error_rate": error_rate,
        }

    def record_request(
        self,
        path: str,
        method: str,
        status_code: int,
        duration_ms: float,
    ) -> None:
        """记录请求指标"""
        metrics = RequestMetrics(
            path=path,
            method=method,
            status_code=status_code,
            duration_ms=duration_ms,
        )
        self._request_samples.append(metrics)

    def get_current_metrics(self) -> dict[str, Any]:
        """获取当前性能指标"""
        request_stats = self._calculate_request_stats()

        return {
            "timestamp": time.time(),
            "requests": request_stats,
            "samples_count": len(self._request_samples),
            "performance_samples_count": len(self._performance_samples),
        }

    def get_historical_metrics(
        self,
        duration_seconds: int = 3600,
    ) -> list[dict[str, Any]]:
        """获取历史性能指标"""
        now = time.time()
        cutoff = now - duration_seconds

        return [
            {
                "timestamp": m.timestamp,
                "cpu_percent": m.cpu_percent,
                "memory_percent": m.memory_percent,
                "memory_used_mb": m.memory_used_mb,
                "requests_per_second": m.requests_per_second,
                "avg_response_time_ms": m.avg_response_time_ms,
                "p95_response_time_ms": m.p95_response_time_ms,
                "p99_response_time_ms": m.p99_response_time_ms,
                "error_rate": m.error_rate,
            }
            for m in self._performance_samples
            if m.timestamp >= cutoff
        ]


def track_performance(monitor: PerformanceMonitor):
    """
    性能追踪装饰器

    Args:
        monitor: 性能监控器实例
    """

    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            start_time = time.time()
            status_code = 200

            try:
                result = await func(*args, **kwargs)
                return result
            except Exception:
                status_code = 500
                raise
            finally:
                duration_ms = (time.time() - start_time) * 1000
                # 从参数中提取路径和方法（如果有的话）
                path = kwargs.get("path", func.__name__)
                method = kwargs.get("method", "UNKNOWN")
                monitor.record_request(path, method, status_code, duration_ms)

        return wrapper

    return decorator


class DatabasePerformanceMonitor:
    """数据库性能监控器"""

    def __init__(self, max_samples: int = 1000):
        """
        初始化数据库性能监控器

        Args:
            max_samples: 最大样本数
        """
        self._max_samples = max_samples
        self._query_samples: deque[dict[str, Any]] = deque(maxlen=max_samples)
        self._lock = asyncio.Lock()

    async def record_query(
        self,
        query: str,
        duration_ms: float,
        rows_affected: int = 0,
        error: str | None = None,
    ) -> None:
        """记录查询指标"""
        async with self._lock:
            self._query_samples.append(
                {
                    "query": query[:100],  # 截断长查询
                    "duration_ms": duration_ms,
                    "rows_affected": rows_affected,
                    "error": error,
                    "timestamp": time.time(),
                }
            )

    async def get_stats(self) -> dict[str, Any]:
        """获取查询统计"""
        async with self._lock:
            if not self._query_samples:
                return {
                    "total_queries": 0,
                    "avg_duration_ms": 0.0,
                    "p95_duration_ms": 0.0,
                    "p99_duration_ms": 0.0,
                    "error_rate": 0.0,
                    "slow_queries": [],
                }

            durations = [s["duration_ms"] for s in self._query_samples]
            errors = [s for s in self._query_samples if s["error"]]

            sorted_durations = sorted(durations)
            p95_idx = int(len(sorted_durations) * 0.95)
            p99_idx = int(len(sorted_durations) * 0.99)

            # 获取慢查询（超过 100ms）
            slow_queries = [s for s in self._query_samples if s["duration_ms"] > 100][-10:]  # 保留最近 10 条

            return {
                "total_queries": len(self._query_samples),
                "avg_duration_ms": sum(durations) / len(durations),
                "p95_duration_ms": sorted_durations[p95_idx],
                "p99_duration_ms": sorted_durations[p99_idx],
                "error_rate": len(errors) / len(self._query_samples),
                "slow_queries": slow_queries,
            }


class CachePerformanceMonitor:
    """缓存性能监控器"""

    def __init__(self):
        """初始化缓存性能监控器"""
        self._stats = {
            "hits": 0,
            "misses": 0,
            "sets": 0,
            "deletes": 0,
            "errors": 0,
        }
        self._lock = asyncio.Lock()

    async def record_hit(self) -> None:
        """记录缓存命中"""
        async with self._lock:
            self._stats["hits"] += 1

    async def record_miss(self) -> None:
        """记录缓存未命中"""
        async with self._lock:
            self._stats["misses"] += 1

    async def record_set(self) -> None:
        """记录缓存设置"""
        async with self._lock:
            self._stats["sets"] += 1

    async def record_delete(self) -> None:
        """记录缓存删除"""
        async with self._lock:
            self._stats["deletes"] += 1

    async def record_error(self) -> None:
        """记录缓存错误"""
        async with self._lock:
            self._stats["errors"] += 1

    async def get_stats(self) -> dict[str, Any]:
        """获取缓存统计"""
        async with self._lock:
            total_requests = self._stats["hits"] + self._stats["misses"]
            hit_rate = self._stats["hits"] / total_requests if total_requests > 0 else 0.0
            return {
                **self._stats,
                "total_requests": total_requests,
                "hit_rate": hit_rate,
            }


# 全局性能监控器实例
performance_monitor = PerformanceMonitor()
db_performance_monitor = DatabasePerformanceMonitor()
cache_performance_monitor = CachePerformanceMonitor()
