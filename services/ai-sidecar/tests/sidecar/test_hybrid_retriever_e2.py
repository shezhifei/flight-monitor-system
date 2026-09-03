"""
E2: Hybrid Knowledge Retriever Tests

Phase K4（docs/plans/2026-08-18-ai-agent-optimization.md）：
架构空测改为真实行为测试 —— 插入 chunk → 关键词能命中正文。

覆盖：
- index_chunk 写入 ai_knowledge_chunks（migration 126）的契约
- 关键词检索命中已持久化的正文
- 向量后端保持 port（默认 None 时优雅降级）
- RRF 融合算法数学正确性
- 知识分片策略（真实 ChunkSplitter）
"""

import json
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

import pytest

from src.infrastructure.ai.hybrid_retriever import (
    ChunkSplitter,
    HybridRetriever,
    KnowledgeChunk,
)

REPO_ROOT = Path(__file__).resolve().parents[4]


class FakeKnowledgeStore:
    """In-memory stand-in for the ai_knowledge_chunks table (migration 126).

    execute() 模拟 INSERT ... ON CONFLICT upsert；fetch() 按空格分词做朴素
    关键词匹配（模拟 websearch_to_tsquery + ts_rank），让「插入 chunk →
    关键词命中正文」可以在无真实数据库时闭环验证。
    """

    def __init__(self) -> None:
        self.rows: dict[UUID, dict[str, Any]] = {}
        self.executed: list[tuple[str, tuple[Any, ...]]] = []

    async def execute(self, sql: str, *args: Any) -> str:
        self.executed.append((sql, args))
        chunk_id, content, metadata, source_uri, version, created_at, updated_at, embedding = args
        self.rows[chunk_id] = {
            "id": chunk_id,
            "content": content,
            "metadata": json.loads(metadata) if isinstance(metadata, str) else metadata,
            "source_uri": source_uri,
            "version": version,
            "created_at": created_at,
            "updated_at": updated_at,
            "embedding": json.loads(embedding) if isinstance(embedding, str) else embedding,
        }
        return "INSERT 0 1"

    async def fetch(self, sql: str, *args: Any) -> list[dict[str, Any]]:
        query = str(args[0])
        min_score = float(args[-1])
        tokens = [t for t in query.lower().split() if t]
        hits: list[dict[str, Any]] = []
        for row in self.rows.values():
            content_lower = row["content"].lower()
            matched = sum(1 for t in tokens if t in content_lower)
            if not tokens or matched == 0:
                continue
            score = matched / len(tokens)  # naive ts_rank stand-in
            if score >= min_score:
                hits.append({**row, "score": score})
        hits.sort(key=lambda r: r["score"], reverse=True)
        return hits


def make_chunk(content: str, source_uri: str = "kb://sop/delay.md") -> KnowledgeChunk:
    return KnowledgeChunk(id=uuid4(), content=content, source_uri=source_uri)


class TestKeywordRoundTrip:
    """验收核心：插入 chunk → 关键词能命中正文。"""

    @pytest.mark.asyncio
    async def test_index_then_keyword_search_hits_content(self):
        store = FakeKnowledgeStore()
        retriever = HybridRetriever(db_pool=store, redis_client=None)

        chunk = make_chunk("航班延误处置 SOP：延误超过两小时需通知机组与地服")
        await retriever.index_chunk(chunk)

        results = await retriever._search_by_keywords("延误 处置", min_score=0.1)

        assert len(results) == 1
        found, score = results[0]
        assert found.id == chunk.id
        assert "延误" in found.content
        assert score > 0

    @pytest.mark.asyncio
    async def test_keyword_search_misses_unrelated_content(self):
        store = FakeKnowledgeStore()
        retriever = HybridRetriever(db_pool=store, redis_client=None)

        await retriever.index_chunk(make_chunk("机位分配规则：优先近机位"))

        results = await retriever._search_by_keywords("延误 处置", min_score=0.1)
        assert results == []

    @pytest.mark.asyncio
    async def test_index_chunk_upsert_contract(self):
        store = FakeKnowledgeStore()
        retriever = HybridRetriever(db_pool=store, redis_client=None)

        chunk = make_chunk("除冰流程正文", source_uri="kb://sop/deice.md")
        chunk.metadata = {"heading": "## 除冰"}
        await retriever.index_chunk(chunk)

        assert len(store.executed) == 1
        sql, args = store.executed[0]
        assert "INSERT INTO ai_knowledge_chunks" in sql
        assert "ON CONFLICT (id) DO UPDATE" in sql
        assert args[0] == chunk.id
        assert args[1] == "除冰流程正文"
        assert json.loads(args[2]) == {"heading": "## 除冰"}
        assert args[3] == "kb://sop/deice.md"
        # 向量后端保持 port：默认不写 embedding
        assert args[7] is None

        # 重复写入同 id 走更新路径，表内仍只有一行
        chunk.content = "除冰流程正文（修订）"
        await retriever.index_chunk(chunk)
        assert len(store.rows) == 1
        assert store.rows[chunk.id]["content"] == "除冰流程正文（修订）"

    @pytest.mark.asyncio
    async def test_index_chunk_without_pool_degrades_gracefully(self):
        retriever = HybridRetriever(db_pool=None, redis_client=None)
        await retriever.index_chunk(make_chunk("无库也不抛异常"))  # 不抛即通过


