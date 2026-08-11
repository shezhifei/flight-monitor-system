import { requestEnvelope } from '@/lib/http/apiClient';
import type { EvalJobDetail, EvalJobSummary } from '@/lib/types/apiModels';

const BASE = '/api/v2/ai/eval';

export interface EvalProfilePayload {
  profile_id: string;
  name: string;
  base_url: string;
  api_key: string;
  model: string;
  timeout: number;
  max_retries: number;
  retry_delay: number;
  api_mode?: string;
}

export async function createEvalJob(payload: {
  profiles: EvalProfilePayload[];
  options: Record<string, unknown>;
}): Promise<{ job_id: string }> {
  return requestEnvelope<{ job_id: string }>(`${BASE}/jobs`, {
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

export async function compareEvalProfiles(
  jobId: string,
  leftProfileId: string,
  rightProfileId: string,
): Promise<Record<string, unknown>> {
  const search = new URLSearchParams({
    left_profile_id: leftProfileId,
    right_profile_id: rightProfileId,
  });
  return requestEnvelope<Record<string, unknown>>(`${BASE}/jobs/${encodeURIComponent(jobId)}/compare?${search.toString()}`);
}
