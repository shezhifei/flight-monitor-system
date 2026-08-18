import { requestEnvelope } from '@/lib/http/apiClient';
import type { EvalJobCreatePayload, EvalJobDetail, EvalJobSummary } from '@/lib/types/apiModels';

const BASE = '/api/v2/ai/eval';

export async function createEvalJob(payload: EvalJobCreatePayload): Promise<{ job_id: string; status: string }> {
  return requestEnvelope<{ job_id: string; status: string }>(`${BASE}/jobs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

export async function listEvalJobs(limit = 20): Promise<EvalJobSummary[]> {
  const data = await requestEnvelope<{ items: EvalJobSummary[] }>(`${BASE}/jobs?limit=${encodeURIComponent(String(limit))}`);
  return data.items || [];
}

export async function getEvalJob(jobId: string): Promise<EvalJobDetail> {
  return requestEnvelope<EvalJobDetail>(`${BASE}/jobs/${encodeURIComponent(jobId)}`);
}

export async function cancelEvalJob(jobId: string): Promise<Record<string, unknown>> {
  return requestEnvelope<Record<string, unknown>>(`${BASE}/jobs/${encodeURIComponent(jobId)}/cancel`, {
    method: 'POST',
  });
}
