// 搬运自 frontend/ai-react/src/components/chat/subagentTreeModel.ts（无逻辑改动）。
/**
 * Subagent tree state reducer (Task C5).
 *
 * Data sources:
 * - `tool.call` / `tool.result` frames for `delegate_to_subagent` /
 *   `handoff_to_entity` (node lifecycle: spawned → done/error).
 * - `subagent_event` SSE frames bubbled from child runs
 *   (see runtime_service `_streaming_tools.py::_on_child_event` and
 *   `_resolve.py::_sanitize_subagent_event`):
 *   { run_id, event_type, subagent_depth, parent_run_id, tool_name?,
 *     tool_type?, delta? }
 *
 * Write actions in child runs remain proposal_only (dispatcher contract), so
 * every node carries `proposalOnly: true`; a bubbled write_action tool call is
 * surfaced as a proposal-only activity.
 */

export type SubagentNodeStatus = 'running' | 'done' | 'error';

export interface SubagentNodeModel {
  id: string;
  parentRunId?: string;
  depth: number;
  label: string;
  status: SubagentNodeStatus;
  proposalOnly: boolean;
  lastActivity?: string;
  toolCalls: number;
}

export interface DelegateToolEvent {
  toolName: string;
  phase: 'call' | 'result';
  status?: string;
  args?: unknown;
}

export const DELEGATE_TOOL_NAMES: ReadonlySet<string> = new Set([
  'delegate_to_subagent',
  'handoff_to_entity',
]);

function asRecord(value: unknown): Record<string, unknown> | null {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  if (typeof value === 'string' && value.trim().startsWith('{')) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed as Record<string, unknown>;
      }
    } catch (_error) {
      return null;
    }
  }
  return null;
}

function lastRunningIndex(nodes: SubagentNodeModel[], parentRunId?: string): number {
  for (let index = nodes.length - 1; index >= 0; index -= 1) {
    const node = nodes[index];
    if (node.status !== 'running') continue;
    if (!parentRunId || !node.parentRunId || node.parentRunId === parentRunId) {
      return index;
    }
  }
  return -1;
}

/**
 * Fold a delegate/handoff tool frame into the tree. Returns `undefined` when
 * the event is not a delegation tool event.
 */
export function applyDelegateToolEvent(
  nodes: SubagentNodeModel[],
  evt: DelegateToolEvent,
): SubagentNodeModel[] | undefined {
  if (!DELEGATE_TOOL_NAMES.has(evt.toolName)) {
    return undefined;
  }
  if (evt.phase === 'call') {
    const args = asRecord(evt.args);
    const label = String(args?.entity_id || args?.target_entity_id || evt.toolName);
    return [
      ...nodes,
      {
        id: `sub_call_${nodes.length}`,
        depth: nodes.length === 0 ? 1 : Math.max(...nodes.map((node) => node.depth)),
        label,
        status: 'running',
        proposalOnly: true,
        lastActivity: evt.toolName === 'handoff_to_entity' ? 'handoff' : 'delegated',
        toolCalls: 0,
      },
    ];
  }
  const index = lastRunningIndex(nodes);
  if (index < 0) {
    return nodes;
  }
  const succeeded = String(evt.status || 'succeeded').toLowerCase() === 'succeeded';
  const updated = [...nodes];
  updated[index] = {
    ...updated[index],
    status: succeeded ? 'done' : 'error',
    lastActivity: succeeded ? 'completed' : String(evt.status || 'failed'),
  };
  return updated;
}

/**
 * Fold a bubbled `subagent_event` SSE payload into the tree.
 */
export function applySubagentStreamEvent(
  nodes: SubagentNodeModel[],
  payload: Record<string, unknown>,
): SubagentNodeModel[] {
  const parentRunId = String(payload.parent_run_id || payload.run_id || '').trim() || undefined;
  const depth = Number(payload.subagent_depth ?? 1) || 1;
  const eventType = String(payload.event_type || '').toLowerCase();

  let next = nodes;
  let index = lastRunningIndex(next, parentRunId);
  if (index < 0 && parentRunId) {
    index = next.findIndex((node) => node.parentRunId === parentRunId);
  }
  if (index < 0) {
    // No delegate tool frame was observed (sanitized streams omit it): create
    // an implicit node from the bubbled child event.
    next = [
      ...next,
      {
        id: `sub_${parentRunId || 'run'}_${depth}_${next.length}`,
        parentRunId,
        depth,
        label: `subagent@depth${depth}`,
        status: 'running',
        proposalOnly: true,
        toolCalls: 0,
      },
    ];
    index = next.length - 1;
  }

  const node = next[index];
  const updated: SubagentNodeModel = {
    ...node,
    parentRunId: node.parentRunId || parentRunId,
    depth: Math.max(node.depth, depth),
  };
  if (eventType === 'tool_call') {
    updated.toolCalls = node.toolCalls + 1;
    updated.lastActivity = String(payload.tool_name || 'tool_call');
  } else if (eventType === 'tool_result') {
    updated.lastActivity = String(payload.tool_name || 'tool_result');
  } else if (eventType === 'completed') {
    updated.status = 'done';
    updated.lastActivity = 'completed';
  } else if (eventType === 'error' || eventType === 'cancelled') {
    updated.status = 'error';
    updated.lastActivity = eventType;
  } else if (eventType === 'text_delta' && !updated.lastActivity) {
    updated.lastActivity = 'streaming';
  }
  const result = [...next];
  result[index] = updated;
  return result;
}
