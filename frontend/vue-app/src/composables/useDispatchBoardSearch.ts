import type { DispatchOrder, DispatchOrderStatus } from './useDispatchBoardOrders';
import type { SafetyGateFilter } from './useDispatchBoardGantt';
import type { SafetyProgressMap } from './useDispatchBoardChecklist';
import { normalizeTaskCrewMembers } from './useDispatchBoardResources';
import { STATUS_LABELS } from './useDispatchBoardOrders';

export interface SearchMatch {
  item: DispatchOrder;
  index: number;
  label: string;
  sub: string;
}

// ---------------------------------------------------------------------------

export const SEARCH_RESULT_RENDER_LIMIT = 10;

export function normalizeSearchQuery(raw: string): string {
  return String(raw ?? '').trim().toLowerCase();
}

/** Search timeline items for a query. Returns matches up to limit. */

export function searchTimelineItems(
  items: ReadonlyArray<DispatchOrder>,
  query: string,
  limit = SEARCH_RESULT_RENDER_LIMIT,
): ReadonlyArray<SearchMatch> {
  const normalizedQuery = normalizeSearchQuery(query);
  if (!normalizedQuery) return [];

  const matches: SearchMatch[] = [];
  for (let i = 0; i < items.length && matches.length < limit; i++) {
    const item = items[i];
    if (!item || item.is_flight_summary) continue;

    const crewMembers = normalizeTaskCrewMembers(item.members);
    const searchable = [
      String(item.order_id ?? ''),
      String(item.flight_id ?? ''),
      String(item.task_type ?? ''),
      String(item.team_name ?? ''),
      String(item.lane_label ?? ''),
      ...crewMembers.map((m) => m.username),
    ].join(' ').toLowerCase();

    if (searchable.includes(normalizedQuery)) {
      const flightLabel = String(item.flight_id || '-').trim();
      const taskLabel = String(item.task_type || item.order_id || '').trim();
      matches.push({
        item,
        index: i,
        label: `${flightLabel} / ${taskLabel}`,
        sub: `${STATUS_LABELS[item.status as DispatchOrderStatus] ?? item.status ?? ''} | ${String(item.team_name ?? item.lane_label ?? '').trim()}`,
      });
    }
  }
  return matches as ReadonlyArray<SearchMatch>;
}

/** Compute safety gate status for a single order. */

export function getOrderSafetyGateStatus(
  orderId: string,
  safetyProgress: SafetyProgressMap,
): 'ready' | 'pending' | 'blocked' | 'unknown' {
  const entry = safetyProgress[orderId];
  if (!entry) return 'unknown';
  if (entry.failed_required_count > 0) return 'blocked';
  if (entry.pending_required_count > 0) return 'pending';
  return 'ready';
}

/** Filter timeline items by safety gate status. */

export function filterTimelineBySafetyGate(
  items: ReadonlyArray<DispatchOrder>,
  safetyProgress: SafetyProgressMap,
  filter: SafetyGateFilter,
): ReadonlyArray<DispatchOrder> {
  if (filter === 'all') return items;
  return items.filter((item) => {
    if (!item || item.is_flight_summary) return true;
    const orderId = String(item.order_id ?? '').trim();
    if (!orderId) return true;
    const status = getOrderSafetyGateStatus(orderId, safetyProgress);
    if (filter === 'blocked') return status === 'blocked';
    if (filter === 'pending') return status === 'pending';
    if (filter === 'ready') return status === 'ready';
    return true;
  });
}

/** Count orders by status. */

export function countOrdersByStatus(
  items: ReadonlyArray<DispatchOrder>,
  safetyProgress: SafetyProgressMap = {},
  safetyGateFilter: SafetyGateFilter = 'all',
): Record<DispatchOrderStatus, number> {
  const filtered = filterTimelineBySafetyGate(items, safetyProgress, safetyGateFilter);
  const counts: Record<DispatchOrderStatus, number> = {
    pending: 0,
    assigned: 0,
    in_progress: 0,
    completed: 0,
    cancelled: 0,
  };
  for (const item of filtered) {
    const status = item.status as DispatchOrderStatus;
    if (status in counts) {
      counts[status] += 1;
    }
  }
  return counts;
}

/** Parse comma-separated ID string into array. */

export function parseCommaSeparatedIds(raw: string): string[] {
  return raw
    .split(/[,，]/)
    .map((s) => s.trim())
    .filter(Boolean);
}

