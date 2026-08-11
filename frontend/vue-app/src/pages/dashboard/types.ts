export interface ApiEnvelope<T> {
  success: boolean;
  data: T;
  message?: string;
  error?: string;
}

export type DashboardIconName =
  | 'activity'
  | 'bar_chart'
  | 'connection'
  | 'fast'
  | 'plane'
  | 'refresh'
  | 'settings'
  | 'storage'
  | 'users';

export interface WorkbenchTask {
  id: string;
  title: string;
  description: string;
  priority: 'high' | 'medium' | 'low';
  due?: string;
  href: string;
  status?: 'pending' | 'in_progress' | 'done';
}

export interface WorkbenchRisk {
  id: string;
  title: string;
  description: string;
  severity: 'critical' | 'warning' | 'info';
  count?: number;
  href?: string;
}

export interface WorkbenchChange {
  id: string;
  title: string;
  description: string;
  timestamp: string;
  actor?: string;
  type?: string;
}

export interface QuickLink {
  id: string;
  title: string;
  description: string;
  icon: DashboardIconName;
  href: string;
  theme?: string;
  wide?: boolean;
}

export interface WorkbenchData {
  user_name: string;
  user_role?: string;
  shift_label?: string;
  my_tasks: WorkbenchTask[];
  operation_risks: WorkbenchRisk[];
  recent_changes: WorkbenchChange[];
  quick_links: QuickLink[];
  modules: QuickLink[];
}

export interface DashboardWorkbenchResponse {
  generated_at: string;
  user_context: DashboardUserContext;
  role_hint: string;
  attention_items: DashboardAttentionItem[];
  risk_summary: DashboardRiskSummary;
  recent_changes: DashboardRecentChange[];
  quick_links: DashboardQuickLinkResponse[];
  module_status: DashboardModuleStatus[];
  degraded_sources: DashboardDegradedSource[];
}

export interface DashboardUserContext {
  user_id: string;
  username: string | null;
  department: string | null;
  is_admin: boolean;
  permissions: string[];
}

export interface DashboardAttentionItem {
  id: string;
  title: string;
  priority: string;
  status: string;
  source: string;
  source_id: string | null;
  owner_id: string | null;
  due_at: string | null;
  updated_at: string | null;
  recommended_action: string;
}

export interface DashboardRiskSummary {
  unresolved_anomalies: number;
  high_risk_flights: number;
  dispatch_conflicts: number;
  stale_data_indicators: DashboardStaleDataIndicator[];
  high_risk_flight_refs: DashboardRiskFlightRef[];
  dispatch_conflict_refs: unknown[];
}

export interface DashboardRiskFlightRef {
  flight_id: string;
  anomaly_id: string;
  severity: string;
  title: string;
  detected_at: string;
}

export interface DashboardStaleDataIndicator {
  source: string;
  state: string;
  detail: string;
  observed_at: string;
}

export interface DashboardRecentChange {
  id: string;
  title: string;
  source: string;
  changed_at: string;
  severity: string | null;
  entity_id: string | null;
}

export interface DashboardQuickLinkResponse {
  id: string;
  label: string;
  href: string;
  module: string;
  intent: string;
}

export interface DashboardModuleStatus {
  module: string;
  status: string;
  detail: string;
}

export interface DashboardDegradedSource {
  source: string;
  reason: string;
}
