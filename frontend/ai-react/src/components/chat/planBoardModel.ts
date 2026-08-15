/**
 * Plan board state reducer (Task C5).
 *
 * Data source: plan-tool calls observed in the SSE tool event stream
 * (`update_plan` / `complete_plan_step` / `list_plan_steps`, see
 * services/ai-sidecar `tools/plan_tools.py`). The sidecar emits no dedicated
 * plan SSE event, so the board is derived from the tool timeline events:
 *
 * - `tool.call`  { tool_name, arguments? }  — arguments carry plan_description/steps
 *   on the nl-query stream; the sanitized runtime stream omits them.
 * - `tool.result` { tool_name, result_status } — completion of a plan tool call.
 */

export type PlanStepStatus = 'pending' | 'in_progress' | 'done' | 'blocked';

export interface PlanStepModel {
  id: string;
  description: string;
  status: PlanStepStatus;
  assignedTo?: string;
  error?: string;
}

export interface PlanBoardModel {
  description: string;
  steps: PlanStepModel[];
}

export interface PlanToolEvent {
  toolName: string;
  phase: 'call' | 'result';
  /** result_status from tool.result frames ("succeeded" / "failed" / ...). */
  status?: string;
  /** Raw tool arguments (object or JSON string) when the stream includes them. */
  args?: unknown;
  /** Raw tool result payload when the stream includes it. */
  result?: unknown;
}

export const PLAN_TOOL_NAMES: ReadonlySet<string> = new Set([
  'update_plan',
  'complete_plan_step',
  'list_plan_steps',
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

function normalizeStepStatus(raw: unknown): PlanStepStatus {
  const value = String(raw || '').toLowerCase();
  if (value === 'completed' || value === 'done' || value === 'succeeded') return 'done';
  if (value === 'in_progress' || value === 'running') return 'in_progress';
  if (value === 'blocked' || value === 'failed') return 'blocked';
  return 'pending';
}

function upsertStep(steps: PlanStepModel[], next: PlanStepModel): PlanStepModel[] {
  const index = steps.findIndex((step) => step.id === next.id);
  if (index < 0) {
    return [...steps, next];
  }
  const updated = [...steps];
  updated[index] = { ...steps[index], description: next.description, assignedTo: next.assignedTo };
  return updated;
}

function promoteNextPending(steps: PlanStepModel[]): PlanStepModel[] {
  if (steps.some((step) => step.status === 'in_progress')) {
    return steps;
  }
  const index = steps.findIndex((step) => step.status === 'pending');
  if (index < 0) {
    return steps;
  }
  const updated = [...steps];
  updated[index] = { ...updated[index], status: 'in_progress' };
  return updated;
}

function applyUpdatePlan(state: PlanBoardModel | null, args: Record<string, unknown> | null): PlanBoardModel {
  const base: PlanBoardModel = state || { description: '', steps: [] };
  const description = String(args?.plan_description || base.description || '');
  const rawSteps = Array.isArray(args?.steps) ? (args.steps as unknown[]) : [];
  let steps = base.steps;
  rawSteps.forEach((rawStep, index) => {
    const record = asRecord(rawStep);
    if (!record) return;
    steps = upsertStep(steps, {
      id: String(record.id || `step-${index}`),
      description: String(record.description || ''),
      status: 'pending',
      assignedTo: record.assigned_to ? String(record.assigned_to) : undefined,
    });
  });
  // A fresh update_plan means the agent starts executing: mark the first
  // pending step in_progress so the board shows live progress.
  steps = promoteNextPending(steps);
  return { description, steps };
}

function applyCompleteStep(
  state: PlanBoardModel | null,
  evt: PlanToolEvent,
): PlanBoardModel | null {
  if (!state) {
    return state;
  }
  const args = asRecord(evt.args);
  const stepId = String(args?.step_id || '').trim();
  if (!stepId) {
    return state;
  }
  const index = state.steps.findIndex((step) => step.id === stepId);
  if (index < 0) {
    return state;
  }
  let steps = [...state.steps];
  if (evt.phase === 'call') {
    steps[index] = { ...steps[index], status: 'in_progress' };
    return { ...state, steps };
  }
  const succeeded = String(evt.status || 'succeeded').toLowerCase() === 'succeeded';
  steps[index] = {
    ...steps[index],
    status: succeeded ? 'done' : 'blocked',
    error: succeeded ? undefined : String(evt.status || 'failed'),
  };
  if (succeeded) {
    steps = promoteNextPending(steps);
  }
  return { ...state, steps };
}

function applyListSteps(state: PlanBoardModel | null, evt: PlanToolEvent): PlanBoardModel | null {
  if (evt.phase !== 'result') {
    return state;
  }
  const result = asRecord(evt.result);
  const rawSteps = result && Array.isArray(result.steps) ? (result.steps as unknown[]) : null;
  if (!rawSteps) {
    return state;
  }
  const steps: PlanStepModel[] = rawSteps
    .map((rawStep) => asRecord(rawStep))
    .filter((record): record is Record<string, unknown> => Boolean(record))
    .map((record, index) => ({
      id: String(record.id || `step-${index}`),
      description: String(record.description || ''),
      status: normalizeStepStatus(record.status),
      assignedTo: record.assigned_to ? String(record.assigned_to) : undefined,
      error: record.error ? String(record.error) : undefined,
    }));
  return { description: state?.description || '', steps };
}

/**
 * Fold one tool stream event into the plan board state.
 *
 * Returns `undefined` when the event is not a plan-tool event (caller keeps
 * previous state); otherwise returns the next board state.
 */
export function applyPlanToolEvent(
  state: PlanBoardModel | null,
  evt: PlanToolEvent,
): PlanBoardModel | null | undefined {
  if (!PLAN_TOOL_NAMES.has(evt.toolName)) {
    return undefined;
  }
  if (evt.toolName === 'update_plan') {
    return evt.phase === 'call' ? applyUpdatePlan(state, asRecord(evt.args)) : state;
  }
  if (evt.toolName === 'complete_plan_step') {
    return applyCompleteStep(state, evt);
  }
  return applyListSteps(state, evt);
}

export function planIncompleteCount(board: PlanBoardModel | null): number {
  if (!board) return 0;
  return board.steps.filter((step) => step.status !== 'done').length;
}
