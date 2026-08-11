import { onBeforeUnmount, readonly, ref, watch } from 'vue';
import type { Ref } from 'vue';
import { normalizeFlightId, type Flight } from '../../../composables/useFlightData';

/**
 * 关键保障节点（里程碑）强提醒。
 * 移植自 legacy flight_monitor.js 的 EP-04 绿屏闪烁：
 * SSE 增量更新中 cleaning_end_time / boarding_allowed_time 从空变为有值时，
 * 触发全屏绿色径向脉冲 + 中央黑卡的强视觉通知。
 */

export const MILESTONE_FIELDS = {
  cleaning_end_time: '清洁结束',
  boarding_allowed_time: '允许登机',
} as const;

export type MilestoneField = keyof typeof MILESTONE_FIELDS;

export interface MilestonePulseEvent {
  flightId: string;
  flightNo: string;
  field: MilestoneField;
  /** 节点中文名，如「清洁结束」 */
  label: string;
}

const MILESTONE_FIELD_NAMES = Object.keys(MILESTONE_FIELDS) as MilestoneField[];

function isEmptyValue(value: unknown): boolean {
  if (value === null || value === undefined) return true;
  if (typeof value === 'string') return value.trim() === '';
  return false;
}

function resolveFlightNo(flight: Flight, fallbackId: string): string {
  const candidates = [
    flight.flight_number,
    flight.outbound_leg?.flight_no,
    flight.inbound_leg?.flight_no,
    fallbackId,
  ];
  for (const candidate of candidates) {
    const normalized = String(candidate ?? '').trim();
    if (normalized) return normalized.toUpperCase();
  }
  return '未知航班';
}

export interface MilestoneDetector {
  /** 记录基线（初始全量加载），不产生任何事件 */
  prime: (flights: readonly Flight[]) => void;
  /** 增量检测：返回本次从空→有值的里程碑跳变事件 */
  detect: (flights: readonly Flight[]) => MilestonePulseEvent[];
}

/**
 * 纯判定逻辑（无 Vue 依赖，便于单测）：
 * - prime 建立基线，初始全量加载不触发（legacy 行为）；
 * - detect 只上报「上次为空、本次有值」的跳变；
 * - 同一航班同一节点只触发一次（fired 集合去重）；
 * - 首次见到的航班只记录基线，不视为跳变。
 */
export function createMilestoneDetector(): MilestoneDetector {
  const seen = new Map<string, Record<MilestoneField, unknown>>();
  const fired = new Set<string>();

  function snapshot(flight: Flight): Record<MilestoneField, unknown> {
    return {
      cleaning_end_time: flight.cleaning_end_time,
      boarding_allowed_time: flight.boarding_allowed_time,
    };
  }

  function prime(flights: readonly Flight[]): void {
    for (const flight of flights) {
      const flightId = normalizeFlightId(flight.flight_id);
      if (!flightId) continue;
      seen.set(flightId, snapshot(flight));
    }
  }

  function detect(flights: readonly Flight[]): MilestonePulseEvent[] {
    const events: MilestonePulseEvent[] = [];
    for (const flight of flights) {
      const flightId = normalizeFlightId(flight.flight_id);
      if (!flightId) continue;
      const current = snapshot(flight);
      const previous = seen.get(flightId);
      seen.set(flightId, current);
      if (!previous) continue; // 新出现的航班只建基线
      for (const field of MILESTONE_FIELD_NAMES) {
        if (!isEmptyValue(previous[field]) || isEmptyValue(current[field])) continue;
        const firedKey = `${flightId}:${field}`;
        if (fired.has(firedKey)) continue;
        fired.add(firedKey);
        events.push({
          flightId,
          flightNo: resolveFlightNo(flight, flightId),
          field,
          label: MILESTONE_FIELDS[field],
        });
      }
    }
    return events;
  }

  return { prime, detect };
}

export interface UseMilestonePulseReturn {
  activePulse: Readonly<Ref<MilestonePulseEvent | null>>;
  dismiss: () => void;
}

export const MILESTONE_PULSE_DURATION_MS = 4000;

/**
 * 监听航班列表变化，首次变化建立基线（初始全量加载不触发），
 * 之后的增量变化中检测里程碑跳变并驱动 MilestonePulse 展示。
 */
export function useMilestonePulse(
  flightsSource: Readonly<Ref<readonly Flight[]>>,
): UseMilestonePulseReturn {
  const detector = createMilestoneDetector();
  const activePulse = ref<MilestonePulseEvent | null>(null);
  let primed = false;
  let hideTimer: ReturnType<typeof setTimeout> | null = null;

  function clearHideTimer(): void {
    if (hideTimer !== null) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
  }

  function dismiss(): void {
    clearHideTimer();
    activePulse.value = null;
  }

  watch(
    () => flightsSource.value,
    (flights) => {
      if (!primed) {
        // 首次变化 = 初始全量快照，仅建基线（legacy 行为：初始加载不触发）
        detector.prime(flights);
        primed = true;
        return;
      }
      const events = detector.detect(flights);
      if (events.length === 0) return;
      // 与 legacy 一致：同屏只展示最新一条
      activePulse.value = events[events.length - 1];
      clearHideTimer();
      hideTimer = setTimeout(() => {
        hideTimer = null;
        activePulse.value = null;
      }, MILESTONE_PULSE_DURATION_MS);
    },
  );

  onBeforeUnmount(dismiss);

  return {
    activePulse: readonly(activePulse),
    dismiss,
  };
}
