import { computed } from 'vue';
import type { ComputedRef, Ref } from 'vue';
import { formatDetailDateTime } from '@/composables/useDispatchBoardDetail';
import type { DispatchOrder, TimelineMember, DispatchQualificationGap } from '@/composables/useDispatchBoardData';

export interface UseDispatchBoardPageDetailOptions {
  detailOrder: Ref<DispatchOrder | null>;
  detailFlightOrders: Ref<DispatchOrder[]>;
  detailCurrentOrderId: ComputedRef<string>;
  detailSafetyGateState: ComputedRef<string>;
}

export interface UseDispatchBoardPageDetailReturn {
  detailCrewMembers: ComputedRef<string[]>;
  detailQualificationGaps: ComputedRef<string[]>;
  detailEquipmentCodes: ComputedRef<string[]>;
  detailTaskInfoRows: ComputedRef<Array<{ label: string; value: string; className?: string }>>;
  detailTimeInfoRows: ComputedRef<Array<{ label: string; value: string }>>;
  detailResourceInfoRows: ComputedRef<Array<{ label: string; value: string }>>;
  detailFlightStatusSummary: ComputedRef<Array<{ label: string; value: number }>>;
}

export function useDispatchBoardPageDetail(options: UseDispatchBoardPageDetailOptions): UseDispatchBoardPageDetailReturn {
  const { detailOrder, detailFlightOrders, detailCurrentOrderId, detailSafetyGateState } = options;

  const detailCrewMembers = computed(() => {
    const order = detailOrder.value;
    if (!order) return [];
    const members =
      Array.isArray(order.task_crew?.members) && order.task_crew.members.length
        ? order.task_crew.members
        : Array.isArray(order.members)
          ? order.members
          : [];
    return members
      .map((m: TimelineMember) => {
        const u = String(m?.username || m?.user_display_name || m?.user_id || '').trim();
        const s = String(m?.slot_code || '').trim();
        const l = String(m?.qualification_level_code || '').trim();
        return [u || '-', [s, l].filter(Boolean).join(' / ')].filter(Boolean).join(' ');
      })
      .filter(Boolean);
  });

  const detailQualificationGaps = computed(() =>
    (Array.isArray(detailOrder.value?.qualification_gap) ? detailOrder.value.qualification_gap : [])
      .map((g: DispatchQualificationGap) =>
        [String(g?.slot_code || '').trim(), String(g?.qualification_code || '').trim(), String(g?.min_level_code || '').trim()].filter(Boolean).join(' / '),
      )
      .filter(Boolean),
  );

  const detailEquipmentCodes = computed(() =>
    (Array.isArray(detailOrder.value?.equipment_codes) ? detailOrder.value.equipment_codes : [])
      .map((c: string | null | undefined) => String(c ?? '').trim())
      .filter(Boolean),
  );

  const detailTaskInfoRows = computed(() => {
    const o = detailOrder.value;
    if (!o) return [];
    return [
      { label: '工单 ID', value: detailCurrentOrderId.value || '-' },
      { label: '航班', value: String(o.flight_id || '') },
      { label: '作业类型', value: String(o.task_type_name || o.task_type || '') },
      { label: '机位', value: String(o.stand_code || o.stand_id || '') },
      { label: '登机口', value: String(o.gate || '') },
      { label: '状态', value: String(o.status || '') },
      { label: '来源', value: String(o.origin_label || o.source || '') },
      { label: '派工方式', value: String(o.dispatch_type || '').trim().toLowerCase() === 'auto' ? '自动' : '手动' },
      { label: '门禁状态', value: detailSafetyGateState.value || 'unknown' },
    ];
  });

  const detailTimeInfoRows = computed(() => {
    const o = detailOrder.value;
    if (!o) return [];
    return [
      { label: '计划开始', value: formatDetailDateTime(o.planned_start_time || o.start_time) },
      { label: '计划结束', value: formatDetailDateTime(o.planned_end_time || o.end_time) },
      { label: '实际开始', value: formatDetailDateTime(o.actual_start_time) },
      { label: '实际结束', value: formatDetailDateTime(o.actual_end_time) },
      { label: '预计完成', value: formatDetailDateTime(o.estimated_completion_time) },
      { label: '有效结束', value: formatDetailDateTime(o.effective_end_time || o.actual_end_time || o.planned_end_time || o.end_time) },
    ];
  });

  const detailResourceInfoRows = computed(() => {
    const o = detailOrder.value;
    if (!o) return [];
    return [
      { label: '归属班组', value: String(o.team_name || '') },
      { label: '负责人', value: String(o.individual_username || o.focus_user_name || '') },
      { label: '执行编组', value: detailCrewMembers.value.length > 0 ? detailCrewMembers.value.join(' / ') : '-' },
      { label: '资质缺口', value: detailQualificationGaps.value.length > 0 ? detailQualificationGaps.value.join(' ; ') : '-' },
      { label: '设备', value: detailEquipmentCodes.value.length > 0 ? detailEquipmentCodes.value.join(' / ') : '-' },
    ];
  });

  const detailFlightStatusSummary = computed(() => {
    const counts = new Map<string, number>();
    for (const order of detailFlightOrders.value) {
      const s = String(order.status || '');
      counts.set(s, (counts.get(s) || 0) + 1);
    }
    return Array.from(counts.entries()).map(([label, value]) => ({ label, value }));
  });

  return {
    detailCrewMembers,
    detailQualificationGaps,
    detailEquipmentCodes,
    detailTaskInfoRows,
    detailTimeInfoRows,
    detailResourceInfoRows,
    detailFlightStatusSummary,
  };
}
