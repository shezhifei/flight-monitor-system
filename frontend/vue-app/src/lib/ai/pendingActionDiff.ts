// 搬运自 frontend/ai-react/src/components/chat/pendingActionDiff.ts（无逻辑改动）。
/**
 * K3: ontology-aware diff mapping for pending action approval cards.
 *
 * Pure mapping layer: folds the raw SSE `pending_action` payload into the
 * card model. Two upstream shapes are supported:
 * - sidecar pending store (`action_id` / `entity_type` / `before_snapshot`
 *   / `json_patch` / `ui_hints` at the top level), and
 * - ontology write proposals (`{"status":"proposal_created","proposal":{...}}`
 *   where `proposal.simulate` carries the I3 before/after/violations block).
 *
 * Everything is optional and defensive: a payload without constraints or
 * snapshots still produces a valid model (no crash, no fabricated diff).
 */

export interface PendingActionConstraint {
  name: string;
  kind: string;
  passed: boolean;
  message?: string;
}

export interface PendingActionDiffRow {
  field: string;
  before: string;
  after: string;
}

export interface PendingActionCardModel {
  actionId: string;
  toolName?: string;
  status?: string;
  message?: string;
  createdAt?: string;
  expiresAt?: string;
  objectType?: string;
  objectId?: string;
  diffRows?: PendingActionDiffRow[];
  hardViolations?: PendingActionConstraint[];
  softViolations?: PendingActionConstraint[];
  irreversible?: boolean;
  sourceRunId?: string;
  sourceTool?: string;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asString(value: unknown): string {
  if (value === null || value === undefined) {
    return '';
  }
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function cleanPath(path: unknown): string {
  const text = String(path || '').trim();
  return text.startsWith('/') ? text.slice(1).replace(/\//g, '.') : text;
}

function rowsFromPatch(patch: unknown): PendingActionDiffRow[] {
  if (!Array.isArray(patch)) {
    return [];
  }
  const rows: PendingActionDiffRow[] = [];
  for (const op of patch) {
    const item = asRecord(op);
    if (!item) {
      continue;
    }
    const field = cleanPath(item.path);
    if (!field) {
      continue;
    }
    const opName = String(item.op || '').toLowerCase();
    if (opName === 'remove') {
      rows.push({ field, before: asString(item.value), after: '—' });
    } else {
      rows.push({ field, before: opName === 'add' ? '—' : '', after: asString(item.value) });
    }
  }
  return rows;
}

function rowsFromSnapshots(before: unknown, after: unknown): PendingActionDiffRow[] {
  const beforeRecord = asRecord(before) || {};
  const afterRecord = asRecord(after) || {};
  const keys = Array.from(new Set([...Object.keys(beforeRecord), ...Object.keys(afterRecord)]));
  const rows: PendingActionDiffRow[] = [];
  for (const key of keys) {
    const beforeText = asString(beforeRecord[key]);
    const afterText = asString(afterRecord[key]);
    if (beforeText === afterText) {
      continue;
    }
    rows.push({ field: key, before: beforeText || '—', after: afterText || '—' });
  }
  return rows;
}

export function extractDiffRows(raw: Record<string, unknown>): PendingActionDiffRow[] {
  const proposal = asRecord(raw.proposal);
  const simulate = asRecord(proposal?.simulate) || asRecord(raw.simulate);

  if (simulate) {
    return rowsFromSnapshots(simulate.before, simulate.after);
  }

  const patchRows = rowsFromPatch(raw.json_patch);
  if (patchRows.length > 0) {
    return patchRows;
  }

  const before = raw.before_snapshot;
  const after = raw.after_snapshot;
  if ((before && typeof before === 'object') || (after && typeof after === 'object')) {
    return rowsFromSnapshots(before, after);
  }

  return [];
}

function fromViolations(violations: unknown): PendingActionConstraint[] {
  if (!Array.isArray(violations)) {
    return [];
  }
  const rows: PendingActionConstraint[] = [];
  for (const violation of violations) {
    const item = asRecord(violation);
    if (!item) {
      continue;
    }
    const name = String(
      item.constraint_name || item.rule_id || item.name || item.message || 'constraint',
    ).trim();
    rows.push({
      name: name || 'constraint',
      kind: String(item.severity || item.kind || 'hard').toLowerCase(),
      passed: false,
      message: item.message ? String(item.message) : undefined,
    });
  }
  return rows;
}

function fromConstraintResults(results: unknown): PendingActionConstraint[] {
  if (!Array.isArray(results)) {
    return [];
  }
  const rows: PendingActionConstraint[] = [];
  for (const result of results) {
    const item = asRecord(result);
    if (!item) {
      continue;
    }
    const name = String(item.constraint_name || item.name || '').trim();
    if (!name) {
      continue;
    }
    rows.push({
      name,
      kind: String(item.kind || item.severity || 'soft').toLowerCase(),
      passed: Boolean(item.passed),
      message: item.message ? String(item.message) : undefined,
    });
  }
  return rows;
}

function fromAvailability(availability: unknown): PendingActionConstraint[] {
  const record = asRecord(availability);
  if (!record || record.is_available !== false) {
    return [];
  }
  const conflicts = Array.isArray(record.conflicts) && record.conflicts.length > 0
    ? record.conflicts.map((item) => asString(item)).join('; ')
    : undefined;
  return [
    {
      name: 'stand_available',
      kind: 'hard',
      passed: false,
      message: conflicts || '目标资源不可用',
    },
  ];
}

/**
 * Collect constraint outcomes and split them: failing hard constraints are
 * violations (red), failing non-hard constraints are soft warnings (yellow).
 * Passed constraints are informational and not surfaced on the card.
 */
export function extractConstraints(raw: Record<string, unknown>): {
  hard: PendingActionConstraint[];
  soft: PendingActionConstraint[];
} {
  const proposal = asRecord(raw.proposal);
  const simulate = asRecord(proposal?.simulate) || asRecord(raw.simulate);

  const collected: PendingActionConstraint[] = [
    ...fromViolations(simulate?.violations),
    ...fromAvailability(simulate?.availability),
    ...fromConstraintResults(proposal?.constraint_results),
    ...fromConstraintResults(raw.constraint_results),
  ];

  const hard: PendingActionConstraint[] = [];
  const soft: PendingActionConstraint[] = [];
  for (const item of collected) {
    if (item.passed) {
      continue;
    }
    // Rust ConstraintResult.kind is business/soft: a failing business
    // constraint is a hard violation; only advisory kinds stay soft.
    if (item.kind === 'hard' || item.kind === 'critical' || item.kind === 'business') {
      hard.push(item);
    } else {
      soft.push(item);
    }
  }
  return { hard, soft };
}

function isIrreversible(raw: Record<string, unknown>, proposal: Record<string, unknown> | null): boolean {
  if (raw.irreversible === true || proposal?.irreversible === true) {
    return true;
  }
  const levels = [
    String(raw.operation_level || '').toUpperCase(),
    String(raw.risk_level || '').toUpperCase(),
  ];
  return levels.some((level) => level === 'CRITICAL_WRITE' || level === 'IRREVERSIBLE');
}

export function toPendingActionCardModel(raw: Record<string, unknown>): PendingActionCardModel {
  const proposal = asRecord(raw.proposal);
  const simulate = asRecord(proposal?.simulate) || asRecord(raw.simulate);
  const diffRows = extractDiffRows(raw);
  const { hard, soft } = extractConstraints(raw);

  const model: PendingActionCardModel = {
    actionId: String(raw.action_id || proposal?.proposal_id || proposal?.id || '').trim(),
    toolName: String(raw.tool_name || proposal?.tool_name || proposal?.action_name || '').trim() || undefined,
    status: String(raw.status === 'proposal_created' ? 'pending' : raw.status || proposal?.status || 'pending'),
    message: String(raw.message || raw.reason || proposal?.reasoning || '').trim() || undefined,
    createdAt: raw.created_at ? String(raw.created_at) : undefined,
    expiresAt: raw.expires_at ? String(raw.expires_at) : undefined,
  };

  const objectType = String(
    raw.entity_type || proposal?.object_type || simulate?.object_type || '',
  ).trim();
  const objectId = String(
    raw.entity_id
      || proposal?.object_id
      || simulate?.flight_id
      || simulate?.object_id
      || '',
  ).trim();
  if (objectType || objectId) {
    model.objectType = objectType || 'Unknown';
    model.objectId = objectId || '-';
  }

  if (diffRows.length > 0) {
    model.diffRows = diffRows;
  }
  if (hard.length > 0) {
    model.hardViolations = hard;
  }
  if (soft.length > 0) {
    model.softViolations = soft;
  }
  if (isIrreversible(raw, proposal)) {
    model.irreversible = true;
  }

  const runId = String(raw.run_id || proposal?.run_id || '').trim();
  if (runId) {
    model.sourceRunId = runId;
  }
  const sourceTool = String(raw.tool_name || proposal?.tool_name || '').trim();
  if (sourceTool) {
    model.sourceTool = sourceTool;
  }

  return model;
}
