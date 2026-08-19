// 搬运自 frontend/ai-react/src/features/ai-monitor/pendingActionRows.ts（无逻辑改动）。
import { normalizeTime } from './utils';

export interface PendingRow {
  key: string;
  actionId: string;
  toolName: string;
  status: string;
  createdAt: string;
  expiresAt: string;
  raw: Record<string, unknown>;
}

function toPendingRow(action: Record<string, unknown>, previous?: PendingRow): PendingRow | null {
  const actionId = String(action.action_id || previous?.actionId || '').trim();
  if (!actionId) {
    return null;
  }

  return {
    key: actionId,
    actionId,
    toolName: String(action.tool_name || previous?.toolName || ''),
    status: String(action.status || previous?.status || ''),
    createdAt: normalizeTime(String(action.created_at || previous?.createdAt || '')),
    expiresAt: normalizeTime(String(action.expires_at || previous?.expiresAt || '')),
    raw: {
      ...(previous?.raw || {}),
      ...action,
    },
  };
}

export function applyPendingActionEvent(
  rows: PendingRow[],
  semantic: string,
  action: Record<string, unknown> | null | undefined,
): PendingRow[] {
  if (!action || typeof action !== 'object') {
    return rows;
  }

  const actionId = String(action.action_id || '').trim();
  if (!actionId) {
    return rows;
  }

  if (semantic === 'approval_result') {
    return rows.filter((row) => row.actionId !== actionId);
  }

  if (semantic !== 'approval_required') {
    return rows;
  }

  const existing = rows.find((row) => row.actionId === actionId);
  const nextRow = toPendingRow(action, existing);
  if (!nextRow) {
    return rows;
  }

  const filtered = rows.filter((row) => row.actionId !== actionId);
  return [...filtered, nextRow];
}
