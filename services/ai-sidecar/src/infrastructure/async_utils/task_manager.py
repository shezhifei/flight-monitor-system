"""异步任务管理器

提供异步任务的管理和优化功能
"""

import asyncio
import contextlib
import time
from collections import deque
from collections.abc import Callable, Coroutine
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


@dataclass
class TaskInfo:
    """任务信息"""

    task_id: str
    name: str
    created_at: float = field(default_factory=time.time)
    started_at: float | None = None
    completed_at: float | None = None
    status: str = "pending"  # pending, running, completed, failed, cancelled
    result: Any = None
    error: str | None = None
    _task: asyncio.Task | None = field(default=None, repr=False)


class TaskManager:
    """异步任务管理器"""

    def __init__(
        self,
        max_concurrent_tasks: int = 100,
        task_timeout: float = 300.0,
        cleanup_interval: float = 60.0,
    ):
        """
        初始化任务管理器

        Args:
            max_concurrent_tasks: 最大并发任务数
            task_timeout: 任务超时时间（秒）
            cleanup_interval: 清理间隔（秒）
        """
        self._max_concurrent_tasks = max_concurrent_tasks
        self._task_timeout = task_timeout
        self._cleanup_interval = cleanup_interval

        self._tasks: dict[str, TaskInfo] = {}
        self._running_tasks: set[str] = set()
        self._semaphore = asyncio.Semaphore(max_concurrent_tasks)
        self._lock = asyncio.Lock()
        self._cleanup_task: asyncio.Task | None = None
        self._task_counter = 0

    async def start(self) -> None:
        """启动任务管理器"""
        if self._cleanup_task is None or self._cleanup_task.done():
            self._cleanup_task = asyncio.create_task(self._cleanup_loop())
            logger.info("Task manager started")

    async def stop(self) -> None:
        """停止任务管理器"""
        if self._cleanup_task:
            self._cleanup_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._cleanup_task

        # 等待所有运行中的任务完成
        if self._running_tasks:
            logger.info(f"Waiting for {len(self._running_tasks)} tasks to complete...")
            await asyncio.gather(
                *[self._wait_task(task_id) for task_id in self._running_tasks],
                return_exceptions=True,
            )

        logger.info("Task manager stopped")

    async def _wait_task(self, task_id: str) -> None:
        """等待任务完成"""
        async with self._lock:
            task_info = self._tasks.get(task_id)
            if task_info and task_info.status == "running":
                # 这里只是等待，实际的任务引用需要额外存储
                pass

    async def _cleanup_loop(self) -> None:
        """定期清理已完成的任务"""
        while True:
            try:
                await asyncio.sleep(self._cleanup_interval)
                await self._cleanup_completed_tasks()
            except asyncio.CancelledError:
                break
            except Exception as e:  # noqa: BLE001 - background cleanup loop must not die on any error
                logger.error(f"Task cleanup error: {e}")

    async def _cleanup_completed_tasks(self) -> None:
        """清理已完成的任务"""
        async with self._lock:
            now = time.time()
            expired_tasks = [
                task_id
                for task_id, task_info in self._tasks.items()
                if task_info.status in ("completed", "failed", "cancelled")
                and now - task_info.completed_at > 3600  # 保留 1 小时
            ]
            for task_id in expired_tasks:
                del self._tasks[task_id]

            if expired_tasks:
                logger.debug(f"Cleaned up {len(expired_tasks)} completed tasks")

    def _generate_task_id(self) -> str:
        """生成任务 ID"""
        self._task_counter += 1
        return f"task_{self._task_counter}_{int(time.time())}"

    async def submit_task(
        self,
        name: str,
        coro: Coroutine,
        priority: int = 0,
    ) -> str:
        """
        提交异步任务

        Args:
            name: 任务名称
            coro: 协程对象
            priority: 优先级（未使用，预留）

        Returns:
            任务 ID
        """
        task_id = self._generate_task_id()

        async with self._lock:
            self._tasks[task_id] = TaskInfo(
                task_id=task_id,
                name=name,
            )

        # 创建包装后的协程
        wrapped_coro = self._execute_task(task_id, coro)
        task = asyncio.create_task(wrapped_coro)

        # 存储任务引用以便取消
        async with self._lock:
            self._tasks[task_id]._task = task

        return task_id

    async def _execute_task(self, task_id: str, coro: Coroutine) -> Any:
        """执行任务"""
        async with self._semaphore:
            async with self._lock:
                task_info = self._tasks.get(task_id)
                if task_info:
                    task_info.status = "running"
                    task_info.started_at = time.time()
                    self._running_tasks.add(task_id)

            try:
                # 添加超时控制
                result = await asyncio.wait_for(coro, timeout=self._task_timeout)

                async with self._lock:
                    task_info = self._tasks.get(task_id)
                    if task_info:
                        task_info.status = "completed"
                        task_info.result = result
                        task_info.completed_at = time.time()
                        self._running_tasks.discard(task_id)

                return result

            except TimeoutError:
                async with self._lock:
                    task_info = self._tasks.get(task_id)
                    if task_info:
                        task_info.status = "failed"
                        task_info.error = "Task timed out"
                        task_info.completed_at = time.time()
                        self._running_tasks.discard(task_id)

                logger.error(f"Task {task_id} ({task_info.name}) timed out")

            except asyncio.CancelledError:
                async with self._lock:
                    task_info = self._tasks.get(task_id)
                    if task_info:
                        task_info.status = "cancelled"
                        task_info.completed_at = time.time()
                        self._running_tasks.discard(task_id)

                logger.info(f"Task {task_id} ({task_info.name}) cancelled")

            except Exception as e:  # noqa: BLE001 - task runner must catch all errors to mark task as failed
                async with self._lock:
                    task_info = self._tasks.get(task_id)
                    if task_info:
                        task_info.status = "failed"
                        task_info.error = str(e)
                        task_info.completed_at = time.time()
                        self._running_tasks.discard(task_id)

                logger.error(f"Task {task_id} ({task_info.name}) failed: {e}")

    async def cancel_task(self, task_id: str) -> bool:
        """
        取消任务

        Args:
            task_id: 任务 ID

        Returns:
            是否成功取消
        """
        async with self._lock:
            task_info = self._tasks.get(task_id)
            if not task_info:
                return False

            if task_info.status != "running":
                return False

            # 获取任务引用并取消
            task = task_info._task
            if task:
                task.cancel()
                return True

            return False

    async def get_task_status(self, task_id: str) -> dict[str, Any] | None:
        """
        获取任务状态

        Args:
            task_id: 任务 ID

        Returns:
            任务状态信息
        """
        async with self._lock:
            task_info = self._tasks.get(task_id)
            if not task_info:
                return None

            return {
                "task_id": task_info.task_id,
                "name": task_info.name,
                "status": task_info.status,
                "created_at": task_info.created_at,
                "started_at": task_info.started_at,
                "completed_at": task_info.completed_at,
                "error": task_info.error,
            }

    async def get_all_tasks(self) -> list[dict[str, Any]]:
        """获取所有任务状态"""
        async with self._lock:
            return [
                {
                    "task_id": task_info.task_id,
                    "name": task_info.name,
                    "status": task_info.status,
                    "created_at": task_info.created_at,
                    "started_at": task_info.started_at,
                    "completed_at": task_info.completed_at,
                }
                for task_info in self._tasks.values()
            ]

    async def get_stats(self) -> dict[str, Any]:
        """获取任务统计信息"""
        async with self._lock:
            status_counts = {}
            for task_info in self._tasks.values():
                status_counts[task_info.status] = status_counts.get(task_info.status, 0) + 1

            return {
                "total_tasks": len(self._tasks),
                "running_tasks": len(self._running_tasks),
                "max_concurrent_tasks": self._max_concurrent_tasks,
                "status_counts": status_counts,
            }


