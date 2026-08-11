# AI Runtime Contract Versioning

> **Status:** Active
> **Date:** 2026-07-11
> **Owners:** Architecture team
> **Scope:** The cross-language (Rust ↔ Python) AI runtime wire contract and its enforcement.

---

## Purpose

The AI runtime spans two languages: the Rust API server (`services/api-server`) and
the Python AI sidecar (`services/ai-sidecar`). They exchange JSON over the wire and
over the message queue. Because the two type systems are maintained independently,
a field added on one side and forgotten on the other is a silent data-loss bug —
exactly the class of defect that produced the `token_usage` drop fixed in W2-1.

This document is the single reference for **which version axes exist, what each one
governs, and the rule for changing them without breaking the other side.**

---

## The three (plus one legacy) version axes

There is no single "contract version" number. Four independent version markers
coexist in the codebase; conflating them is a common source of confusion.

### Axis 1 — `contract_version` string tags (`ai-*.v1`)

The primary structural contract between Rust and Python. These are string tags
carried inside the payload body:

| Contract | Tag value | Rust source | Python source |
| --- | --- | --- | --- |
| Context envelope (request) | `ai-runtime.v1` | `crates/domain/src/models/ai_context_envelope.rs:6` | `src/infrastructure/ai/context_envelope.py:62` |
| Structured output (response) | `ai-structured-output.v1` | `crates/domain/src/models/ai_structured_output.rs:6` | `src/infrastructure/ai/structured_output.py:42` |

Rust actively validates the response tag: `ai_output_validator.rs:32` rejects any
`AiStructuredOutput` whose `contract_version != "ai-structured-output.v1"`. This is
a hard gate on the live path — bumping the tag without updating both sides fails
validation at runtime.

**This axis owns the field sets enforced by the drift gate (see below).**

### Axis 2 — `schema_version` numeric (MQ event envelope)

A separate `u32` on the message-queue event envelope, unrelated to Axis 1. It
versions the *transport envelope* wrapping runtime events, not the AI payload:

- Rust: `crates/domain/src/ai_runtime_event.rs:85` (`pub schema_version: u32`),
  default `1` (`default_schema_version()` at `:89`).
- Python: `SCHEMA_VERSION` constant, asserted in
  `tests/sidecar/governance/test_ai_runtime_event_publisher.py:69` and serialized as
  `"schema_version":1`.

Both sides currently pin `1`. Being numeric with a serde `default`, it tolerates
absence gracefully — an older consumer reading a newer producer still deserializes.

### Axis 3 — legacy `"contract_version": "2.0"` meta blocks

An **unrelated, pre-existing** version string embedded in tool/report `meta`
blocks. It does **not** describe the Rust↔Python runtime contract and must not be
confused with Axis 1. Known sites:

- Python: `tools/base.py:231`, `graph/callbacks.py:70`,
  `todo_agent_executor/_executor_core.py:172`,
  `nl_query_service/service.py:871,970,972`.
- Rust: `ai_route_service.rs:193,335,393,451`,
  `ai_runtime_service/service.rs:266,318`.

Treat as legacy metadata. Leave it alone unless doing a dedicated cleanup; do not
"align" it with Axis 1.

### Axis 4 — legacy `"1.0"` / report `schema_version` meta

A further legacy marker on the streaming-finalizer and report paths, also unrelated
to Axis 1:

- Rust: `streaming_finalizer.rs:418,441,473,484,525,539` (`"contract_version": "1.0"`).
- Python: `tools/report_tool_executor.py:221` (`"schema_version": "1.0"`),
  `aip/approval_diff.py:256`.

Same guidance as Axis 3: legacy metadata, out of scope for runtime-contract changes.

---

## Enforcement: the drift gate (Axis 1)

Field-set parity for Axis 1 is enforced in **tests only**, never in production
models. The live finalize path must keep tolerating unknown/missing fields at
runtime (the W0-1 graceful-degradation hardening) — so we do **not** add serde
`deny_unknown_fields` or pydantic `extra="forbid"` to the live contract structs.
Drift is caught in CI instead.

Two artifacts under `services/ai-sidecar/tests/fixtures/` are the source of truth:

1. **`contract_field_manifest.json`** — enumerates the exact wire field set for
   every contract type, per contract (`context_envelope_contract`,
   `structured_output_contract`). Its `python_internal_fields` section lists fields
   that exist on the Python pydantic models but are deliberately **not** on the wire
   (`ContextEnvelope.conversation_history`, `EnvelopeRequester.permissions`); the
   Python introspection test subtracts these before comparing.

2. **`runtime_contract.json`** — an *exhaustive* fixture in which every optional and
   nested field is populated, so both sides' round-trip tests exercise the full
   field set (not just the required subset).

Both languages assert against these:

- **Python** — `tests/sidecar/test_shared_fixture.py`: introspects
  `model_fields` on each pydantic model and asserts (minus internals) it equals the
  manifest; also asserts the exhaustive fixture covers the manifest.
- **Rust** — `crates/api/src/routes/nl_query/tests.rs`:
  `test_shared_fixture_round_trips_without_field_drift` loads the manifest and
  fixture (via `test_support::load_contract_field_manifest` /
  `load_shared_runtime_contract_fixture`), round-trips `ContextEnvelope` and
  `AiStructuredOutput`, and asserts recursive key parity against the manifest.

Because both tests assert against the same manifest, **adding or removing a field on
either side fails CI until the manifest and the exhaustive fixture are consciously
updated.** CI already runs both: the Python sidecar job runs `pytest tests/sidecar`
and the Rust job runs `cargo test` (`.github/workflows/ci.yml`) — no extra wiring.

---

## Breaking-change rule (Axis 1)

Any change to the Axis-1 field set or tag value is a contract change. To make one:

1. **Decide wire vs. internal.** A field only one side needs and never transmits is
   *internal* — add it to `python_internal_fields` in the manifest, do not put it on
   the Rust struct or in the fixture. A field crossing the wire is *contract* — it
   must appear on **both** the Rust struct and the Python model.

2. **Update all four artifacts in the same change:**
   - the Rust struct (`crates/domain/src/models/...`),
   - the Python pydantic model (`src/infrastructure/ai/...`),
   - `contract_field_manifest.json` (field list),
   - `runtime_contract.json` (populate the new field exhaustively).

3. **Bump the version tag only for a breaking change.** Additive, backward-tolerant
   fields (both sides default them, à la `token_usage` with `#[serde(default)]` /
   pydantic default) keep `v1`. Renames, removals, or type/semantics changes that an
   old peer cannot deserialize require a new tag (`ai-*.v2`) **and** a documented
   compatibility window during which both tags are accepted, until every deployed
   peer has rolled forward.

4. **Never reject on the live path to enforce a field.** Field-set enforcement lives
   in the drift-gate tests. The runtime must degrade gracefully.

5. **Verify both sides before merging:**
   - `.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_shared_fixture.py`
   - `cargo test -p fms-api --lib shared_fixture` (from `services/api-server`)

---

## Quick reference

| Axis | Marker | Governs | Change discipline |
| --- | --- | --- | --- |
| 1 | `contract_version: "ai-*.v1"` | Rust↔Python request/response contract | Drift gate + breaking-change rule above |
| 2 | `schema_version: u32` | MQ event envelope transport | Numeric, serde-default tolerant; bump on envelope change |
| 3 | `contract_version: "2.0"` | Legacy tool/report meta | Legacy — leave as-is |
| 4 | `contract_version`/`schema_version: "1.0"` | Legacy finalizer/report meta | Legacy — leave as-is |
