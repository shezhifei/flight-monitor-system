export interface ApiSuccessResponse<T = unknown> {
  ok: true;
  status: number;
  data: T;
}

export interface ApiErrorResponse {
  ok: false;
  status: number;
  error: string;
}

export type ApiResponse<T = unknown> = ApiSuccessResponse<T> | ApiErrorResponse;

export interface SSEEventPayload {
  topic?: string;
  type?: string;
  data?: unknown;
  [key: string]: unknown;
}

export interface ChatMessageItem {
  id: string;
  content: string;
  sender?: string;
  timestamp?: string;
  [key: string]: unknown;
}

export interface DispatchSuggestionItem {
  id: string;
  label: string;
  [key: string]: unknown;
}

export interface FlightLegLabels {
  commercial?: string[];
  vip?: boolean;
  [key: string]: unknown;
}
