/**
 * Frontend surface inventory and strict-parity evidence targets.
 *
 * Ownership and parity are deliberately separate concepts: a source file may
 * exist and own a production URL while its behavior is still unverified.
 * `parityEvidence.test.ts` is the promotion gate for evidence-bearing rows.
 */
export type SurfaceKind =
  | 'vue-page'
  | 'vue-native'
  | 'react-page'
  | 'react-widget'
  | 'react-drawer'
  | 'redirect'
  | 'retired'
  | 'debug-removed';

export type SurfaceParityStatus =
  | 'unverified'
  | 'contract-captured'
  | 'in-progress'
  | 'blocked'
  | 'verified'
  | 'retired'
  | 'redirect'
  | 'debug-excluded';

export type EvidenceGatedSurfaceStatus = Exclude<
  SurfaceParityStatus,
  'retired' | 'redirect' | 'debug-excluded'
>;

export const REQUIRED_PARITY_VIEWPORTS = [
  'desktop-wide',
  'desktop',
  'laptop',
  'tablet',
  'mobile',
] as const;

export type RequiredParityViewport = (typeof REQUIRED_PARITY_VIEWPORTS)[number];

export interface SurfaceParityEvidence {
  /** Tracked legacy contract derived from the ignored archive. */
  contract: string;
  /** Playwright/Vitest spec that owns the referenced scenario IDs. */
  browserSpec: string;
  /** Tracked legacy screenshots required to promote this surface. */
  legacyScreenshots: string[];
  /** Stable end-to-end behavior scenario IDs asserted by the browser spec. */
  functionalScenarios: string[];
  /** Stable scenario IDs asserted by the browser spec. */
  apiScenarios: string[];
  /** Stable named-event/reconnect scenario IDs, including explicit N/A coverage. */
  realtimeScenarios: string[];
  /** Stable automated accessibility scenario IDs asserted by the browser spec. */
  accessibilityScenarios: string[];
  /** Viewport IDs covered by the screenshots and browser scenarios. */
  viewports: RequiredParityViewport[];
  /** Approved exception IDs. Empty is the normal state. */
  exceptions: string[];
}

interface SurfaceParityRowBase {
  id: string;
  kind: SurfaceKind;
  legacyHtml?: string;
  vueHtml?: string;
  vueEntry?: string;
  vueComponent?: string;
  reactEntry?: string;
  hostId?: string;
  canonicalUrl?: string;
  notes?: string;
}

export interface EvidenceGatedSurfaceParityRow extends SurfaceParityRowBase {
  kind: 'vue-page' | 'react-page' | 'react-widget' | 'react-drawer';
  status: EvidenceGatedSurfaceStatus;
  evidence: SurfaceParityEvidence;
}

export interface RetiredSurfaceParityRow extends SurfaceParityRowBase {
  kind: 'retired';
  status: 'retired';
  evidence?: never;
}

/**
 * Vue MPA pages that never had a legacy HTML archive counterpart.
 * Listed for inventory/parity matrix completeness; not evidence-gated for pixel promotion.
 */
export interface VueNativeSurfaceParityRow extends SurfaceParityRowBase {
  kind: 'vue-native';
  status: 'unverified';
  evidence?: never;
}

export interface RedirectSurfaceParityRow extends SurfaceParityRowBase {
  kind: 'redirect';
  status: 'redirect';
  evidence?: never;
}

export interface DebugExcludedSurfaceParityRow extends SurfaceParityRowBase {
  kind: 'debug-removed';
  status: 'debug-excluded';
  evidence?: never;
}

export type SurfaceParityRow =
  | EvidenceGatedSurfaceParityRow
  | VueNativeSurfaceParityRow
  | RetiredSurfaceParityRow
  | RedirectSurfaceParityRow
  | DebugExcludedSurfaceParityRow;

const LEGACY_HTML_ROOT = 'frontend/legacy/html';
const PARITY_ROOT = 'frontend/vue-app';

/** Screenshot basename prefixes (must match capture.metadata.json file names). */
const PRIMARY_LEGACY_SCENARIO_BY_PAGE: Record<string, string> = {
  ai_config_center: 'ai_config_center-success',
  ai_monitor: 'ai_monitor-success',
  anomaly_monitor: 'anomaly_monitor-success',
  command_center: 'command_center-success',
  dashboard: 'dashboard-success',
  dispatch_board: 'dispatch_board-success',
  dispatch_rule_center: 'dispatch_rule_center-success',
  flight_imports: 'flight_imports-success',
  flight_monitor: 'flight_monitor-success',
  flowable_modeler: 'flowable_modeler-success',
  kpi_dashboard: 'kpi_dashboard-success',
  label_manager: 'label-manager-success',
  llm_eval_lab: 'llm-eval-lab-success',
  login: 'default',
  nl_query: 'nl-query-success',
  operations_review_report: 'operations-review-report-success',
  resource_manager: 'resource-manager-success',
  resource_utilization: 'resource-utilization-success',
  system_flags: 'system-flags-success',
  system_status: 'system-status-success',
  user_manager: 'user-manager-success',
};

