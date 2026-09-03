"""处置建议工具执行器。"""

import asyncio
import hashlib
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
    """知识库服务：通过 HybridRetriever 实现 PostgreSQL 混合检索 (关键词 + 向量)。

    PageIndex 已弃用，HybridRetriever 提供：
    - PostgreSQL ts_vector 全文搜索
    - pgvector 向量相似度检索
    - RRF 融合算法 (k=60)
    """

    def __init__(
        self,
        base_path: str = "knowledge_base",
        db_pool: Any | None = None,
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
        self._db_pool = db_pool
        self._index: list[dict[str, Any]] = []
        self._extract_errors: list[dict[str, str]] = []
        self._doc_registry: dict[str, dict[str, Any]] = {}
        logger.info("[Advisor] SimpleKnowledgeBase initialized with HybridRetriever support")

    def index_files(self) -> int:
        """构建文件索引；仅用于文档管理（不再用于检索）。"""
        self._index = []
        self._extract_errors = []
        if not self.base_path.exists():
            logger.warning(f"知识库目录不存在：{self.base_path}")
            return 0

        file_paths: list[Path] = [
            path
            for path in self.base_path.rglob("*")
            if path.is_file() and path.suffix.lower() in self.supported_extensions
        ]
        file_paths.sort(key=lambda path: str(path))

        for file_path in file_paths:
            file_path.suffix.lower()
            path_str = str(file_path)
            content_hash = self._build_file_hash(file_path)
            entry: dict[str, Any] = {
                "path": path_str,
                "name": file_path.name,
                "category": file_path.parent.name,
                "extract_error": None,
                "content_hash": content_hash,
            }

            # Document registry kept for backward compatibility (not used by HybridRetriever)
            self._doc_registry[path_str] = {
                "content_hash": content_hash,
                "name": file_path.name,
                "category": file_path.parent.name,
            }

            self._index.append(entry)

        logger.info(f"已索引 {len(self._index)} 个知识库文件 (仅供文档管理参考)")
        return len(self._index)

    async def search(self, query: str, max_results: int = 5) -> list[dict[str, Any]]:
        """Use HybridRetriever for PostgreSQL full-text + vector search.

        Falls back to body-text keyword matching over locally readable
        files (returns snippet + provenance, never a bare filename list),
        then to filename matching as the last resort.
        """
        safe_limit = max(1, int(max_results or 5))
        normalized_query = str(query or "").strip()

        if not normalized_query:
            return []

        if not self._db_pool:
            logger.warning("[Advisor] Database connection pool not available, fallback to body-text retrieval")
            body_hits = self._fallback_body_text_results(normalized_query, safe_limit)
            if body_hits:
                return body_hits
            return self._fallback_filename_results(query=normalized_query, max_results=safe_limit)

        try:
            from src.infrastructure.ai.hybrid_retriever import HybridRetriever
        except ImportError as exc:
            logger.warning(f"[Advisor] Failed to import HybridRetriever: {exc}, fallback to body-text retrieval")
            body_hits = self._fallback_body_text_results(normalized_query, safe_limit)
            if body_hits:
                return body_hits
            return self._fallback_filename_results(query=normalized_query, max_results=safe_limit)

        retriever = HybridRetriever(db_pool=self._db_pool)

        try:
            scores = await retriever.search(
                query=normalized_query,
                top_k=safe_limit,
                min_keyword_score=0.3,
                min_vector_score=0.4,
            )

            # Convert SearchScore to dict format expected by callers
            results = [
                {
                    "name": Path(score.chunk.source_uri).name,
                    "path": score.chunk.source_uri,
                    "category": score.chunk.metadata.get("category", "unknown"),
                    "chunk_id": str(score.chunk.id),
                    "snippet": str(score.chunk.content)[:280],
                    "score": round(float(score.combined_score), 4),
                    "retrieval_mode": "hybrid_rrf",
                }
                for score in scores
            ]

            if results:
                logger.info(f"[Advisor] Retrieved {len(results)} chunks via HybridRetriever")
                return results

        except Exception as exc:  # noqa: BLE001
            logger.warning(f"[Advisor] HybridRetriever search failed: {exc}")

        # Fallback to body-text matching, then filename matching.
        body_hits = self._fallback_body_text_results(normalized_query, safe_limit)
        if body_hits:
            return body_hits
        keywords = self._extract_keywords(normalized_query)
        if not keywords:
            keywords = [normalized_query.lower()] if normalized_query else []

        return self._fallback_filename_results(keywords=keywords, max_results=safe_limit)

    def _fallback_body_text_results(self, query: str, max_results: int = 5) -> list[dict[str, Any]]:
        """Body-text keyword matching over locally readable KB files.

        Returns snippet + provenance per plan E2 ("SOP 能按正文命中",
        "返回片段和出处，不返回文件名列表"). Binary formats (pdf/docx/xlsx)
        are skipped here — they are served by the DB-backed retriever.
        """
        if not self._index:
            self.index_files()

        keywords = self._extract_keywords(query)
        if not keywords:
            keywords = [str(query).strip().lower()] if str(query).strip() else []
        if not keywords:
            return []

        text_exts = {".md", ".txt", ".csv", ".json", ".yaml", ".yml"}
        results: list[dict[str, Any]] = []

        for item in self._index:
            path = Path(item.get("path", ""))
            if path.suffix.lower() not in text_exts:
                continue
            try:
                content = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            lower = content.lower()
            if any(keyword in lower for keyword in keywords):
                snippet = self._build_snippet(content, keywords)
                results.append(
                    {
                        **item,
                        "chunk_id": None,
                        "chunk_index": None,
                        "snippet": snippet,
                        "score": 0.5,
                        "retrieval_mode": "fallback_body_text",
                    }
                )
                if len(results) >= max(1, int(max_results or 5)):
                    break

        if results:
            logger.info(f"[Advisor] Body-text fallback matched {len(results)} documents")
        return results

    def _fallback_filename_results(
        self, query: str | None = None, keywords: list[str] | None = None, max_results: int = 5
    ) -> list[dict[str, Any]]:
        """Fallback to filename matching when HybridRetriever unavailable."""
        fallback_results: list[dict[str, Any]] = []

        # Extract keywords from query if not provided
        if keywords is None:
            query_str = str(query or "").strip()
            keywords = self._extract_keywords(query_str) if query_str else []
            if not keywords and query_str:
                keywords = [query_str.lower()]

        if not keywords:
            return []

        for item in self._index:
            name_lower = str(item.get("name", "")).lower()
            if any(keyword in name_lower for keyword in keywords if keyword):
                fallback_results.append(
                    {
                        **item,
                        "chunk_id": None,
                        "chunk_index": None,
                        "snippet": f"文件名匹配：{item.get('name')}",
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
        """获取所有已索引文件列表（用于文档管理，不再用于检索）。"""
        if not self._index:
            self.index_files()
        return self._index

    def get_extract_errors(self, limit: int = 20) -> list[dict[str, str]]:
        """Return empty - HybridRetriever does not track extraction errors."""
        return []

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
