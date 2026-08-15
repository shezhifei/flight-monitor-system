"""人工反馈队列 — 基于项目现有 mq-gateway(RocketMQ)HTTP API。

复用 services/mq-gateway 暴露的三个端点:
    POST /messages/publish  — 发布 AI 作答任务到人工队列
    POST /messages/receive  — 消费组批量拉取待回答任务
    POST /messages/ack      — 确认处理完成

网关配置与环境变量(与 ai-sidecar 现有惯例一致):
    AI_MQ_GATEWAY_URL / MQ_GATEWAY_URL — 网关 base URL
    AI_MQ_GATEWAY_API_KEY              — 可选,生产环境必填
      (写入端点鉴权:Authorization: Bearer <key> 或 x-mq-gateway-token)

消息设计:
    topic   = "shadow.human.queue"
    tag     = "shadow.task"
    key     = session_id(RocketMQ 按 key 有序,同会话任务串行)
    body    = ShadowTask 的 JSON 序列化

AI 侧运行暂存:沿用 Redis(shadow:ai:{session_id},TTL 7 天),
仅作为 reconcile 时的对账快照,不承担队列语义。
"""

from __future__ import annotations

import json
import os
import uuid
from dataclasses import asdict, dataclass
from typing import Any

import httpx

TOPIC = "shadow.human.queue"
TAG = "shadow.task"
CONSUMER_GROUP = "shadow_workers"
AI_RUN_TTL_SECONDS = 7 * 24 * 3600


class MqGatewayError(RuntimeError):
    """mq-gateway 调用失败(网关拒绝或传输错误)。"""


@dataclass(frozen=True)
class ShadowTask:
    """一条待人工回答的 Shadow 任务。"""

    session_id: str
    question_text: str
    ai_answer: str
    ai_confidence: float | None
    created_at: str

    def to_body(self) -> dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_body(cls, body: dict[str, Any]) -> ShadowTask:
        return cls(
            session_id=str(body["session_id"]),
            question_text=str(body["question_text"]),
            ai_answer=str(body.get("ai_answer", "")),
            ai_confidence=body.get("ai_confidence"),
            created_at=str(body.get("created_at", "")),
        )