class BatchProcessor:
    """批量处理器"""

    def __init__(
        self,
        batch_size: int = 100,
        flush_interval: float = 1.0,
        max_queue_size: int = 10000,
    ):
        """
        初始化批量处理器

        Args:
            batch_size: 批次大小
            flush_interval: 刷新间隔（秒）
            max_queue_size: 最大队列大小
        """
        self._batch_size = batch_size
        self._flush_interval = flush_interval
        self._max_queue_size = max_queue_size

        self._queue: deque = deque(maxlen=max_queue_size)
        self._processor: Callable | None = None
        self._flush_task: asyncio.Task | None = None
        self._lock = asyncio.Lock()
        self._stats = {
            "total_items": 0,
            "total_batches": 0,
            "total_errors": 0,
        }

    def set_processor(self, processor: Callable) -> None:
        """设置处理器函数"""
        self._processor = processor

    async def start(self) -> None:
        """启动批量处理器"""
        if self._flush_task is None or self._flush_task.done():
            self._flush_task = asyncio.create_task(self._flush_loop())
            logger.info("Batch processor started")

    async def stop(self) -> None:
        """停止批量处理器"""
        if self._flush_task:
            self._flush_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._flush_task

        # 处理剩余的项目
        await self._flush()
        logger.info("Batch processor stopped")

    async def _flush_loop(self) -> None:
        """定期刷新队列"""
        while True:
            try:
                await asyncio.sleep(self._flush_interval)
                await self._flush()
            except asyncio.CancelledError:
                break
            except Exception as e:  # noqa: BLE001 - background flush loop must not die on any error
                logger.error(f"Batch flush error: {e}")

    async def _flush(self) -> None:
        """刷新队列"""
        if not self._processor:
            return

        async with self._lock:
            if not self._queue:
                return

            # 取出批次数据
            batch = []
            while self._queue and len(batch) < self._batch_size:
                batch.append(self._queue.popleft())

        if batch:
            try:
                await self._processor(batch)
                self._stats["total_batches"] += 1
                self._stats["total_items"] += len(batch)
            except Exception as e:  # noqa: BLE001 - batch processor may raise arbitrary errors
                logger.error(f"Batch processing error: {e}")
                self._stats["total_errors"] += 1

                # 将失败的项目放回队列
                async with self._lock:
                    for item in reversed(batch):
                        self._queue.appendleft(item)

    async def add_item(self, item: Any) -> bool:
        """
        添加项目到队列

        Args:
            item: 要处理的项目

        Returns:
            是否成功添加
        """
        async with self._lock:
            if len(self._queue) >= self._max_queue_size:
                logger.warning("Batch processor queue is full, dropping item")
                return False

            self._queue.append(item)
            return True

    async def add_items(self, items: list[Any]) -> int:
        """
        批量添加项目到队列

        Args:
            items: 要处理的项目列表

        Returns:
            成功添加的项目数
        """
        added = 0
        async with self._lock:
            for item in items:
                if len(self._queue) < self._max_queue_size:
                    self._queue.append(item)
                    added += 1
                else:
                    break
        return added

    async def get_stats(self) -> dict[str, Any]:
        """获取统计信息"""
        async with self._lock:
            return {
                **self._stats,
                "queue_size": len(self._queue),
                "max_queue_size": self._max_queue_size,
            }


