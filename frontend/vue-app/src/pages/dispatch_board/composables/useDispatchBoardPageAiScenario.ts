import { ref } from 'vue';
import type { Ref } from 'vue';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import { unwrapApiData } from '@/shared/apiEnvelope';
import {
  type AiSuggestion,
  normalizeOrderIds,
  splitCommaSeparatedIds,
  parseScenarioDelayInput,
} from './useDispatchBoardPageAiTypes';

export interface UseDispatchBoardPageAiScenarioOptions {
  windowStartMs: Readonly<Ref<number>>;
  windowEndMs: Readonly<Ref<number>>;
  impactedOrderIds: Ref<string[]>;
}

export interface UseDispatchBoardPageAiScenarioReturn {
  scenarioEquipment: Ref<string>;
  scenarioStand: Ref<string>;
  scenarioDelay: Ref<string>;
  scenarioFrozen: Ref<string>;
  scenarioImpactedOrders: Ref<Array<{ id: string; title: string; description: string; orderId?: string }>>;
  scenarioProjectedConflicts: Ref<Array<{ id: string; title: string; description: string; orderId?: string }>>;
  scenarioRecommendations: Ref<Array<{ id: string; title: string; description: string; orderId?: string }>>;
  scenarioMetricsData: Ref<{ impactedCount: string; conflictCount: string; delayedCount: string; riskLevel: string; manualConfirmation: string; changedCount: string }>;
  previewScenario: () => Promise<void>;
  clearScenario: () => void;
}

export function useDispatchBoardPageAiScenario(options: UseDispatchBoardPageAiScenarioOptions): UseDispatchBoardPageAiScenarioReturn {
  const { windowStartMs, windowEndMs, impactedOrderIds } = options;
  const api = useApi();
  const toast = useToast();

  const scenarioEquipment = ref('');
  const scenarioStand = ref('');
  const scenarioDelay = ref('');
  const scenarioFrozen = ref('');
  const scenarioImpactedOrders = ref<Array<{ id: string; title: string; description: string; orderId?: string }>>([]);
  const scenarioProjectedConflicts = ref<Array<{ id: string; title: string; description: string; orderId?: string }>>([]);
  const scenarioRecommendations = ref<Array<{ id: string; title: string; description: string; orderId?: string }>>([]);
  const scenarioMetricsData = ref({
    impactedCount: '-',
    conflictCount: '-',
    delayedCount: '-',
    riskLevel: '-',
    manualConfirmation: '-',
    changedCount: '-',
  });

  async function previewScenario() {
    try {
      const delayedOrdersResult = parseScenarioDelayInput(scenarioDelay.value);
      if (delayedOrdersResult.error) {
        toast.show('error', delayedOrdersResult.error);
        return;
      }
      const res = await api.post<unknown>('/api/v2/dispatch/scenarios/preview', {
        window_start: new Date(windowStartMs.value).toISOString(),
        window_end: new Date(windowEndMs.value).toISOString(),
        equipment_unavailable_ids: splitCommaSeparatedIds(scenarioEquipment.value),
        closed_stand_ids: splitCommaSeparatedIds(scenarioStand.value),
        delayed_orders: delayedOrdersResult.items,
        frozen_order_ids: splitCommaSeparatedIds(scenarioFrozen.value),
      });
      const payload = unwrapApiData<Record<string, unknown>>(res.data);
      if (!res.ok) {
        toast.show('error', `场景预览失败: HTTP ${res.status}`);
        return;
      }
      if (res.ok && payload) {
        const recommendations = Array.isArray(payload.recommendations) ? payload.recommendations : [];
        const impactedOrders = Array.isArray(payload.impacted_orders) ? payload.impacted_orders : [];
        const projectedConflicts = Array.isArray(payload.projected_conflicts) ? payload.projected_conflicts : [];
        scenarioImpactedOrders.value = impactedOrders.map((r: Record<string, unknown>, i: number) => ({
          id: `impacted-${i}`,
          title: String(r.impact_type || r.dispatch_order_id || `影响 ${i + 1}`),
          description: String(r.reason || r.message || ''),
          orderId: String(r.dispatch_order_id || '').trim() || undefined,
        }));
        scenarioProjectedConflicts.value = projectedConflicts.map((c: Record<string, unknown>, i: number) => ({
          id: `conflict-${i}`,
          title: String(c.conflict_type || `冲突 ${i + 1}`),
          description: String(c.message || c.description || ''),
          orderId: Array.isArray(c.related_dispatch_order_ids) && c.related_dispatch_order_ids.length > 0 ? String(c.related_dispatch_order_ids[0]) : undefined,
        }));
        scenarioRecommendations.value = recommendations.map((r: Record<string, unknown>, i: number) => {
          const suggestion: AiSuggestion = {
            id: `rec-${i}`,
            title: String(r.action || r.type || `建议 ${i + 1}`),
            description: String(r.reason || r.description || ''),
            orderId: String(r.dispatch_order_id || r.target_order_id || '').trim() || undefined,
          };
          return {
            id: suggestion.id,
            title: suggestion.title,
            description: suggestion.description,
            orderId: suggestion.orderId,
          };
        });
        const impactedOrderIdsList = normalizeOrderIds([
          ...(Array.isArray(payload.changed_orders) ? payload.changed_orders : []),
          ...impactedOrders.map((item: Record<string, unknown>) => item.dispatch_order_id),
        ]);
        impactedOrderIds.value = impactedOrderIdsList;
        const impactSummary = payload.impact_summary && typeof payload.impact_summary === 'object' ? (payload.impact_summary as Record<string, unknown>) : {};
        scenarioMetricsData.value = {
          impactedCount: String(impactSummary.impacted_orders ?? '-'),
          conflictCount: String(impactSummary.projected_conflicts ?? '-'),
          delayedCount: String(impactSummary.delayed_orders ?? '-'),
          riskLevel: String(payload.risk_level ?? '-'),
          manualConfirmation: String(payload.requires_manual_confirmation ?? '-'),
          changedCount: String(Array.isArray(payload.changed_orders) ? payload.changed_orders.length : '-'),
        };
      }
    } catch (e) {
      console.warn('Failed to preview scenario:', e);
      toast.show('error', `场景预览失败: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  function clearScenario() {
    scenarioEquipment.value = '';
    scenarioStand.value = '';
    scenarioDelay.value = '';
    scenarioFrozen.value = '';
    scenarioImpactedOrders.value = [];
    scenarioProjectedConflicts.value = [];
    scenarioRecommendations.value = [];
    impactedOrderIds.value = [];
    scenarioMetricsData.value = {
      impactedCount: '-',
      conflictCount: '-',
      delayedCount: '-',
      riskLevel: '-',
      manualConfirmation: '-',
      changedCount: '-',
    };
  }

  return {
    scenarioEquipment,
    scenarioStand,
    scenarioDelay,
    scenarioFrozen,
    scenarioImpactedOrders,
    scenarioProjectedConflicts,
    scenarioRecommendations,
    scenarioMetricsData,
    previewScenario,
    clearScenario,
  };
}
