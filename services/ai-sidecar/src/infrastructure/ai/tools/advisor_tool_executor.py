"""处置建议工具执行器。"""

import asyncio
import hashlib
import os
import re
from pathlib import Path
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.logging.core import get_logger

from .advisor_tools import AdvisorToolName
from .base import BaseToolExecutor, ToolCategory

logger = get_logger(__name__)

try:
    from pageindex import PageIndexAPIError, PageIndexClient
except Exception as exc:  # pragma: no cover - optional dependency  # noqa: BLE001 - optional dependency import guard
    logger.warning("pageindex import unavailable: %s", exc)
    PageIndexClient = None  # type: ignore[assignment]  # optional dep stub — guarded by import guard above

    class PageIndexAPIError(RuntimeError):  # type: ignore[no-redef]  # intentional fallback type
        """Fallback error type when pageindex package is unavailable."""


ADVISOR_PROMPT_TEMPLATE = """你是一名机场运行管理专家。请根据以下事件描述和参考资料，提供专业的处置建议。

# 事件描述
{incident_description}

# 紧急程度
{urgency}

# 航班上下文
{flight_context}

# 参考资料
{reference_docs}

# 输出要求
1. 给出3-5条具体可操作的处置建议，按优先级排序
2. 如适用，引用相关规范文件或案例
3. 提示需要通知的相关部门或人员
4. 给出预估处置时间

请使用简洁的中文输出。"""

FEW_SHOT_CASES = """
案例1: 旅客拒绝登机
- 处置措施: 1)安抚旅客情绪 2)了解具体原因 3)如涉及行李,协调卸载 4)通知值班经理 5)填写不正常事件记录
- 预估时间: 15-30分钟

案例2: 航班大面积延误
- 处置措施: 1)启动延误处置预案 2)协调候机区服务保障 3)每30分钟更新航班动态 4)安排餐食饮水 5)对于超4小时延误启动住宿安排
- 预估时间: 视延误时长而定

案例3: 机械故障需换机
- 处置措施: 1)通知机务确认故障 2)申请备份飞机 3)协调旅客行李转运 4)更新航班动态 5)通知联检单位
- 预估时间: 2-4小时
"""