/**
 * Contract scenario IDs wired into e2e titles by scripts/parity/align-evidence-ids.mjs.
 * Keep this map in sync by re-running that script after contract/e2e changes.
 */
import pageEvidenceScenarios from './pageEvidenceScenarios.json';

const PAGE_EXCEPTIONS: Record<string, string[]> = {
  dispatch_board: ['legacy-dispatch-board-gantt-syntax-error'],
  label_manager: ['legacy-label-manager-build-assets-missing'],
};

type PageEvidenceBuckets = {
  functional: string[];
  api: string[];
  realtime: string[];
  accessibility: string[];
};

function evidenceBucketsFor(ownerPageId: string): PageEvidenceBuckets {
  const buckets = (pageEvidenceScenarios as Record<string, PageEvidenceBuckets>)[ownerPageId];
  if (!buckets) {
    throw new Error(`Missing pageEvidenceScenarios entry for ${ownerPageId}`);
  }
  return buckets;
}

function createPlannedEvidence(
  surfaceId: string,
  ownerPageId = surfaceId,
): SurfaceParityEvidence {
  const primaryScenario = PRIMARY_LEGACY_SCENARIO_BY_PAGE[ownerPageId];
  if (!primaryScenario) throw new Error(`Missing primary legacy screenshot scenario for ${ownerPageId}`);
  const screenshotRegion = surfaceId === ownerPageId ? 'full-page' : surfaceId;
  const buckets = evidenceBucketsFor(ownerPageId);

  return {
    contract: `${PARITY_ROOT}/parity/contracts/${ownerPageId}.json`,
    browserSpec: `${PARITY_ROOT}/e2e/parity/pages/${ownerPageId}.parity.spec.ts`,
    legacyScreenshots: REQUIRED_PARITY_VIEWPORTS.map(
      (viewport) =>
        `${PARITY_ROOT}/e2e/parity/snapshots/legacy/${ownerPageId}/${primaryScenario}--${screenshotRegion}--${viewport}.png`,
    ),
    // Real contract scenario IDs. Browser specs must include these IDs in enabled test titles.
    functionalScenarios: buckets.functional,
    apiScenarios: buckets.api,
    realtimeScenarios: buckets.realtime,
    accessibilityScenarios: buckets.accessibility,
    viewports: [...REQUIRED_PARITY_VIEWPORTS],
    exceptions: [...(PAGE_EXCEPTIONS[ownerPageId] ?? [])],
  };
}

const VUE_PAGE_COMPONENTS = [
  ['login', 'pages/login/Login.vue'],
  ['dashboard', 'pages/dashboard/Dashboard.vue'],
  ['flight_monitor', 'pages/flight_monitor/FlightMonitorPage.vue'],
  ['flight_imports', 'pages/flight_imports/FlightImports.vue'],
  ['dispatch_board', 'pages/dispatch_board/DispatchBoard.vue'],
  ['dispatch_rule_center', 'pages/dispatch_rule_center/DispatchRuleCenter.vue'],
  ['kpi_dashboard', 'pages/kpi_dashboard/KpiDashboard.vue'],
  ['operations_review_report', 'pages/operations_review_report/OperationsReviewReport.vue'],
  ['resource_manager', 'pages/resource_manager/ResourceManager.vue'],
  ['resource_utilization', 'pages/resource_utilization/ResourceUtilization.vue'],
  ['system_status', 'pages/system_status/SystemStatus.vue'],
  ['system_flags', 'pages/system_flags/SystemFlags.vue'],
  ['anomaly_monitor', 'pages/anomaly_monitor/AnomalyMonitor.vue'],
  ['command_center', 'pages/command_center/CommandCenter.vue'],
  ['flowable_modeler', 'pages/flowable_modeler/FlowableModeler.vue'],
  ['ai_config_center', 'pages/ai_config_center/AiConfigCenter.vue'],
  ['ai_monitor', 'pages/ai_monitor/AiMonitor.vue'],
  ['llm_eval_lab', 'pages/llm_eval_lab/LlmEvalLab.vue'],
  ['nl_query', 'pages/nl_query/NlQuery.vue'],
  ['user_manager', 'pages/user_manager/UserManager.vue'],
  ['label_manager', 'pages/label_manager/LabelManagerPage.vue'],
] as const;

