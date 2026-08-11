export type PageDefinition = {
  id: string;
  title: string;
  summary: string;
  url?: string;
};

export const pageRegistry: Record<string, PageDefinition> = {
  dashboard: {
    id: 'dashboard',
    title: 'Dashboard',
    summary: 'Operations hub with live module cards, SSE updates, and quick navigation to dispatch and flight workspaces.',
    url: '/frontend/dashboard.html',
  },
  workspace: {
    id: 'workspace',
    title: 'Workspace',
    summary: 'Multi-tab operations workspace shell: function bar + in-page tabs embedding module pages via iframe.',
    url: '/frontend/workspace.html',
  },
  login: {
    id: 'login',
    title: 'Login',
    summary: 'Authentication form with validation, token storage, and redirect to flight monitor.',
    url: '/frontend/login.html',
  },
  command_center: {
    id: 'command_center',
    title: 'Command Center',
    summary: 'Operational command workspace for unified incident, dispatch, and notification orchestration.',
    url: '/frontend/command_center.html',
  },
  resource_manager: {
    id: 'resource_manager',
    title: 'Resource Manager',
    summary: 'Dispatch resource administration for teams, members, equipment, types, and operational status.',
    url: '/frontend/resource_manager.html',
  },
  dispatch_rule_center: {
    id: 'dispatch_rule_center',
    title: 'Dispatch Rule Center',
    summary: 'Left-right admin workbench: department-scoped dispatch rules (task types, generation/adjustment rules, requirements, preview, manual order, snapshot export) plus flight/leg label definition management.',
    url: '/frontend/dispatch_rule_center.html',
  },
  system_status: {
    id: 'system_status',
    title: 'System Status',
    summary: 'Platform health view with subsystem checks, dependencies, and recent operational alerts.',
    url: '/frontend/system_status.html',
  },
  anomaly_monitor: {
    id: 'anomaly_monitor',
    title: 'Anomaly Monitor',
    summary: 'Anomaly inbox with timeline, acknowledgement, and drill-down into related dispatch and flight context.',
    url: '/frontend/anomaly_monitor.html',
  },
  flight_imports: {
    id: 'flight_imports',
    title: 'Flight Imports',
    summary: 'Flight schedule import workspace with upload, validation, dry-run, and commit job tracking.',
    url: '/frontend/flight_imports.html',
  },
  kpi_dashboard: {
    id: 'kpi_dashboard',
    title: 'KPI Dashboard',
    summary: 'Operational and executive KPI charts with date-range filtering and drill-down to source records.',
    url: '/frontend/kpi_dashboard.html',
  },
  resource_utilization: {
    id: 'resource_utilization',
    title: 'Resource Utilization',
    summary: 'Utilization analytics for teams, equipment, and shift coverage across configurable time windows.',
    url: '/frontend/resource_utilization.html',
  },
  flowable_modeler: {
    id: 'flowable_modeler',
    title: 'Flowable Modeler',
    summary: 'Embedded BPMN modeler with integrated AI chat for editing, validating, and deploying workflow definitions.',
    url: '/frontend/flowable_modeler.html',
  },
  system_flags: {
    id: 'system_flags',
    title: 'System Flags',
    summary: 'Runtime feature flag administration with audit log of toggles, scopes, and rollout status.',
    url: '/frontend/system_flags.html',
  },
  user_manager: {
    id: 'user_manager',
    title: 'User Manager',
    summary: 'User administration covering accounts, roles, permission templates, and direct linking to the label manager.',
    url: '/frontend/user_manager.html',
  },
  ai_monitor: {
    id: 'ai_monitor',
    title: 'AI Monitor',
    summary: 'React-hosted AI runtime monitor for jobs, traces, guardrail events, and provider health.',
    url: '/frontend/ai_monitor.html',
  },
  ai_config_center: {
    id: 'ai_config_center',
    title: 'AI Config Center',
    summary: 'AI ontology, action, mapping, constraint, and model/tooling configuration owned by the Vue shell.',
    url: '/frontend/ai_config_center.html',
  },
  llm_eval_lab: {
    id: 'llm_eval_lab',
    title: 'LLM Eval Lab',
    summary: 'React-hosted evaluation lab for model comparison, prompt regression, and offline test runs.',
    url: '/frontend/llm_eval_lab.html',
  },
  nl_query: {
    id: 'nl_query',
    title: 'NL Query',
    summary: 'Natural-language query workspace for text-to-SQL exploration, prompt history, and result inspection.',
    url: '/frontend/nl_query.html',
  },
  operations_review_report: {
    id: 'operations_review_report',
    title: 'Operations Review Report',
    summary: 'Operations review composer with sectioned narrative, evidence linking, and export to dispatch handover.',
    url: '/frontend/operations_review_report.html',
  },
  flight_monitor: {
    id: 'flight_monitor',
    title: 'Flight Monitor',
    summary: 'Live flight board with SSE updates, filters, list/detail views, and notification modals.',
    url: '/frontend/flight_monitor.html',
  },
  dispatch_board: {
    id: 'dispatch_board',
    title: 'Dispatch Board',
    summary: 'Dispatch workspace with gantt chart, AI assistant, replan, and collaboration panels.',
    url: '/frontend/dispatch_board.html',
  },
  label_manager: {
    id: 'label_manager',
    title: 'Label Manager',
    summary: 'Compatibility entry: redirects to dispatch_rule_center?section=labels (labels merged into the rule workbench).',
    url: '/frontend/label_manager.html',
  },
};

export function getPageDefinition(pageId: string): PageDefinition {
  const page = pageRegistry[pageId];

  if (!page) {
    throw new Error(`Unknown page id: ${pageId}`);
  }

  return page;
}