class SimpleKnowledgeBase:
    """知识库服务：移除本地 RAG，仅通过 PageIndex 检索，失败时回退文件名匹配。"""

    def __init__(
        self,
        base_path: str = "knowledge_base",
        db_pool: Any | None = None,
        pageindex_api_key: str | None = None,
        pageindex_client: Any | None = None,
    ):
        self.base_path = Path(base_path)
        self.supported_extensions = {
            ".pdf",
            ".docx",
            ".xlsx",
            ".md",
            ".txt",
            ".csv",
            ".json",
            ".yaml",
            ".yml",
        }
        self.pageindex_extensions = {".pdf"}
        self._index: list[dict[str, Any]] = []
        self._extract_errors: list[dict[str, str]] = []
        self._db_pool = db_pool  # backward compatibility only
        self._pageindex_api_key = str(pageindex_api_key or os.getenv("PAGEINDEX_API_KEY", "")).strip()
        self._pageindex_client = pageindex_client
        self._doc_registry: dict[str, dict[str, Any]] = {}
        self._poll_interval_seconds = 0.8
        self._poll_max_attempts = 8

    def index_files(self) -> int:
        """构建文件索引；不再做本地抽取和分块。"""
        self._index = []
        self._extract_errors = []
        if not self.base_path.exists():
            logger.warning(f"知识库目录不存在: {self.base_path}")
            return 0

        file_paths: list[Path] = [
            path
            for path in self.base_path.rglob("*")
            if path.is_file() and path.suffix.lower() in self.supported_extensions
        ]
        file_paths.sort(key=lambda path: str(path))

        for file_path in file_paths:
            ext = file_path.suffix.lower()
            path_str = str(file_path)
            content_hash = self._build_file_hash(file_path)
            entry: dict[str, Any] = {
                "path": path_str,
                "name": file_path.name,
                "category": file_path.parent.name,
                "extract_error": None,
                "content_hash": content_hash,
            }

            if ext not in self.pageindex_extensions:
                message = f"unsupported_by_pageindex:{ext}"
                entry["extract_error"] = message
                self._upsert_extract_error(path_str, message)
            else:
                registry_entry = self._doc_registry.get(path_str) or {}
                if registry_entry.get("content_hash") != content_hash:
                    self._doc_registry[path_str] = {
                        "doc_id": None,
                        "content_hash": content_hash,
                        "name": file_path.name,
                        "category": file_path.parent.name,
                    }

            self._index.append(entry)

        logger.info(
            f"已索引 {len(self._index)} 个知识库文件, "
            f"pageindex_candidates={len([i for i in self._index if not i.get('extract_error')])}, "
            f"extract_errors={len(self._extract_errors)}"
        )
        return len(self._index)

    async def search(self, query: str, max_results: int = 5) -> list[dict[str, Any]]:
        """通过 PageIndex 执行检索，失败时回退文件名匹配。"""
        if not self._index:
            self.index_files()

        safe_limit = max(1, int(max_results or 5))
        normalized_query = str(query or "").strip()
        keywords = self._extract_keywords(normalized_query)
        if not keywords and normalized_query:
            keywords = [normalized_query.lower()]
        if not normalized_query:
            return []

        pageindex_docs = [item for item in self._index if not item.get("extract_error")]
        if not pageindex_docs:
            return self._fallback_filename_results(keywords=keywords, max_results=safe_limit)

        client = self._get_pageindex_client()
        if client is None:
            return self._fallback_filename_results(keywords=keywords, max_results=safe_limit)

        await self._ensure_pageindex_documents(client=client, docs=pageindex_docs)
        retrieval_hits = await self._search_via_pageindex(
            client=client,
            docs=pageindex_docs,
            query=normalized_query,
            max_results=safe_limit,
            keywords=keywords,
        )
        if retrieval_hits:
            return retrieval_hits[:safe_limit]

        return self._fallback_filename_results(keywords=keywords, max_results=safe_limit)

    def _get_pageindex_client(self) -> Any | None:
        if self._pageindex_client is not None:
            return self._pageindex_client
        if not self._pageindex_api_key:
            return None
        if PageIndexClient is None:
            logger.warning("pageindex package unavailable, fallback to filename retrieval")
            return None
        try:
            self._pageindex_client = PageIndexClient(api_key=self._pageindex_api_key)
        except Exception as exc:  # noqa: BLE001 - third-party PageIndex client init may raise arbitrary errors
            logger.warning(f"初始化 PageIndex 客户端失败: {exc}")
            return None
        return self._pageindex_client

    async def _ensure_pageindex_documents(self, client: Any, docs: list[dict[str, Any]]) -> None:
        for doc in docs:
            path = str(doc.get("path") or "").strip()
            if not path:
                continue
            content_hash = str(doc.get("content_hash") or "")
            registry_entry = self._doc_registry.setdefault(
                path,
                {
                    "doc_id": None,
                    "content_hash": content_hash,
                    "name": doc.get("name"),
                    "category": doc.get("category"),
                },
            )
            if registry_entry.get("doc_id") and registry_entry.get("content_hash") == content_hash:
                continue

            try:
                resp = await self._run_in_thread(client.submit_document, path)
                doc_id = self._extract_doc_id(resp)
                if not doc_id:
                    raise RuntimeError("doc_id missing in PageIndex submit response")
                registry_entry["doc_id"] = doc_id
                registry_entry["content_hash"] = content_hash
            except (PageIndexAPIError, Exception) as exc:  # noqa: BLE001 - third-party PageIndex client call may raise arbitrary errors
                message = f"pageindex_submit_failed:{exc}"
                doc["extract_error"] = message
                self._upsert_extract_error(path, message)
                logger.warning(f"PageIndex 文档提交失败 path={path}: {exc}")

    async def _search_via_pageindex(
        self,
        *,
        client: Any,
        docs: list[dict[str, Any]],
        query: str,
        max_results: int,
        keywords: list[str],
    ) -> list[dict[str, Any]]:
        results: list[dict[str, Any]] = []
        for doc in docs:
            path = str(doc.get("path") or "").strip()
            if not path:
                continue
            doc_registry = self._doc_registry.get(path) or {}
            doc_id = str(doc_registry.get("doc_id") or "").strip()
            if not doc_id:
                continue

            try:
                if not await self._run_in_thread(client.is_retrieval_ready, doc_id):
                    continue

                query_resp = await self._run_in_thread(client.submit_query, doc_id, query, False)
                retrieval_id = self._extract_retrieval_id(query_resp)
                if not retrieval_id:
                    logger.warning(f"PageIndex 查询返回缺少 retrieval_id, path={path}")
                    continue

                payload = await self._poll_retrieval_result(client, retrieval_id)
                hits = self._normalize_retrieval_hits(
                    payload=payload,
                    doc=doc,
                    retrieval_id=retrieval_id,
                    keywords=keywords,
                )
                results.extend(hits)
            except (PageIndexAPIError, Exception) as exc:  # noqa: BLE001 - third-party PageIndex client call may raise arbitrary errors
                logger.warning(f"PageIndex 查询失败 doc_id={doc_id}: {exc}")

        results.sort(key=lambda item: float(item.get("score") or 0), reverse=True)
        return results[: max(1, int(max_results or 5))]

    async def _poll_retrieval_result(self, client: Any, retrieval_id: str) -> dict[str, Any]:
        last_payload: dict[str, Any] = {}
        for attempt in range(self._poll_max_attempts):
            payload = await self._run_in_thread(client.get_retrieval, retrieval_id)
            if isinstance(payload, dict):
                last_payload = payload
                if self._iter_retrieval_candidates(payload):
                    return payload
                status = str(payload.get("status") or payload.get("state") or "").lower().strip()
                if status in {"failed", "error", "cancelled", "timeout"}:
                    raise RuntimeError(f"retrieval status={status}")
            if attempt < self._poll_max_attempts - 1:
                await asyncio.sleep(self._poll_interval_seconds)
        return last_payload

    def _normalize_retrieval_hits(
        self,
        *,
        payload: dict[str, Any],
        doc: dict[str, Any],
        retrieval_id: str,
        keywords: list[str],
    ) -> list[dict[str, Any]]:
        hits: list[dict[str, Any]] = []
        candidates = self._iter_retrieval_candidates(payload)
        for idx, raw_item in enumerate(candidates):
            item = raw_item if isinstance(raw_item, dict) else {"text": str(raw_item)}
            snippet = str(
                item.get("snippet") or item.get("text") or item.get("content") or item.get("chunk_text") or ""
            )
            if not snippet:
                snippet = self._build_snippet(str(payload), keywords)
            score = float(item.get("score") or item.get("similarity") or item.get("relevance") or 0.1)
            chunk_id = item.get("chunk_id") or item.get("id") or f"{retrieval_id}#{idx}"
            chunk_index = item.get("chunk_index")
            if chunk_index is None:
                chunk_index = item.get("index", idx)

            hits.append(
                {
                    "name": item.get("name") or doc.get("name"),
                    "path": item.get("path") or doc.get("path"),
                    "category": item.get("category") or doc.get("category"),
                    "chunk_id": chunk_id,
                    "chunk_index": chunk_index,
                    "snippet": str(snippet)[:280],
                    "score": round(score, 4),
                    "retrieval_mode": item.get("retrieval_mode") or "pageindex_retrieval",
                }
            )
        return hits

    @staticmethod
    def _iter_retrieval_candidates(payload: dict[str, Any]) -> list[Any]:
        if not isinstance(payload, dict):
            return []

        direct_candidates = payload.get("results") or payload.get("result") or payload.get("chunks")
        if isinstance(direct_candidates, list):
            return direct_candidates

        data = payload.get("data")
        if isinstance(data, list):
            return data
        if isinstance(data, dict):
            for key in ("results", "chunks", "items", "matches", "nodes"):
                values = data.get(key)
                if isinstance(values, list):
                    return values
        return []

    @staticmethod
    def _extract_doc_id(resp: Any) -> str | None:
        if not isinstance(resp, dict):
            return None
        return str(resp.get("doc_id") or resp.get("document_id") or resp.get("id") or "").strip() or None

    @staticmethod
    def _extract_retrieval_id(resp: Any) -> str | None:
        if not isinstance(resp, dict):
            return None
        return str(resp.get("retrieval_id") or resp.get("id") or "").strip() or None

    def _fallback_filename_results(self, keywords: list[str], max_results: int) -> list[dict[str, Any]]:
        fallback_results: list[dict[str, Any]] = []
        for item in self._index:
            name_lower = str(item.get("name", "")).lower()
            if any(keyword in name_lower for keyword in keywords if keyword):
                fallback_results.append(
                    {
                        **item,
                        "chunk_id": None,
                        "chunk_index": None,
                        "snippet": f"文件名匹配: {item.get('name')}",
                        "score": 0.01,
                        "retrieval_mode": "fallback_filename",
                    }
                )
                if len(fallback_results) >= max(1, int(max_results or 5)):
                    break
        return fallback_results

    def _upsert_extract_error(self, path: str, error: str) -> None:
        for item in self._extract_errors:
            if item.get("path") == path:
                item["error"] = error
                return
        self._extract_errors.append({"path": path, "error": error})

    @staticmethod
    def _build_file_hash(file_path: Path) -> str:
        digest = hashlib.sha256()
        with file_path.open("rb") as fp:
            while True:
                chunk = fp.read(8192)
                if not chunk:
                    break
                digest.update(chunk)
        return digest.hexdigest()

    @staticmethod
    async def _run_in_thread(func: Any, *args: Any, **kwargs: Any) -> Any:
        return await asyncio.to_thread(func, *args, **kwargs)

    def get_file_list(self) -> list[dict[str, Any]]:
        """获取所有已索引文件列表。"""
        if not self._index:
            self.index_files()
        return self._index

    def get_extract_errors(self, limit: int = 20) -> list[dict[str, str]]:
        """返回文档抽取失败列表，用于前端提示部分文档不可读。"""
        safe_limit = max(0, int(limit or 0))
        if safe_limit == 0:
            return []
        return self._extract_errors[:safe_limit]

    @staticmethod
    def _extract_keywords(query: str) -> list[str]:
        text = str(query or "").strip().lower()
        if not text:
            return []
        return [token for token in re.split(r"\s+", text) if token]

    @staticmethod
    def _build_snippet(content: str, keywords: list[str], width: int = 220) -> str:
        normalized = str(content or "")
        if not normalized:
            return ""
        lower = normalized.lower()
        anchor = 0
        for keyword in keywords:
            if not keyword:
                continue
            pos = lower.find(keyword)
            if pos >= 0:
                anchor = pos
                break
        start = max(0, anchor - width // 3)
        end = min(len(normalized), start + width)
        snippet = normalized[start:end].replace("\n", " ").strip()
        return snippet


class AdvisorToolExecutor(BaseToolExecutor):
    """处置建议工具执行器"""

    def __init__(
        self,
        knowledge_base: SimpleKnowledgeBase | None = None,
        flight_service=None,
        ai_entity=None,
        default_user: str = "AdvisorAgent",
    ):
        super().__init__(default_user)
        self._knowledge_base = knowledge_base or SimpleKnowledgeBase()
        self._flight_service = flight_service
        self._ai_entity = ai_entity

    def get_category(self) -> ToolCategory:
        return ToolCategory.ADVISOR

    def _register_handlers(self) -> None:
        self._handlers = {AdvisorToolName.GET_HANDLING_RECOMMENDATION.value: self._handle_get_recommendation}

    def set_services(self, knowledge_base=None, flight_service=None, ai_entity=None):
        if knowledge_base:
            self._knowledge_base = knowledge_base
        if flight_service:
            self._flight_service = flight_service
        if ai_entity:
            self._ai_entity = ai_entity

    async def _handle_get_recommendation(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理获取处置建议请求。"""
        incident_description = self._require_arg(args, "incident_description")

        flight_id = args.get("flight_id")
        urgency = args.get("urgency", "中")

        flight_context = "无关联航班"
        if flight_id and self._flight_service:
            flight = await self._safe_call(
                lambda: self._flight_service.get_flight(flight_id),
                "获取航班信息失败",
            )
            if flight:
                flight_obj = self._unwrap(flight, "get_flight")
                flight_context = f"""
- 航班号: {self._extract_value(getattr(flight_obj, "flight_number", None))}
- 状态: {self._extract_value(getattr(flight_obj, "status", None))}
- 机位: {self._extract_value(getattr(flight_obj, "stand", None))}
- 登机口: {self._extract_value(getattr(flight_obj, "gate", None))}
- 计划起飞: {self._to_iso(getattr(flight_obj, "scheduled_departure", None))}"""

        reference_docs = "（知识库中暂无匹配文档，使用内置案例）\n\n" + FEW_SHOT_CASES
        docs: list[dict[str, str]] = []
        if self._knowledge_base:
            docs = await self._safe_call(
                lambda: self._knowledge_base.search(incident_description),
                "知识库搜索失败",
                default=[],
            )
            if docs:
                reference_docs = "相关文档:\n" + "\n".join(
                    [
                        (
                            f"- [{d.get('name')}]"
                            f" (分类: {d.get('category')}, "
                            f"chunk_id={d.get('chunk_id')}, "
                            f"score={d.get('score')}, "
                            f"mode={d.get('retrieval_mode')})\n"
                            f"  片段: {str(d.get('snippet') or '')[:180]}"
                        )
                        for d in docs
                    ]
                )

        prompt = ADVISOR_PROMPT_TEMPLATE.format(
            incident_description=incident_description,
            urgency=urgency,
            flight_context=flight_context,
            reference_docs=reference_docs,
        )

        def _recommendation_fallback(exc: Exception | None) -> str:
            if exc:
                return f"生成失败: {exc!s}\n\n请参考内置案例:\n{FEW_SHOT_CASES}"
            return f"（AI未配置）\n\n请参考内置案例:\n{FEW_SHOT_CASES}"

        recommendations = await self._run_ai_task(
            prompt=prompt,
            ai_entity=self._ai_entity,
            error_message="AI生成建议失败",
            fallback_builder=_recommendation_fallback,
        )

        return self._success_response(
            incident_description=incident_description,
            urgency=urgency,
            recommendations=recommendations,
            extract_errors=self._knowledge_base.get_extract_errors(limit=10) if self._knowledge_base else [],
            sources=[
                {
                    "name": d.get("name"),
                    "path": d.get("path"),
                    "chunk_id": d.get("chunk_id"),
                    "score": d.get("score"),
                    "snippet": d.get("snippet"),
                    "retrieval_mode": d.get("retrieval_mode"),
                }
                for d in docs
            ],
            generated_at=utc_now().isoformat(),
        )


__all__ = ["AdvisorToolExecutor", "SimpleKnowledgeBase"]
