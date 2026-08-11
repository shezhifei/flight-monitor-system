"""Responses API session-state manager.

Tracks ``previous_response_id`` for Responses-API session chaining
and validates fingerprints to auto-reset when the call configuration
changes (model, api_format, system_prompt, tools).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.feature_flags import is_ai_feature_enabled
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


@dataclass
class SessionFingerprints:
    """Immutable snapshot of the call-config that a session was created for."""

    model: str = ""
    api_format: str = ""
    system_prompt_hash: str = ""
    tools_hash: str = ""


@dataclass
class ResponsesSessionData:
    """In-memory / serialisable session-chain state."""

    previous_response_id: str | None = None
    synced_message_count: int = 0
    fingerprints: SessionFingerprints = field(default_factory=SessionFingerprints)
    reset_reason: str | None = None


# ---------------------------------------------------------------------------
# State manager
# ---------------------------------------------------------------------------


class ResponsesSessionStateManager:
    """Manage ``previous_response_id`` lifecycle.

    For *persistent* conversations the state is stored inside
    ``conversation.metadata.custom_data["llm"]["responses_session"]``
    via ``ConversationManager.merge_custom_data``.

    For *in-memory only* use cases (e.g. ``TodoAgentExecutor``)
    the caller can use :meth:`get_state` / :meth:`advance_state` /
    :meth:`reset_state` directly.
    """

    CUSTOM_DATA_NAMESPACE = "llm"
    CUSTOM_DATA_KEY = "responses_session"

    def __init__(self, conversation_manager: Any | None = None):
        self._conversation_manager = conversation_manager
        # In-memory store keyed by conversation_id
        self._states: dict[str, ResponsesSessionData] = {}

    # -- read -----------------------------------------------------------

    def get_state(self, conversation_id: str) -> ResponsesSessionData:
        """Return the current session state, creating a blank one if needed."""
        if conversation_id not in self._states:
            self._states[conversation_id] = ResponsesSessionData()
        return self._states[conversation_id]

    # -- write ----------------------------------------------------------

    def advance_state(
        self,
        conversation_id: str,
        response_id: str,
        message_count: int,
        fingerprints: SessionFingerprints,
    ) -> ResponsesSessionData:
        """Record a successful response and advance the chain.

        If the current fingerprints don't match the provided ones the
        chain is automatically reset first (model/tools changed).
        """
        state = self.get_state(conversation_id)

        if state.previous_response_id is not None and not self._fingerprints_match(state.fingerprints, fingerprints):
            logger.info(
                "Session chain reset for %s: fingerprint mismatch (model/tools/prompt changed)",
                conversation_id,
            )
            self.reset_state(conversation_id, reason="fingerprint_mismatch")
            state = self.get_state(conversation_id)

        state.previous_response_id = response_id
        state.synced_message_count = message_count
        state.fingerprints = fingerprints
        state.reset_reason = None
        return state

    def reset_state(
        self,
        conversation_id: str,
        reason: str = "manual",
    ) -> None:
        """Clear the session chain for *conversation_id*."""
        self._states[conversation_id] = ResponsesSessionData(reset_reason=reason)
        logger.info(
            "Session chain cleared for %s: reason=%s",
            conversation_id,
            reason,
        )

    # -- persistence helpers -------------------------------------------

    async def persist_state(self, conversation_id: str) -> None:
        """Write current state to the conversation's ``custom_data``.

        No-op if no ``conversation_manager`` is configured or if it
        doesn't support ``merge_custom_data``.
        """
        if self._conversation_manager is None:
            return

        merge_fn = getattr(self._conversation_manager, "merge_custom_data", None)
        if not callable(merge_fn):
            return

        state = self.get_state(conversation_id)
        patch = {
            self.CUSTOM_DATA_NAMESPACE: {
                self.CUSTOM_DATA_KEY: {
                    "previous_response_id": state.previous_response_id,
                    "synced_message_count": state.synced_message_count,
                    "fingerprints": {
                        "model": state.fingerprints.model,
                        "api_format": state.fingerprints.api_format,
                        "system_prompt_hash": state.fingerprints.system_prompt_hash,
                        "tools_hash": state.fingerprints.tools_hash,
                    },
                    "reset_reason": state.reset_reason,
                },
            },
        }
        try:
            await merge_fn(conversation_id, patch)
        except Exception as exc:  # noqa: BLE001 - session state persist must not break flow
            logger.warning(
                "Failed to persist session state for %s: %s",
                conversation_id,
                exc,
            )

    async def load_state(self, conversation_id: str) -> ResponsesSessionData:
        """Load state from the conversation's ``custom_data``.

        Falls back to an empty state on any error.
        """
        if self._conversation_manager is None:
            return self.get_state(conversation_id)

        get_conv = getattr(self._conversation_manager, "get_conversation", None)
        if not callable(get_conv):
            return self.get_state(conversation_id)

        try:
            conv = await get_conv(conversation_id)
            custom = getattr(conv, "metadata", None)
            if custom is None:
                return self.get_state(conversation_id)
            custom_data = getattr(custom, "custom_data", None) or {}
            llm = custom_data.get(self.CUSTOM_DATA_NAMESPACE, {})
            session = llm.get(self.CUSTOM_DATA_KEY, {})
            if not session:
                return self.get_state(conversation_id)

            fps = session.get("fingerprints", {})
            state = ResponsesSessionData(
                previous_response_id=session.get("previous_response_id"),
                synced_message_count=int(session.get("synced_message_count", 0)),
                fingerprints=SessionFingerprints(
                    model=fps.get("model", ""),
                    api_format=fps.get("api_format", ""),
                    system_prompt_hash=fps.get("system_prompt_hash", ""),
                    tools_hash=fps.get("tools_hash", ""),
                ),
                reset_reason=session.get("reset_reason"),
            )
            self._states[conversation_id] = state
            return state
        except Exception as exc:  # noqa: BLE001 - session state load must not break flow
            logger.debug("Failed to load session state for %s: %s", conversation_id, exc)
            return self.get_state(conversation_id)

    # -- helpers --------------------------------------------------------

    @staticmethod
    def _fingerprints_match(a: SessionFingerprints, b: SessionFingerprints) -> bool:
        return (
            a.model == b.model
            and a.api_format == b.api_format
            and a.system_prompt_hash == b.system_prompt_hash
            and a.tools_hash == b.tools_hash
        )


# ---------------------------------------------------------------------------
# Feature-flag helper
# ---------------------------------------------------------------------------


def should_enable_session_chain(
    entity_config: Any,
    *,
    feature_overrides: dict[str, Any] | None = None,
    config_manager: Any | None = None,
) -> bool:
    """Check whether Responses session chaining should be used."""
    flag_on = is_ai_feature_enabled(
        "AI_RESPONSES_SESSION_CHAIN_V1",
        default=False,
        config_manager=config_manager,
        overrides=feature_overrides,
    )
    if not flag_on:
        return False

    entity_on = getattr(entity_config, "enable_responses_session_chain", False)
    api_format = getattr(entity_config, "api_format", "chat_completions")
    # Session chaining only works with the Responses API
    if str(api_format).lower().replace("-", "_") != "responses":
        return False

    return bool(entity_on)
