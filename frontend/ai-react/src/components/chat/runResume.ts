/**
 * Run resume / compression notice helpers (Task C5).
 *
 * REST contract (Rust control plane, crates/api/src/routes/ai_resume.rs):
 * - GET  /api/v2/ai/jobs/{job_id}/runs/{run_id}/checkpoints
 *     → { items: [{ checkpoint_id, sequence_no, checkpoint_type, created_at, ... }] }
 * - POST /api/v2/ai/runs/{run_id}/resume  (optional body { from_checkpoint_id })
 * - DELETE /api/v2/ai/jobs/{job_id}  (cancel via command queue, ai_jobs.rs)
 *
 * Compression contract (sidecar `_streaming_tools.py::_apply_context_budget`):
 * SSE `context.compressed` → { run_id, strategy, before_tokens, after_tokens,
 * summary_model, persisted, latency_ms }.
 */

export interface RunCheckpointItem {
  checkpointId: string;
  sequenceNo: number;
  checkpointType: string;
  createdAt?: string;
}

/** Checkpoint types that the resume endpoint can recover from. */
const RECOVERABLE_CHECKPOINT_TYPES: ReadonlySet<string> = new Set(['before_tool', 'after_tool']);

export function normalizeCheckpoint(row: Record<string, unknown>): RunCheckpointItem {
  return {
    checkpointId: String(row.checkpoint_id || ''),
    sequenceNo: Number(row.sequence_no ?? 0) || 0,
    checkpointType: String(row.checkpoint_type || '').toLowerCase(),
    createdAt: row.created_at ? String(row.created_at) : undefined,
  };
}

/** Latest recoverable (before_tool / after_tool) checkpoint by sequence number. */
export function latestRecoverableCheckpoint(items: RunCheckpointItem[]): RunCheckpointItem | null {
  let best: RunCheckpointItem | null = null;
  for (const item of items) {
    if (!RECOVERABLE_CHECKPOINT_TYPES.has(item.checkpointType)) {
      continue;
    }
    if (!best || item.sequenceNo >= best.sequenceNo) {
      best = item;
    }
  }
  return best;
}

export interface CompressionNoticeModel {
  strategy?: string;
  beforeTokens?: number;
  afterTokens?: number;
  latencyMs?: number;
  at: string;
}

export function toCompressionNotice(payload: Record<string, unknown>): CompressionNoticeModel {
  const toNumber = (value: unknown): number | undefined => {
    const num = Number(value);
    return Number.isFinite(num) && num > 0 ? num : undefined;
  };
  return {
    strategy: payload.strategy ? String(payload.strategy) : undefined,
    beforeTokens: toNumber(payload.before_tokens),
    afterTokens: toNumber(payload.after_tokens),
    latencyMs: toNumber(payload.latency_ms),
    at: new Date().toLocaleTimeString(),
  };
}