const IN_PROGRESS_PAGES = new Set([
  // Desktop full-page and critical login-card captures are pixel-identical after
  // aligning the document language; remaining viewport/live gates still apply.
  'login',
  // Desktop full-page is 0.371% and the critical workspace is pixel-identical;
  // remaining viewport/live gates still apply.
  'system_flags',
  // Desktop full-page is 0.306% and the critical realtime-log region is
  // pixel-identical; remaining viewport/live gates still apply.
  'system_status',
  // Deterministic Vue capture is stable, but desktop full-page remains 73.74%
  // different on the shared canvas and the legacy-only handover drawer is skipped.
  'dashboard',
]);

const VUE_PAGES: EvidenceGatedSurfaceParityRow[] = VUE_PAGE_COMPONENTS.map(
  ([id, vueComponent]) => ({
    id,
    kind: 'vue-page',
    // Contracts and legacy baselines exist; functional promotion remains evidence-gated.
    status: IN_PROGRESS_PAGES.has(id) ? 'in-progress' : 'contract-captured',
    legacyHtml: `${LEGACY_HTML_ROOT}/${id}.html`,
    vueHtml: `${PARITY_ROOT}/${id}.html`,
    vueEntry: `${PARITY_ROOT}/src/entries/${id}.ts`,
    vueComponent: `${PARITY_ROOT}/src/${vueComponent}`,
    canonicalUrl: `/frontend/${id}.html`,
    evidence: createPlannedEvidence(id),
    notes: id === 'login'
      ? 'Functional/API e2e green; desktop full-page and critical login-card captures are pixel-identical. Remaining viewport and canonical-runtime live gates prevent promotion.'
      : id === 'system_flags'
        ? 'Functional e2e green; desktop full-page differs by 0.371% and the critical flags workspace is pixel-identical. Remaining viewport and canonical-runtime live gates prevent promotion.'
        : id === 'system_status'
          ? 'Functional/SSE e2e green; desktop full-page differs by 0.306% and the critical realtime-log region is pixel-identical. Remaining viewport and canonical-runtime live gates prevent promotion.'
          : id === 'dashboard'
            ? 'Vue success/empty/partial captures are deterministic; desktop full-page structural progress is 73.74% differing (74% ceiling). Legacy-only handover interaction is soft-skipped, so verification is not eligible.'
            : undefined,
  }),
);

const REACT_AI_SURFACES: EvidenceGatedSurfaceParityRow[] = [
  {
    id: 'ai_monitor_react',
    kind: 'react-page',
    status: 'contract-captured',
    legacyHtml: `${LEGACY_HTML_ROOT}/ai_monitor.html`,
    reactEntry: 'frontend/ai-react/src/entries/ai_monitor.tsx',
    hostId: 'ai-react-root',
    canonicalUrl: '/frontend/ai_monitor.html',
    evidence: createPlannedEvidence('ai_monitor_react', 'ai_monitor'),
    notes: 'Hosted by Vue AiMonitor.vue via AiReactEntryShell.',
  },
  {
    id: 'nl_query_react',
    kind: 'react-page',
    status: 'contract-captured',
    legacyHtml: `${LEGACY_HTML_ROOT}/nl_query.html`,
    reactEntry: 'frontend/ai-react/src/entries/nl_query.tsx',
    hostId: 'ai-react-root',
    canonicalUrl: '/frontend/nl_query.html',
    evidence: createPlannedEvidence('nl_query_react', 'nl_query'),
    notes: 'Hosted by Vue NlQuery.vue via AiReactEntryShell.',
  },
  {
    id: 'dispatch_board_ai',
    kind: 'react-drawer',
    status: 'contract-captured',
    legacyHtml: `${LEGACY_HTML_ROOT}/dispatch_board.html`,
    reactEntry: 'frontend/ai-react/src/entries/dispatch_board_ai.tsx',
    hostId: 'dispatch-ai-root',
    canonicalUrl: '/frontend/dispatch_board.html',
    evidence: createPlannedEvidence('dispatch_board_ai', 'dispatch_board'),
    notes: 'Embedded drawer mounted inside Vue DispatchBoard via AiReactEntryShell.',
  },
];

const REACT_ENTRY_NAME_BY_ROW_ID: Record<string, string> = {
  ai_monitor_react: 'ai_monitor',
  nl_query_react: 'nl_query',
  dispatch_board_ai: 'dispatch_board_ai',
};

