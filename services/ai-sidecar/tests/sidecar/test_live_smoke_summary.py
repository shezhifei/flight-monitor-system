import json


def build_live_smoke_summary(events, model, duration_ms, token_usage):
    model_str = str(model)
    lower_model = model_str.lower()
    for forbidden in ["sk-", "bearer", "api_key", "authorization"]:
        if forbidden in lower_model:
            model_str = "[REDACTED]"
            break

    summary = {
        "model": model_str,
        "first_progress_seen": any(e.get("event") == "progress" for e in events),
        "first_token_seen": any(e.get("event") == "token" for e in events),
        "run_complete_seen": any(e.get("event") == "run.complete" for e in events),
        "degraded": False,
        "duration_ms": duration_ms,
        "token_usage": token_usage,
        "event_counts": {},
    }
    for e in events:
        evt = e.get("event")
        if evt:
            summary["event_counts"][evt] = summary["event_counts"].get(evt, 0) + 1
        if evt == "run.complete" and e.get("data", {}).get("degraded") is True:
            summary["degraded"] = True

    return summary


def format_live_smoke_summary(summary):
    formatted = json.dumps(summary, indent=2)
    lower_fmt = formatted.lower()
    for forbidden in ["sk-", "bearer", "api_key", "authorization"]:
        if forbidden in lower_fmt:
            return '{"error": "redacted due to sensitive terms"}'
    return formatted


def test_summary_counts_and_booleans():
    events = [
        {"event": "progress"},
        {"event": "token", "data": {"delta": "hello"}},
        {"event": "token", "data": {"delta": " world"}},
        {"event": "run.complete", "data": {"degraded": True, "answer": "hello world"}},
    ]
    summary = build_live_smoke_summary(events, "gpt-4", 1500, {"total_tokens": 10})

    assert summary["first_progress_seen"] is True
    assert summary["first_token_seen"] is True
    assert summary["run_complete_seen"] is True
    assert summary["degraded"] is True
    assert summary["event_counts"]["token"] == 2
    assert summary["event_counts"]["progress"] == 1
    assert summary["event_counts"]["run.complete"] == 1

    fmt = format_live_smoke_summary(summary)
    assert "gpt-4" in fmt
    assert "hello" not in fmt  # data/answer should not be in the summary string


def test_summary_redacts_sensitive_model():
    events = []
    summary = build_live_smoke_summary(events, "gpt-4-sk-12345", 100, {})
    assert summary["model"] == "[REDACTED]"

    fmt = format_live_smoke_summary(summary)
    assert "sk-12345" not in fmt


def test_format_refuses_leakage():
    summary = build_live_smoke_summary([], "gpt-4", 100, {"prompt_tokens": 5})
    # Force injection
    summary["event_counts"]["api_key"] = 1
    fmt = format_live_smoke_summary(summary)
    assert "redacted due to sensitive terms" in fmt
