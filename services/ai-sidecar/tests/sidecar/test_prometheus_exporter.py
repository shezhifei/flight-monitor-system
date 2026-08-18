"""Task J1 — agent metrics labelled by task type with run-cost accounting.

Covers the Prometheus export surface:

* existing metrics gain ``task_type`` / ``entity_id`` / ``status`` /
  ``blocked_by`` labels;
* the run metric context bound at the run entry point feeds bridge helpers
  that are called without explicit labels;
* ``fms_ai_run_cost_usd`` grows from the model price table, and unknown
  models record zero cost plus ``fms_ai_price_missing_total``;
* every bridge helper stays safe when ``prometheus_client`` is absent
  (``_NoopMetric`` fallback).

The value assertions only run when ``prometheus_client`` is installed;
the context/price semantics are pure Python and always asserted.
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.monitoring import prometheus_exporter as exporter
from src.infrastructure.ai.monitoring.model_prices import (
    MODEL_PRICES_PER_1M,
    estimate_run_cost_usd,
    lookup_price_per_1m,
)

PROM = exporter._PROM_AVAILABLE


# ---------------------------------------------------------------------------
# Price table semantics
# ---------------------------------------------------------------------------


def test_price_lookup_longest_prefix_wins():
    # "gpt-4o-mini-2024-07-18" must match gpt-4o-mini, not gpt-4o.
    assert lookup_price_per_1m("gpt-4o-mini-2024-07-18") == MODEL_PRICES_PER_1M["gpt-4o-mini"]
    assert lookup_price_per_1m("GPT-4o") == MODEL_PRICES_PER_1M["gpt-4o"]


def test_price_lookup_unknown_model_is_none():
    assert lookup_price_per_1m("claude-3-opus") is None
    assert lookup_price_per_1m("") is None


def test_estimate_cost_known_model():
    cost, missing = estimate_run_cost_usd("gpt-4o", 1_000_000, 500_000)
    assert missing is False
    prompt_price, completion_price = MODEL_PRICES_PER_1M["gpt-4o"]
    assert cost == pytest.approx(prompt_price + completion_price / 2.0)


def test_estimate_cost_unknown_model_is_zero_and_flagged():
    cost, missing = estimate_run_cost_usd("some-unpiced-model", 123, 456)
    assert cost == 0.0
    assert missing is True


# ---------------------------------------------------------------------------
# Run metric context
# ---------------------------------------------------------------------------


def test_metric_context_defaults_to_unknown():
    assert exporter.current_task_type() == "unknown"
    assert exporter.current_entity_id() == "unknown"


def test_bind_metric_context_sets_and_restores():
    with exporter.bind_metric_context(task_type="dispatch_ops", entity_id="FLIGHT-1"):
        assert exporter.current_task_type() == "dispatch_ops"
        assert exporter.current_entity_id() == "FLIGHT-1"
    assert exporter.current_task_type() == "unknown"
    assert exporter.current_entity_id() == "unknown"


def test_bind_metric_context_normalizes_empty_values():
    with exporter.bind_metric_context(task_type="  ", entity_id=None):
        assert exporter.current_task_type() == "unknown"
        assert exporter.current_entity_id() == "unknown"


# ---------------------------------------------------------------------------
# Bridge helpers: safe in noop mode, labelled when prometheus_client exists
# ---------------------------------------------------------------------------


def test_bridge_helpers_are_noop_safe():
    # Must not raise regardless of whether prometheus_client is installed.
    with exporter.bind_metric_context(task_type="query_ops", entity_id="FLT123"):
        exporter.inc_llm_call("gpt-4o", status="ok")
        exporter.inc_llm_call("gpt-4o", status="error")
        exporter.observe_tokens("gpt-4o", 100, 50)
        exporter.observe_run_cost("gpt-4o", 100, 50)
        exporter.inc_tool_call("ontology.lookup", "success")
        exporter.inc_tool_call("assign_gate", "blocked", blocked_by="lease")
        exporter.observe_tool_duration("ontology.lookup", 0.12)
        exporter.observe_request_latency(0.5, "stream_run")
        exporter.inc_error("ungrounded")
        exporter.inc_mq_gate_decision("public_direct")
        exporter.observe_first_progress(1.2)
        exporter.inc_resume("success")
        exporter.inc_resume("failed")
        exporter.inc_proposal("ontology.propose_action", "created")
        exporter.inc_proposal("ontology.propose_action", "approved")


def _counter_value(metric, **labels) -> float:
    return metric.labels(**labels)._value.get()


def _unlabeled_value(metric) -> float:
    return metric._value.get()


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_llm_call_counter_labels_task_type_entity_status():
    labels = dict(model="gpt-4o", task_type="query_ops", entity_id="FLT1", status="ok")
    before = _counter_value(exporter.fms_ai_llm_calls_total, **labels)
    with exporter.bind_metric_context(task_type="query_ops", entity_id="FLT1"):
        exporter.inc_llm_call("gpt-4o")
    assert _counter_value(exporter.fms_ai_llm_calls_total, **labels) == before + 1


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_tool_call_counter_labels_blocked_by():
    labels = dict(tool="assign_gate", task_type="dispatch_ops", status="blocked", blocked_by="lease")
    before = _counter_value(exporter.fms_ai_tool_calls_total, **labels)
    with exporter.bind_metric_context(task_type="dispatch_ops"):
        exporter.inc_tool_call("assign_gate", "blocked", blocked_by="lease")
    assert _counter_value(exporter.fms_ai_tool_calls_total, **labels) == before + 1


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_tokens_counter_labelled_by_task_type():
    labels = dict(model="gpt-4o", type="prompt", task_type="anomaly_ops")
    before = _counter_value(exporter.fms_ai_tokens_total, **labels)
    with exporter.bind_metric_context(task_type="anomaly_ops"):
        exporter.observe_tokens("gpt-4o", 250, 0)
    assert _counter_value(exporter.fms_ai_tokens_total, **labels) == before + 250


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_run_cost_counter_grows_for_priced_model():
    labels = dict(task_type="dispatch_ops", entity_id="FLT9")
    before = _counter_value(exporter.fms_ai_run_cost_usd, **labels)
    price_missing_before = _unlabeled_value(exporter.fms_ai_price_missing_total)
    with exporter.bind_metric_context(task_type="dispatch_ops", entity_id="FLT9"):
        exporter.observe_run_cost("gpt-4o", 1_000_000, 0)
    expected = MODEL_PRICES_PER_1M["gpt-4o"][0]
    assert _counter_value(exporter.fms_ai_run_cost_usd, **labels) == pytest.approx(before + expected)
    assert _unlabeled_value(exporter.fms_ai_price_missing_total) == price_missing_before


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_run_cost_unknown_model_records_zero_and_price_missing():
    labels = dict(task_type="query_ops", entity_id="unknown")
    before = _counter_value(exporter.fms_ai_run_cost_usd, **labels)
    price_missing_before = _unlabeled_value(exporter.fms_ai_price_missing_total)
    with exporter.bind_metric_context(task_type="query_ops"):
        exporter.observe_run_cost("mystery-llm-9000", 5000, 5000)
    assert _counter_value(exporter.fms_ai_run_cost_usd, **labels) == before
    assert _unlabeled_value(exporter.fms_ai_price_missing_total) == price_missing_before + 1


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_resume_and_proposal_counters():
    before_resume = _counter_value(exporter.fms_ai_resume_total, status="success")
    exporter.inc_resume("success")
    assert _counter_value(exporter.fms_ai_resume_total, status="success") == before_resume + 1

    labels = dict(action="ontology.propose_action", status="created")
    before_proposal = _counter_value(exporter.fms_ai_proposal_total, **labels)
    exporter.inc_proposal("ontology.propose_action", "created")
    assert _counter_value(exporter.fms_ai_proposal_total, **labels) == before_proposal + 1


@pytest.mark.skipif(not PROM, reason="prometheus_client not installed")
def test_first_progress_histogram_labelled_by_task_type():
    with exporter.bind_metric_context(task_type="query_ops"):
        exporter.observe_first_progress(0.9)
    # Histogram exposes a labelled sum; assert the sample landed.
    labelled = exporter.fms_ai_first_progress_seconds.labels(task_type="query_ops")
    assert labelled._sum.get() >= 0.9
