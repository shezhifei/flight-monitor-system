export interface EmployeeAnalyticsBucket {
  label: string;
  orderCount: number;
  completedOrderCount: number;
  occupiedMinutes: number;
  teamLabels: Set<string>;
  resourceId: string;
  orderIds: Set<string>;
  representativeOrderId: string;
}

export interface EmployeeAnalyticsItem {
  id: string;
  label: string;
  value: string;
  orderCount: number;
  completedOrderCount: number;
  occupiedMinutes: number;
  teamLabels: string[];
  resourceId: string;
  orderIds: string[];
  representativeOrderId: string;
}

export interface AiSuggestion {
  id: string;
  title: string;
  description: string;
  confidence?: number;
  orderId?: string;
  orderIds?: string[];
  suggestionType?: string;
}

export function toTimestamp(value: unknown): number {
  if (!value) return 0;
  const ts = Date.parse(String(value));
  return Number.isFinite(ts) ? ts : 0;
}

export function normalizeOrderIds(orderIds: readonly unknown[]): string[] {
  return Array.from(new Set(orderIds.map((id: unknown) => String(id || '').trim()).filter(Boolean)));
}

export function splitCommaSeparatedIds(input: string): string[] {
  return Array.from(new Set(String(input || '').split(',').map((part) => part.trim()).filter(Boolean)));
}

export function parseScenarioDelayInput(input: string): { items: Array<{ dispatch_order_id: string; delay_minutes: number }>; error: string | null } {
  const trimmed = String(input || '').trim();
  if (!trimmed) return { items: [], error: null };
  const parts = trimmed.split(/[\n,，；;]+/).map((part) => part.trim()).filter(Boolean);
  const items: Array<{ dispatch_order_id: string; delay_minutes: number }> = [];
  for (const part of parts) {
    const [orderId, delayText] = part.split(':').map((token) => token.trim());
    const delayMinutes = Number(delayText);
    if (!orderId || !delayText || !Number.isFinite(delayMinutes) || delayMinutes <= 0) {
      return { items: [], error: `延迟工单格式无效: ${part}。请使用 "order-id:分钟"` };
    }
    items.push({ dispatch_order_id: orderId, delay_minutes: Math.max(1, Math.round(delayMinutes)) });
  }
  return { items, error: null };
}
