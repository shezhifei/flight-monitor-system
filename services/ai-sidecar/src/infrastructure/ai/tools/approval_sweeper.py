"""Background sweeper that expires stale pending actions.

Runs as an asyncio background task, periodically scanning for pending
actions whose TTL has elapsed and marking them as ``expired``.
"""

from __future__ import annotations

import asyncio
import contextlib
from typing import TYPE_CHECKING

from src.domain.utils.time_utils import utc_now
from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    pass

logger = get_logger(__name__)

DEFAULT_SWEEP_INTERVAL_SECONDS = 30


class ApprovalSweeper:
    """Periodically marks expired pending actions.

    Usage::

        sweeper = ApprovalSweeper(interval_seconds=30)
        await sweeper.start()   # Non-blocking; spawns background task
        ...
        await sweeper.stop()
    """

    def __init__(
        self,
        *,
        interval_seconds: int = DEFAULT_SWEEP_INTERVAL_SECONDS,
        sse_broadcast_fn=None,
    ):
        self._interval = max(5, int(interval_seconds))
        self._sse_broadcast_fn = sse_broadcast_fn
        self._task: asyncio.Task | None = None
        self._running = False

    async def start(self) -> None:
        """Start the background sweep loop."""
        if self._running:
            return
        self._running = True
        self._task = asyncio.create_task(self._loop(), name="approval-sweeper")
        logger.info(f"ApprovalSweeper started (interval={self._interval}s)")

    async def stop(self) -> None:
        """Gracefully stop the sweeper."""
        self._running = False
        if self._task is not None:
            self._task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._task
            self._task = None
        logger.info("ApprovalSweeper stopped")

    async def _loop(self) -> None:
        while self._running:
            try:
                await asyncio.sleep(self._interval)
                await self._sweep()
            except asyncio.CancelledError:
                break
            except Exception as exc:  # noqa: BLE001 - background sweeper loop must not die on any error
                logger.warning(f"ApprovalSweeper sweep error: {exc}", exc_info=True)
                await asyncio.sleep(self._interval)

    async def _sweep(self) -> None:
        from src.infrastructure.ai.tools.pending_actions import get_pending_action_store

        store = get_pending_action_store()
        now = utc_now()

        try:
            expired_actions = await store.expire_stale_actions(now)
        except Exception as exc:  # noqa: BLE001 - store operation may raise arbitrary errors
            logger.warning(f"expire_stale_actions failed: {exc}")
            return

        if not expired_actions:
            return

        logger.info(f"ApprovalSweeper: expired {len(expired_actions)} action(s)")

        # Broadcast SSE events for each expired action
        if self._sse_broadcast_fn is not None:
            for action in expired_actions:
                try:
                    await self._sse_broadcast_fn(
                        "action_expired",
                        {
                            "event": "approval_expired",
                            "action_id": action.action_id,
                            "tool_name": action.tool_name,
                            "status": "expired",
                            "expires_at": (
                                action.expires_at.isoformat()
                                if hasattr(action.expires_at, "isoformat")
                                else str(action.expires_at)
                            ),
                            "pending_action": action.to_dict(),
                        },
                    )
                except Exception as exc:  # noqa: BLE001 - SSE broadcast must not abort sweep
                    logger.warning(f"Failed to broadcast action_expired SSE for {action.action_id}: {exc}")


# ── Module-level singleton ─────────────────────────────────────

_sweeper: ApprovalSweeper | None = None


def get_approval_sweeper() -> ApprovalSweeper | None:
    return _sweeper


def set_approval_sweeper(sweeper: ApprovalSweeper | None) -> None:
    global _sweeper
    _sweeper = sweeper


__all__ = [
    "DEFAULT_SWEEP_INTERVAL_SECONDS",
    "ApprovalSweeper",
    "get_approval_sweeper",
    "set_approval_sweeper",
]
