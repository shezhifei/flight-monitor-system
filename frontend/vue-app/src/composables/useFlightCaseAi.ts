import type { Ref } from 'vue';
import { ref } from 'vue';
import { useApi } from './useApi';
import { useAuth } from './useAuth';
import { useToast } from './useToast';
import { fetchFlightHistoryReport } from './useFlightData';
import type { Flight } from './useFlightDataTypes';

export interface FlightCaseAiState {
  diagnosisResult: Ref<{ summary?: string; recommendations?: string[]; details?: string } | null>;
  diagnosisLoading: Ref<boolean>;
  journeyResult: Ref<{ summary?: string; details?: string } | null>;
  journeyLoading: Ref<boolean>;
  reportResult: Ref<{ summary?: string; details?: string } | null>;
  reportLoading: Ref<boolean>;
  runAiDiagnosis: () => Promise<void>;
  runAiEventJourney: () => Promise<void>;
  runHistoryReport: () => Promise<void>;
}

export function useFlightCaseAi(flight: Ref<Flight | null>): FlightCaseAiState {
  const api = useApi();
  const auth = useAuth();
  const toast = useToast();

  const diagnosisResult = ref<{ summary?: string; recommendations?: string[]; details?: string } | null>(null);
  const diagnosisLoading = ref(false);
  const journeyResult = ref<{ summary?: string; details?: string } | null>(null);
  const journeyLoading = ref(false);
  const reportResult = ref<{ summary?: string; details?: string } | null>(null);
  const reportLoading = ref(false);

  function requireFlightId(actionLabel: string): string | null {
    const flightId = String(flight.value?.flight_id || '').trim();
    if (!flightId) {
      toast.showToast('warning', `请先选择航班后再${actionLabel}`, { duration: 3500 });
      return null;
    }
    return flightId;
  }

  async function runAiDiagnosis() {
    const flightId = requireFlightId('AI 诊断');
    if (!flightId) return;

    diagnosisLoading.value = true;
    diagnosisResult.value = null;

    try {
      const { post } = api;
      const result = await post('/api/v2/ai/tools/execute', {
        tool_name: 'get_handling_recommendation',
        tool_args: {
          flight_id: String(flightId),
          context: 'flight_diagnosis',
        },
      });

      if (result.ok && result.data) {
        const payload = result.data as {
          data?: { result?: Record<string, unknown> | null } | null;
          result_data?: Record<string, unknown> | null;
          message?: string;
        };
        const data = (payload.result_data && typeof payload.result_data === 'object')
          ? payload.result_data
          : (payload.data?.result && typeof payload.data.result === 'object' ? payload.data.result : {});
        diagnosisResult.value = {
          summary: String(data.summary || data.output || payload.message || '诊断完成'),
          recommendations: Array.isArray(data.recommendations) ? data.recommendations as string[] : [],
          details: String(data.details || ''),
        };
      } else {
        diagnosisResult.value = {
          summary: '诊断失败',
          recommendations: [],
          details: `请求失败 (${result.status})`,
        };
        toast.showToast('error', `AI 诊断失败 (${result.status})`, { duration: 4000 });
      }
    } catch (err) {
      console.error('AI diagnosis failed:', err);
      diagnosisResult.value = {
        summary: '诊断失败',
        recommendations: [],
        details: err instanceof Error ? err.message : '未知错误',
      };
      toast.showToast('error', 'AI 诊断失败', { duration: 4000 });
    } finally {
      diagnosisLoading.value = false;
    }
  }

  async function runAiEventJourney() {
    const flightId = requireFlightId('生成事件经过');
    if (!flightId) return;

    journeyLoading.value = true;
    journeyResult.value = null;

    try {
      const { post } = api;
      const result = await post('/api/v2/ai/tools/execute', {
        tool_name: 'generate_event_journey',
        tool_args: {
          flight_id: String(flightId),
          context: 'event_journey',
        },
      });

      if (result.ok && result.data) {
        const payload = result.data as {
          data?: { result?: Record<string, unknown> | null } | null;
          result_data?: Record<string, unknown> | null;
          message?: string;
        };
        const data = (payload.result_data && typeof payload.result_data === 'object')
          ? payload.result_data
          : (payload.data?.result && typeof payload.data.result === 'object' ? payload.data.result : {});
        journeyResult.value = {
          summary: String(data.summary || data.output || payload.message || '事件经过已生成'),
          details: String(data.details || ''),
        };
      } else {
        journeyResult.value = {
          summary: '生成失败',
          details: `请求失败 (${result.status})`,
        };
        toast.showToast('error', `生成事件经过失败 (${result.status})`, { duration: 4000 });
      }
    } catch (err) {
      console.error('Event journey failed:', err);
      journeyResult.value = {
        summary: '生成失败',
        details: err instanceof Error ? err.message : '未知错误',
      };
      toast.showToast('error', '生成事件经过失败', { duration: 4000 });
    } finally {
      journeyLoading.value = false;
    }
  }

  async function runHistoryReport() {
    const flightId = requireFlightId('生成动态报表');
    if (!flightId) return;

    reportLoading.value = true;
    reportResult.value = null;

    try {
      const data = await fetchFlightHistoryReport(flightId, { apiBase: auth.apiBase.value, authFetch: auth.fetch });
      const resultData = (data?.data ?? data) as Record<string, unknown>;
      reportResult.value = {
        summary: String(resultData?.summary || '动态报表已生成'),
        details: String(resultData?.details || resultData?.report || ''),
      };
    } catch (err) {
      console.error('History report failed:', err);
      reportResult.value = {
        summary: '生成失败',
        details: err instanceof Error ? err.message : '未知错误',
      };
      toast.showToast('error', '生成动态报表失败', { duration: 4000 });
    } finally {
      reportLoading.value = false;
    }
  }

  return {
    diagnosisResult,
    diagnosisLoading,
    journeyResult,
    journeyLoading,
    reportResult,
    reportLoading,
    runAiDiagnosis,
    runAiEventJourney,
    runHistoryReport,
  };
}