class RateLimiter:
    """速率限制器"""

    def __init__(
        self,
        max_requests: int = 100,
        time_window: float = 60.0,
    ):
        """
        初始化速率限制器

        Args:
            max_requests: 时间窗口内最大请求数
            time_window: 时间窗口（秒）
        """
        self._max_requests = max_requests
        self._time_window = time_window
        self._requests: deque = deque()
        self._lock = asyncio.Lock()

    async def acquire(self) -> bool:
        """
        获取请求许可

        Returns:
            是否允许请求
        """
        async with self._lock:
            now = time.time()

            # 清理过期的请求记录
            while self._requests and self._requests[0] < now - self._time_window:
                self._requests.popleft()

            # 检查是否超过限制
            if len(self._requests) >= self._max_requests:
                return False

            self._requests.append(now)
            return True

    async def wait(self, timeout: float | None = None) -> bool:
        """
        等待获取请求许可

        Args:
            timeout: 超时时间（秒）

        Returns:
            是否成功获取许可
        """
        start_time = time.time()
        while True:
            if await self.acquire():
                return True

            if timeout and time.time() - start_time >= timeout:
                return False

            # 计算需要等待的时间
            async with self._lock:
                if self._requests:
                    wait_time = self._requests[0] + self._time_window - time.time()
                    wait_time = max(0.1, min(wait_time, 1.0))
                else:
                    wait_time = 0.1

            await asyncio.sleep(wait_time)

    async def get_stats(self) -> dict[str, Any]:
        """获取统计信息"""
        async with self._lock:
            now = time.time()
            # 清理过期的请求记录
            while self._requests and self._requests[0] < now - self._time_window:
                self._requests.popleft()

            return {
                "current_requests": len(self._requests),
                "max_requests": self._max_requests,
                "time_window": self._time_window,
                "remaining_requests": max(0, self._max_requests - len(self._requests)),
            }


# 全局任务管理器实例
task_manager = TaskManager()
batch_processor = BatchProcessor()
