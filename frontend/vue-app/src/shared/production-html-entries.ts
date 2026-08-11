/**
 * Production HTML entries served as `/frontend/<page>.html`.
 *
 * Hand-written allowlist consumed by `vite.config.ts` (rollup input) and by
 * `src/legacy/frontendEntryInventory.test.ts`. Adding or removing a page
 * requires an explicit edit here AND in the parity matrix; the inventory
 * test fails fast on drift.
 *
 * Special entries:
 * - `index` is a redirect-only stub (meta refresh + JS replace) to
 *   `/frontend/dashboard.html`. It does NOT load a Vue bundle.
 * - `test.html` is intentionally absent. It was a scratch file and has been
 *   deleted from the project; the inventory test fails if it returns.
 */
export const PRODUCTION_HTML_ENTRIES = [
  'index',
  'login',
  'dashboard',
  'workspace',
  'flight_monitor',
  'flight_imports',
  'dispatch_board',
  'dispatch_rule_center',
  'kpi_dashboard',
  'operations_review_report',
  'resource_manager',
  'resource_utilization',
  'system_status',
  'system_flags',
  'anomaly_monitor',
  'command_center',
  'flowable_modeler',
  'ai_config_center',
  'ai_monitor',
  'llm_eval_lab',
  'nl_query',
  'user_manager',
  'label_manager',
  'ontology_center',
] as const;

export type ProductionHtmlEntry = (typeof PRODUCTION_HTML_ENTRIES)[number];