class HumanFeedbackQueue:
    """通过 mq-gateway(RocketMQ)承载的人工反馈队列。

    网关负责实际的 RocketMQ 收发与重试;本类只承担:
    构造请求体、鉴权头、结果解析,以及在传输错误上的简单重试。
    """

    def __init__(
        self,
        gateway_base_url: str | None = None,
        *,
        api_key: str | None = None,
        pool: Any = None,
        redis: Any = None,
        timeout: float = 5.0,
        max_retries: int = 3,
        backoff_seconds: float = 0.25,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        self._base_url = (gateway_base_url or _resolve_gateway_url() or "").rstrip("/")
        if not self._base_url:
            raise MqGatewayError(
                "mq-gateway URL 未配置:请设置 AI_MQ_GATEWAY_URL 或 MQ_GATEWAY_URL"
            )
        self._api_key = api_key or _resolve_gateway_api_key()
        self._pool = pool
        self._redis = redis
        self._owns_client = client is None
        self._client = client or httpx.AsyncClient(timeout=timeout)
        self._max_retries = max_retries
        self._backoff = backoff_seconds

    # ------------------------------------------------------------------
    # 队列操作(publish / receive / ack)
    # ------------------------------------------------------------------
    async def publish_task(self, task: ShadowTask) -> str:
        """发布一条任务,返回网关分配的 message_id。"""
        wire = {
            "topic": TOPIC,
            "tag": TAG,
            "key": task.session_id,
            "body": task.to_body(),
            "properties": {"source": "shadow_interceptor"},
        }
        payload = await self._post_json("/messages/publish", wire)
        message_id = payload.get("message_id")
        if not message_id:
            raise MqGatewayError(f"SHADOW_MQ_INVALID_RESPONSE: publish 响应缺少 message_id: {payload}")
        return str(message_id)

    async def read_pending(
        self, *, batch_size: int = 10, wait_ms: int = 200
    ) -> list[tuple[ShadowTask, str]]:
        """拉取一批待处理任务,返回 (task, receipt_handle) 列表。"""
        wire = {
            "topic": TOPIC,
            "consumer_group": CONSUMER_GROUP,
            "filter_tag": TAG,
            "batch_size": batch_size,
            "wait_ms": wait_ms,
        }
        payload = await self._post_json("/messages/receive", wire)
        results: list[tuple[ShadowTask, str]] = []
        for msg in payload.get("messages", []):
            try:
                task = ShadowTask.from_body(msg["body"])
            except (KeyError, TypeError, ValueError) as exc:
                raise MqGatewayError(
                    f"SHADOW_MQ_INVALID_BODY: 任务消息体无法解析: {exc}"
                ) from exc
            results.append((task, str(msg["receipt_handle"])))
        return results

    async def ack(self, receipt_handle: str) -> None:
        """确认一条消息处理完成。"""
        await self._post_json("/messages/ack", {"receipt_handle": receipt_handle})

    # ------------------------------------------------------------------
    # 数据库持久化(反馈 / 差异)
    # ------------------------------------------------------------------
    async def record_feedback(
        self,
        *,
        session_id: str,
        operator_id: int,
        human_answer: str,
        confidence_self: int = 3,
        notes: str | None = None,
    ) -> int:
        """写入 operator_feedback,返回新行 id。"""
        if self._pool is None:
            raise RuntimeError("record_feedback 需要 Postgres 连接池(asyncpg pool)")
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                """
                INSERT INTO operator_feedback
                    (session_id, operator_id, human_answer, confidence_self, notes)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                """,
                uuid.UUID(session_id),
                operator_id,
                human_answer,
                confidence_self,
                notes,
            )
        return int(row["id"])

    async def insert_discrepancy(
        self,
        *,
        session_id: str,
        operator_id: int,
        discrepancy_type: str,
        severity: str,
        question_text: str,
        ai_answer: str,
        human_answer: str,
        details: str | None = None,
        ai_confidence: float | None = None,
    ) -> int | None:
        """写入 shadow_mode_discrepancies,返回新行 id(无差异时返回 None)。"""
        if self._pool is None:
            raise RuntimeError("insert_discrepancy 需要 Postgres 连接池(asyncpg pool)")
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                """
                INSERT INTO shadow_mode_discrepancies
                    (session_id, operator_id, discrepancy_type, severity,
                     question_text, ai_answer, human_answer, details, ai_confidence)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING id
                """,
                uuid.UUID(session_id),
                operator_id,
                discrepancy_type,
                severity,
                question_text,
                ai_answer,
                human_answer,
                details,
                ai_confidence,
            )
        return int(row["id"])

    # ------------------------------------------------------------------
    # AI 侧运行暂存(Redis 对账快照)
    # ------------------------------------------------------------------
    async def save_ai_run(self, session_id: str, payload: dict[str, Any]) -> None:
        """暂存 AI 运行快照(reconcile 对账用),TTL 7 天。"""
        if self._redis is None:
            raise RuntimeError("save_ai_run 需要 Redis 客户端(redis.asyncio)")
        await self._redis.set(
            f"shadow:ai:{session_id}", json.dumps(payload, ensure_ascii=False),
            ex=AI_RUN_TTL_SECONDS,
        )

    async def load_ai_run(self, session_id: str) -> dict[str, Any] | None:
        raw = await self._redis.get(f"shadow:ai:{session_id}") if self._redis else None
        if raw is None:
            return None
        if isinstance(raw, bytes):
            raw = raw.decode("utf-8")
        return json.loads(raw)

    # ------------------------------------------------------------------
    # 内部:HTTP 调用与重试
    # ------------------------------------------------------------------
    async def _post_json(self, path: str, wire: dict[str, Any]) -> dict[str, Any]:
        url = f"{self._base_url}{path}"
        headers = {"Content-Type": "application/json"}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"
            headers["x-mq-gateway-token"] = self._api_key
        last_exc: Exception | None = None
        attempts = self._max_retries + 1
        import asyncio

        for attempt in range(1, attempts + 1):
            try:
                response = await self._client.post(url, json=wire, headers=headers)
                response.raise_for_status()
                if response.status_code == 204 or not response.content:
                    return {}
                return response.json()
            except httpx.HTTPStatusError as exc:
                status = exc.response.status_code
                if 500 <= status < 600 and attempt < attempts:
                    last_exc = exc
                    await asyncio.sleep(self._backoff * (2 ** (attempt - 1)))
                    continue
                raise MqGatewayError(
                    f"SHADOW_MQ_HTTP_{status}: 网关拒绝请求: {exc.response.text}"
                ) from exc
            except httpx.HTTPError as exc:
                if attempt < attempts:
                    last_exc = exc
                    await asyncio.sleep(self._backoff * (2 ** (attempt - 1)))
                    continue
                raise MqGatewayError(
                    f"SHADOW_MQ_TRANSPORT: {attempts} 次尝试后仍失败: {exc}"
                ) from exc
        raise MqGatewayError(f"SHADOW_MQ_TRANSPORT: 重试耗尽: {last_exc}") from last_exc

    async def aclose(self) -> None:
        if self._owns_client:
            await self._client.aclose()

    async def __aenter__(self) -> HumanFeedbackQueue:
        return self

    async def __aexit__(self, *exc: Any) -> None:
        await self.aclose()


def _resolve_gateway_url() -> str | None:
    for key in ("AI_MQ_GATEWAY_URL", "MQ_GATEWAY_URL", "AI_MQ_GATEWAY_BASE_URL"):
        value = os.environ.get(key, "").strip()
        if value:
            return value
    return None


def _resolve_gateway_api_key() -> str | None:
    value = os.environ.get("AI_MQ_GATEWAY_API_KEY", "").strip()
    return value or None