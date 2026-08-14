/**
 * 页面路由常量 —— 所有前端页面内跳转的单一来源。
 *
 * 路径统一为 `/frontend/<page>.html`，对应 Vite 构建产物 `vue-app/dist/<page>.html`，
 * 由边缘静态服务对外暴露。页面跳转必须经由 `pageUrl()`，禁止硬编码路径。
 */

export const PAGE_ROUTES = {
  login:            '/frontend/login.html',
  dashboard:        '/frontend/dashboard.html',
  workspace:        '/frontend/workspace.html',
  flight_monitor:   '/frontend/flight_monitor.html',
  flight_imports:   '/frontend/flight_imports.html',
  dispatch_board:   '/frontend/dispatch_board.html',
  dispatch_rule_center: '/frontend/dispatch_rule_center.html',
  kpi_dashboard:    '/frontend/kpi_dashboard.html',
  operations_review_report: '/frontend/operations_review_report.html',
  resource_manager: '/frontend/resource_manager.html',
  resource_utilization:     '/frontend/resource_utilization.html',
  system_status:    '/frontend/system_status.html',
  system_flags:     '/frontend/system_flags.html',
  anomaly_monitor:  '/frontend/anomaly_monitor.html',
  command_center:   '/frontend/command_center.html',
  flowable_modeler: '/frontend/flowable_modeler.html',
  ai_config_center: '/frontend/ai_config_center.html',
  ai_monitor:       '/frontend/ai_monitor.html',
  llm_eval_lab:     '/frontend/llm_eval_lab.html',
  nl_query:         '/frontend/nl_query.html',
  user_manager:     '/frontend/user_manager.html',
  label_manager:    '/frontend/label_manager.html',
  ontology_center:  '/frontend/ontology_center.html',
} as const;

export type PageKey = keyof typeof PAGE_ROUTES;

/** 获取页面 URL。使用 PAGE_ROUTES 常量作为单一来源。 */
export function pageUrl(key: PageKey): string {
  return PAGE_ROUTES[key];
}

/**
 * Ontology Center deep-link.
 * Query: `flight` (preferred) and/or `registration`, optional `tab`.
 * Example: `/frontend/ontology_center.html?flight=FL…&tab=resources`
 */
export function ontologyCenterUrl(opts: {
  flightId?: string | null;
  registration?: string | null;
  tab?: string | null;
} = {}): string {
  const base = PAGE_ROUTES.ontology_center;
  const q = new URLSearchParams();
  const flight = String(opts.flightId ?? '').trim();
  const registration = String(opts.registration ?? '').trim();
  const tab = String(opts.tab ?? '').trim();
  if (flight) q.set('flight', flight);
  if (registration) q.set('registration', registration);
  if (tab) q.set('tab', tab);
  const qs = q.toString();
  return qs ? `${base}?${qs}` : base;
}