class TestVectorBackendPort:
    """向量后端保持 port：默认无 embedding model 时优雅降级。"""

    @pytest.mark.asyncio
    async def test_vector_search_returns_empty_without_embedding_model(self):
        store = FakeKnowledgeStore()
        retriever = HybridRetriever(db_pool=store, redis_client=None)
        await retriever.index_chunk(make_chunk("任意正文"))

        results = await retriever._search_by_vector_similarity("任意", min_score=0.4)
        assert results == []

    @pytest.mark.asyncio
    async def test_keyword_search_without_pool_returns_empty(self):
        retriever = HybridRetriever(db_pool=None, redis_client=None)
        results = await retriever._search_by_keywords("延误", min_score=0.1)
        assert results == []


class TestRrfFusion:
    """Reciprocal Rank Fusion (RRF) 算法正确实现。"""

    def test_rrf_ranking_formula(self):
        # Verify RRF ranking formula: score = sum(1 / (k + rank)); k=60 is standard.
        k = 60

        # Same number of ranked lists for both docs; Doc 1 ranks higher.
        doc_1_keywords = [1, 5]
        doc_1_vectors = [3]

        doc_2_keywords = [4, 9]
        doc_2_vectors = [2]

        rrf_doc_1 = sum(1 / (k + r) for r in doc_1_keywords) + sum(1 / (k + r) for r in doc_1_vectors)
        rrf_doc_2 = sum(1 / (k + r) for r in doc_2_keywords) + sum(1 / (k + r) for r in doc_2_vectors)

        assert rrf_doc_1 > rrf_doc_2, "RRF should correctly rank Doc 1 higher than Doc 2"

    def test_apply_rrf_merges_dual_engine_results(self):
        retriever = HybridRetriever(db_pool=None, redis_client=None)
        shared = make_chunk("两路都命中的正文")
        keyword_only = make_chunk("仅关键词命中")
        vector_only = make_chunk("仅向量命中")

        scores = retriever._apply_rrf(
            [(shared, 0.9), (keyword_only, 0.5)],
            [(shared, 0.8), (vector_only, 0.6)],
        )

        by_id = {s.chunk.id: s for s in scores}
        # 双路命中的 chunk 拿到两项 RRF 分数，必然排第一
        assert by_id[shared.id].combined_score > by_id[keyword_only.id].combined_score
        assert by_id[shared.id].combined_score > by_id[vector_only.id].combined_score
        assert len(scores) == 3


class TestChunkingStrategies:
    """知识分片策略测试（真实 ChunkSplitter 实现）。"""

    def test_markdown_header_splitter(self):
        markdown_doc = """# Introduction
## Background
Some background text here.

## Methodology
More detailed methodology content.

### Experimental Design
Specific experimental details.

## Results
Key findings and data analysis.
"""
        chunks = ChunkSplitter.split_markdown(markdown_doc)

        assert len(chunks) >= 3, "Should create at least 3 chunks from headers"
        assert all(c.metadata["heading"].startswith("#") for c in chunks)
        assert all(c.content for c in chunks), "每个分片都应有正文"
        assert any("Methodology" in c.metadata["heading"] for c in chunks)

    def test_row_based_excel_splitting(self):
        excel_rows = [
            {"flight_number": "CA1234", "status": "delayed"},
            {"flight_number": "MU5678", "status": "on_time"},
            {"flight_number": "CZ3022", "status": "cancelled"},
        ]

        chunks = ChunkSplitter.split_excel_rows(excel_rows)

        assert len(chunks) == 3
        assert [c.metadata["row_index"] for c in chunks] == [0, 1, 2]
        assert "CA1234" in chunks[0].content

    def test_pdf_page_chunks_with_overlap(self):
        pdf_pages = ["Page 1 content\nPage 1 last line", "Page 2 content", "Page 3 content"]

        chunks = ChunkSplitter.split_pdf_pages(pdf_pages, overlap_size=1)

        assert len(chunks) == 3
        # 第 2 页的分片应携带第 1 页最后一行作为上下文重叠
        assert "Page 1 last line" in chunks[1].content
        assert chunks[2].metadata["page_number"] == 3

    def test_semantic_chunking(self):
        text = (
            "The Boeing 737 MAX experienced a series of critical safety issues. "
            "This led to global grounding orders by aviation authorities. "
            "Recovery took approximately two years worldwide.\n\n"
            "Airbus A320neo maintains strong market position. "
            "Fuel efficiency improvements range from 15-20%. "
            "Customer orders exceeded 5000 aircraft globally."
        )

        chunks = ChunkSplitter.split_semantically(text, max_chunk_size=150)

        assert len(chunks) == 2, "Should split into Boeing section and Airbus section"
        assert "Boeing" in chunks[0].content, "First chunk should contain Boeing topic"
        assert "Airbus" in chunks[1].content, "Second chunk should contain Airbus topic"


class TestKnowledgePersistence:
    """知识持久化层测试（对照 migration 126 真实 DDL）。"""

    def test_migration_126_defines_expected_schema(self):
        ddl = (REPO_ROOT / "migrations" / "126_ai_knowledge_chunks.sql").read_text(encoding="utf-8")

        assert "CREATE TABLE IF NOT EXISTS ai_knowledge_chunks" in ddl
        for column in ("id", "content", "metadata", "source_uri", "version", "created_at", "updated_at", "embedding"):
            assert column in ddl, f"migration 126 should define column '{column}'"
        # 全文检索索引与 K4 关键词路径配套
        assert "to_tsvector('simple', content)" in ddl
        # migration 120 之后禁止新增外键
        assert "REFERENCES" not in ddl
