"""Tests for the working memory workspace (Task B2).

Asserts:
1. plan.md / notes.md / evidence.json write & read round-trips
2. Tool results above the ~2k token threshold spill: model gets summary + pointer,
   full payload lands in evidence.json
3. Snapshot serialization is JSON-only (no pickle) and round-trips through the
   checkpoint context_snapshot shape used by resume.ResumeContext.working_memory
"""

from __future__ import annotations

import json

from src.infrastructure.ai.resume import RunCheckpoint, RunRestorer, CheckpointLoader
from src.infrastructure.ai.working_memory import (
    DEFAULT_SPILL_TOKEN_THRESHOLD,
    EVIDENCE_FILE,
    NOTES_FILE,
    PLAN_FILE,
    WorkingMemory,
    estimate_tokens,
)


def _run(coro):
    import asyncio

    return asyncio.run(coro)


class TestPlanAndNotes:
    def test_plan_write_read(self):
        wm = WorkingMemory(run_id="run_b2_1")
        assert wm.read_plan() == ""

        wm.write_plan("1. 查询 F1234 状态\n2. 评估机位冲突")
        assert "F1234" in wm.read_plan()

        wm.write_plan("1. 重新规划")
        assert "F1234" not in wm.read_plan()

    def test_notes_append_read(self):
        wm = WorkingMemory(run_id="run_b2_2")
        wm.append_notes("当前航班: F1234")
        wm.append_notes("冲突: 机位 A12 与 F5678 重叠")
        wm.append_notes("   ")  # blank notes are ignored

        notes = wm.read_notes()
        assert "F1234" in notes
        assert "F5678" in notes
        assert notes.count("\n") == 1


class TestEvidence:
    def test_add_and_read_evidence(self):
        wm = WorkingMemory(run_id="run_b2_3")
        record = wm.add_evidence(
            source="list_flights",
            object_id="F1234",
            summary="flight list excerpt",
            content='{"flights": ["F1234"]}',
        )

        assert record.source == "list_flights"
        assert record.object_id == "F1234"
        assert record.pointer == f"{EVIDENCE_FILE}#{record.evidence_id}"

        evidence = wm.read_evidence()
        assert len(evidence) == 1
        assert evidence[0]["source"] == "list_flights"
        assert evidence[0]["object_id"] == "F1234"
        assert wm.get_evidence_content(record.pointer) == '{"flights": ["F1234"]}'

    def test_evidence_ids_are_stable_sequence(self):
        wm = WorkingMemory(run_id="run_b2_4")
        first = wm.add_evidence(source="t1", object_id="", summary="s", content="c")
        second = wm.add_evidence(source="t2", object_id="", summary="s", content="c")
        assert first.evidence_id == "ev-0001"
        assert second.evidence_id == "ev-0002"


class TestSpill:
    def test_small_result_does_not_spill(self):
        wm = WorkingMemory(run_id="run_b2_5")
        assert wm.should_spill('{"ok": true}') is False

    def test_large_result_spills_summary_plus_pointer(self):
        wm = WorkingMemory(run_id="run_b2_6")
        # ~4k tokens of ASCII (ascii/4 heuristic) — above the 2k threshold.
        big_content = json.dumps({"rows": ["x" * 100] * 160})
        assert estimate_tokens(big_content) > DEFAULT_SPILL_TOKEN_THRESHOLD

        fed_to_model = wm.spill_tool_result(
            tool_name="list_flights",
            content=big_content,
            object_id="F1234",
        )

        # Model receives summary + pointer, not the raw payload.
        assert "pointer:" in fed_to_model
        assert f"{EVIDENCE_FILE}#ev-0001" in fed_to_model
        assert "summary:" in fed_to_model
        assert len(fed_to_model) < len(big_content)

        # Full payload is retrievable from the workspace.
        assert wm.get_evidence_content("ev-0001") == big_content
        evidence = wm.read_evidence()[0]
        assert evidence["source"] == "list_flights"
        assert evidence["object_id"] == "F1234"

    def test_custom_threshold(self):
        wm = WorkingMemory(run_id="run_b2_7", spill_token_threshold=10)
        assert wm.should_spill("x" * 100) is True


class TestSerialization:
    def test_to_dict_is_json_serializable_no_pickle(self):
        wm = WorkingMemory(run_id="run_b2_8")
        wm.write_plan("step 1")
        wm.append_notes("note")
        wm.add_evidence(source="t", object_id="F1234", summary="s", content="payload")

        snapshot = wm.to_dict()
        # Must be plain JSON — checkpoints persist this via JSONB, pickle is banned.
        encoded = json.dumps(snapshot, ensure_ascii=False)
        assert json.loads(encoded)[PLAN_FILE] == "step 1"
        assert snapshot[NOTES_FILE] == "note"
        assert snapshot[EVIDENCE_FILE][0]["object_id"] == "F1234"

    def test_round_trip(self):
        wm = WorkingMemory(run_id="run_b2_9")
        wm.write_plan("1. 未完成步骤")
        wm.append_notes("结论: F1234 延误")
        wm.spill_tool_result(tool_name="list_flights", content="y" * 9000, object_id="F1234")

        restored = WorkingMemory.from_dict(json.loads(json.dumps(wm.to_dict(), ensure_ascii=False)))

        assert restored.run_id == "run_b2_9"
        assert restored.read_plan() == wm.read_plan()
        assert restored.read_notes() == wm.read_notes()
        assert restored.read_evidence() == wm.read_evidence()

    def test_from_dict_accepts_nested_checkpoint_shape(self):
        wm = WorkingMemory(run_id="run_b2_10")
        wm.append_notes("nested")
        snapshot = {
            "messages_count": 7,
            "working_memory": wm.to_dict(),
        }
        restored = WorkingMemory.from_dict(snapshot)
        assert restored.read_notes() == "nested"

    def test_checkpoint_context_snapshot_round_trip(self):
        """Snapshot survives the resume path: checkpoint context_snapshot -> ResumeContext."""
        wm = WorkingMemory(run_id="run_b2_11")
        wm.write_plan("1. 恢复后续步骤")
        wm.append_notes("当前航班: F1234")
        wm.spill_tool_result(tool_name="list_flights", content="z" * 9000, object_id="F1234")

        # Persist into the checkpoint payload the way the runner emits it.
        context_snapshot = json.loads(
            json.dumps({"working_memory": wm.to_dict()}, ensure_ascii=False)
        )
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_b2",
            run_id="run_b2_11",
            checkpoint_type="after_tool",
            round_index=1,
            created_at=0.0,
            context_snapshot=context_snapshot,
        )

        restorer = RunRestorer(CheckpointLoader.get_instance())
        resume_ctx = _run(restorer.restore_to_checkpoint(checkpoint))

        restored = WorkingMemory.from_dict(resume_ctx.working_memory)
        assert restored.read_plan() == "1. 恢复后续步骤"
        assert "F1234" in restored.read_notes()
        assert restored.get_evidence_content("ev-0001") == "z" * 9000
