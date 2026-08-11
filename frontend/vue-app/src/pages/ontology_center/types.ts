/** Ontology V1 frontend types (aligned with /api/v2/ontology). */

export type OntologyTabId =
  | 'views'
  | 'reassign'
  | 'resources'
  | 'suggestions'
  | 'links';

export interface StandOccupation {
  id: string;
  registration: string;
  stand_code: string | { 0?: string };
  starts_at?: string;
  ends_at?: string;
  kind?: string;
  moving_to_stand?: string | { 0?: string } | null;
  flight_id?: string | { 0?: string } | null;
  status?: string;
  created_by?: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface GateAssignment {
  id: string;
  registration: string;
  gate_code: string | { 0?: string };
  starts_at?: string;
  ends_at?: string;
  flight_id?: string | { 0?: string } | null;
  status?: string;
  created_by?: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface FlightResourceView {
  flight_id: string;
  registration: string | null;
  plan_stand: string | null;
  plan_gate: string | null;
  occupations: StandOccupation[];
  assignments: GateAssignment[];
  turnaround_links: TurnaroundLink[] | unknown[];
}

export interface AircraftResourceView {
  registration: string;
  in_field: boolean;
  current_stand: string | null;
  current_gate: string | null;
  occupations: StandOccupation[];
  assignments: GateAssignment[];
  flights: unknown[];
}

export interface ResourceAdjustmentSuggestion {
  id: string;
  flight_id: string | { 0?: string };
  kind: 'stand' | 'gate' | string;
  current_value: string | null;
  suggested_value: string;
  status: string;
  reason: string | null;
  payload?: unknown;
  created_by: string;
  decided_by?: string | null;
  decided_at?: string | null;
  expires_at?: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface TurnaroundLink {
  id: string;
  inbound_flight_id: string | { 0?: string };
  outbound_flight_id: string | { 0?: string };
  status: string;
  source: string;
  broken_reason?: string | null;
  created_by?: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface ReassignAppliedResult {
  flight_id: string;
  old_registration: string | null;
  new_registration: string;
  broken_links: string[];
  created_links: string[];
  suggestions: string[];
}

export interface AutoLinkScanResult {
  evaluated: number;
  created: string[];
  skipped: number;
  errors: string[];
}

export interface StandOccupationResult {
  occupation: Record<string, unknown>;
  overlap_warnings: string[];
}

export interface GateAssignmentResult {
  assignment: Record<string, unknown>;
  consistency_warnings: string[];
}

export function idField(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && '0' in (value as object)) {
    const inner = (value as { 0?: unknown })[0];
    if (typeof inner === 'string') return inner;
  }
  return String(value ?? '');
}

export function suggestionStatusTone(status: string): 'ok' | 'warn' | 'danger' | 'muted' {
  switch (status) {
    case 'pending':
      return 'warn';
    case 'accepted_executed':
      return 'ok';
    case 'rejected':
    case 'expired':
      return 'muted';
    default:
      return 'muted';
  }
}

export function linkStatusTone(status: string): 'ok' | 'warn' | 'danger' | 'muted' {
  return status === 'active' ? 'ok' : status === 'broken' ? 'danger' : 'muted';
}