const RETIRED_SURFACES: RetiredSurfaceParityRow[] = [
  {
    id: 'llm_eval_lab_react',
    kind: 'retired',
    status: 'retired',
    reactEntry: 'frontend/ai-react/src/entries/llm_eval_lab.tsx',
    notes:
      'React entry retired; Vue LlmEvalLab.vue is the single owner of /frontend/llm_eval_lab.html.',
  },
  {
    id: 'dashboard_ai_widget',
    kind: 'retired',
    status: 'retired',
    reactEntry: 'frontend/ai-react/src/entries/dashboard_ai_widget.tsx',
    notes:
      'React entry retired; Vue DashboardAiWidget.vue is the single owner of the dashboard AI widget.',
  },
  {
    id: 'ai_config_center_react',
    kind: 'retired',
    status: 'retired',
    reactEntry: 'frontend/ai-react/src/entries/ai_config_center.tsx',
    notes:
      'React entry retired; Vue AiConfigCenter.vue is the single owner of /frontend/ai_config_center.html.',
  },
  {
    id: 'flight_monitor_ai',
    kind: 'retired',
    status: 'retired',
    reactEntry: 'frontend/ai-react/src/entries/flight_monitor_ai.tsx',
    notes:
      'React entry retired; replacement capability evidence is still required before Flight Monitor can be verified.',
  },
  {
    id: 'flowable_assistant_ai',
    kind: 'retired',
    status: 'retired',
    reactEntry: 'frontend/ai-react/src/entries/flowable_assistant_ai.tsx',
    notes:
      'React entry retired; replacement capability evidence is still required before Flowable Modeler can be verified.',
  },
];

/** Vue-only shells with no legacy archive HTML (not in the 21 legacy parity set). */
const VUE_NATIVE_PAGES: VueNativeSurfaceParityRow[] = [
  {
    id: 'workspace',
    kind: 'vue-native',
    status: 'unverified',
    vueHtml: `${PARITY_ROOT}/workspace.html`,
    vueEntry: `${PARITY_ROOT}/src/entries/workspace.ts`,
    vueComponent: `${PARITY_ROOT}/src/pages/workspace/WorkspacePage.vue`,
    canonicalUrl: '/frontend/workspace.html',
    notes: 'Workspace iframe shell; Vue-native, no legacy HTML counterpart.',
  },
  {
    id: 'ontology_center',
    kind: 'vue-native',
    status: 'unverified',
    vueHtml: `${PARITY_ROOT}/ontology_center.html`,
    vueEntry: `${PARITY_ROOT}/src/entries/ontology_center.ts`,
    vueComponent: `${PARITY_ROOT}/src/pages/ontology_center/OntologyCenter.vue`,
    canonicalUrl: '/frontend/ontology_center.html',
    notes:
      'Ontology V1 workbench; Vue-native. Functional coverage: e2e/ontology_center.spec.ts (not legacy pixel parity).',
  },
];

const SPECIAL_ENTRIES: Array<RedirectSurfaceParityRow | DebugExcludedSurfaceParityRow> = [
  {
    id: 'index',
    kind: 'redirect',
    status: 'redirect',
    vueHtml: 'frontend/vue-app/index.html',
    canonicalUrl: '/frontend/index.html',
    notes: 'Meta-refresh redirect to /frontend/dashboard.html with accessible fallback link.',
  },
  {
    id: 'test',
    kind: 'debug-removed',
    status: 'debug-excluded',
    notes:
      'Removed from frontend/vue-app and from production-html-entries.ts. Inventory test fails if it returns.',
  },
];

export const PAGE_PARITY_MATRIX: readonly SurfaceParityRow[] = [
  ...VUE_PAGES,
  ...VUE_NATIVE_PAGES,
  ...REACT_AI_SURFACES,
  ...RETIRED_SURFACES,
  ...SPECIAL_ENTRIES,
];

export function isEvidenceGatedSurface(
  row: SurfaceParityRow,
): row is EvidenceGatedSurfaceParityRow {
  return 'evidence' in row && row.evidence !== undefined;
}

export const ACTIVE_REACT_ENTRY_IDS = REACT_AI_SURFACES.map(
  (row) => REACT_ENTRY_NAME_BY_ROW_ID[row.id] ?? row.id,
);
export const RETIRED_REACT_ENTRY_IDS = ['llm_eval_lab', 'dashboard_ai_widget', 'ai_config_center', 'flight_monitor_ai', 'flowable_assistant_ai'];
