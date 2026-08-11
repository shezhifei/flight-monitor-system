/**
 * 页面路由常量 —— 所有前端页面内跳转的单一来源。
 *
 * 策略说明：
 * - Canonical 路径：`/frontend/<page>.html`
 *   这些是 Vite 构建后 `vue-app/dist/<page>.html` 通过 Python
 *   FastAPI static mount 对外暴露的路径（base = '/frontend/'）。
 * - 兼容路径：`/frontend/html/<page>.html`
 *   这些是遗留 HTML 入口，当前生产环境仍可用。
 *   本次不切断兼容路径，但所有新代码必须使用 canonical 路径。
 *
 * 后续迁移：
 * - 当 Python 静态文件挂载切换为优先服务 `vue-app/dist/`
 *   且遗留 `frontend/html/` 入口确认不再使用后，
 *   可将兼容路径从代码中移除。
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
