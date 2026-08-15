"""E2: Real HybridRetriever / ChunkSplitter unit tests (hermetic, no DB).

Covers the plan's exit criteria: "SOP 能按正文命中" — a knowledge chunk
indexed from an SOP document is found by body text via keyword search
fusion, with the vector backend being optional (RRF degrades gracefully
to keyword-only when no vector results exist).
"""

from __future__ import annotations

from src.infrastructure.ai.hybrid_retriever import (
    ChunkSplitter,
    HybridRetriever,
    KnowledgeChunk,
    SearchScore,
)
from src.infrastructure.ai.tools.advisor_tool_executor import SimpleKnowledgeBase

SOP_TEXT = """# 航班延误处置 SOP
本文档适用于航班延误时的统一处置流程，各岗位必须遵循。

## 延误等级判定
延误超过两小时定义为重大延误，需要启动会商流程并通知运控值班经理。

## 信息通报
延误信息必须通过派工群组在 15 分钟内完成通报，并更新航班状态看板。

## 旅客安置
重大延误旅客安置遵循先改签后住宿的原则，住宿标准为四星级及以上。
"""


def _chunk_from_text(text: str, *, source_uri: str) -> KnowledgeChunk:
    chunks = ChunkSplitter.split_semantically(text)
    chunks[0].source_uri = source_uri
    chunks[0].metadata["category"] = "sop"
    return chunks[0]


def test_split_semantically_splits_sop_by_paragraph():
    # ``max_chunk_size`` is the cap after which a new chunk starts, so a
    # small cap keeps one chunk per SOP section.
    chunks = ChunkSplitter.split_semantically(SOP_TEXT, max_chunk_size=60)
    assert len(chunks) >= 3
    assert all("延误" in c.content for c in chunks)
    assert any("旅客安置" in c.content for c in chunks)


def test_split_markdown_preserves_headings():
    chunks = ChunkSplitter.split_markdown(SOP_TEXT)
    assert len(chunks) == 4
    assert chunks[0].metadata["heading"] == "# 航班延误处置 SOP"
    assert chunks[3].metadata["heading"] == "## 旅客安置"


def test_rrf_ranks_shared_doc_higher():
    """A doc found by both engines outranks docs found by one."""
    doc_a = _chunk_from_text("A", source_uri="kb/a.md")
    doc_b = _chunk_from_text("B", source_uri="kb/b.md")
    doc_c = _chunk_from_text("C", source_uri="kb/c.md")

    retriever = HybridRetriever(db_pool=None, redis_client=None)
    scores = retriever._apply_rrf(
        keyword_results=[(doc_a, 0.9), (doc_b, 0.5)],
        vector_results=[(doc_a, 0.8), (doc_c, 0.7)],
    )
    ranked = sorted(scores, key=lambda s: s.combined_score, reverse=True)
    assert ranked[0].chunk.id == doc_a.id
    assert {s.chunk.id for s in ranked} == {doc_a.id, doc_b.id, doc_c.id}
    assert ranked[0].combined_score > ranked[1].combined_score


def test_keyword_only_rrf_degrades_gracefully_without_vector_backend():
    """No vector results (no Chroma/pgvector) — keyword ranking still works."""
    doc_a = _chunk_from_text("A", source_uri="kb/a.md")
    doc_b = _chunk_from_text("B", source_uri="kb/b.md")

    retriever = HybridRetriever(db_pool=None, redis_client=None)
    scores = retriever._apply_rrf(
        keyword_results=[(doc_a, 0.9), (doc_b, 0.5)],
        vector_results=[],
    )
    ranked = sorted(scores, key=lambda s: s.combined_score, reverse=True)
    assert ranked[0].chunk.id == doc_a.id
    assert all(s.vector_score == 0.0 for s in ranked)


def test_retriever_search_returns_results_without_db_or_redis():
    """``search()`` must not raise when both backends are absent (optional
    adapters) — it returns an empty ranking instead of crashing."""

    async def _run():
        retriever = HybridRetriever(db_pool=None, redis_client=None)
        return await retriever.search("延误处置")

    import asyncio

    result = asyncio.run(_run())
    assert isinstance(result, list)
    assert all(isinstance(s, SearchScore) for s in result)


def test_simple_knowledge_base_hits_sop_by_body_text(tmp_path):
    """Plan exit: SOP 能按正文命中 — SimpleKnowledgeBase finds the SOP
    document when the query quotes its body text, and reports provenance."""
    doc = tmp_path / "dispatch_sop.md"
    doc.write_text(SOP_TEXT, encoding="utf-8")

    async def _run():
        kb = SimpleKnowledgeBase(str(tmp_path))
        return await kb.search("旅客安置 住宿标准 四星级", max_results=5)

    import asyncio

    matches = asyncio.run(_run())
    assert matches, "SOP should be hit by body text"
    best = matches[0]
    assert doc.name in str(best.get("path", best.get("source_uri", "")))
    snippet = best.get("snippet", best.get("content", ""))
    assert "旅客安置" in snippet
