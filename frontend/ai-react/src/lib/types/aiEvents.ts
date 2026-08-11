export type RuntimeStatus =
  | 'in_progress'
  | 'success'
  | 'error'
  | 'pending_approval'
  | 'timeout'
  | 'permission_denied'
  | 'validation_error'
  | 'not_found';

export interface AiRuntimeEvent {
  event?: string;
  type?: string;
  payload?: Record<string, unknown>;
  execution_id?: string;
  request_id?: string;
  scene?: string;
  status?: RuntimeStatus | string;
  message?: string;
  data?: unknown;
}

export interface ToolLifecycleEvent {
  tool_call_id?: string;
  tool_name?: string;
  status?: RuntimeStatus | string;
  message?: string;
  pending_action?: PendingActionEvent;
  tool_arguments?: unknown;
  tool_result?: unknown;
  error?: unknown;
}

export interface ApprovalEvent {
  action_id?: string;
  pending_action?: PendingActionEvent;
  execution_result?: Record<string, unknown>;
}

export interface ExecutionTerminalEvent {
  execution_id?: string;
  status?: RuntimeStatus | string;
  message?: string;
}

export interface PendingActionEvent {
  action_id: string;
  tool_name?: string;
  status?: string;
  message?: string;
  created_at?: string;
  expires_at?: string;
  [key: string]: unknown;
}

