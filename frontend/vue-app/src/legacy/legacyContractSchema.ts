export const LEGACY_CONTRACT_VERSION = 1 as const;

export const LEGACY_CONTRACT_PAGES = [
  'ai_config_center',
  'ai_monitor',
  'anomaly_monitor',
  'command_center',
  'dashboard',
  'dispatch_board',
  'dispatch_rule_center',
  'flight_imports',
  'flight_monitor',
  'flowable_modeler',
  'kpi_dashboard',
  'label_manager',
  'llm_eval_lab',
  'login',
  'nl_query',
  'operations_review_report',
  'resource_manager',
  'resource_utilization',
  'system_flags',
  'system_status',
  'user_manager',
] as const;

export type LegacyContractPage = (typeof LEGACY_CONTRACT_PAGES)[number];
export type LegacyAssetKind = 'script' | 'stylesheet' | 'asset' | 'external' | 'dynamic';
export type LegacyCaptureStatus =
  | 'rendered'
  | 'rendered-with-fixture-gaps'
  | 'rendered-with-errors';
export type LegacyStorageArea = 'localStorage' | 'sessionStorage';
export type LegacyStorageOperation = 'set' | 'remove' | 'clear';

export interface LegacyAssetReference {
  kind: LegacyAssetKind;
  reference: string;
  archivePath: string | null;
  exists: boolean;
  sha256: string | null;
}

export interface LegacySourceContract {
  html: string;
  htmlSha256: string;
  scripts: LegacyAssetReference[];
  stylesheets: LegacyAssetReference[];
  assets: LegacyAssetReference[];
  sourceHash: string;
}

export interface LegacyRegionContract {
  selector: string;
  tag: string;
  role: string | null;
  text: string;
  visible: boolean;
}

export interface LegacyHeadingContract {
  selector: string;
  level: number;
  text: string;
}

export interface LegacyControlContract {
  selector: string;
  tag: string;
  type: string;
  text: string;
  name: string | null;
  ariaLabel: string | null;
  disabled: boolean;
}

export interface LegacyLabelContract {
  selector: string;
  text: string;
  for: string | null;
}

export interface LegacyFormContract {
  selector: string;
  method: string;
  action: string;
  fields: string[];
}

export interface LegacyTableContract {
  selector: string;
  columns: string[];
}

export interface LegacyOverlayContract {
  selector: string;
  title: string;
  visible: boolean;
}

export interface LegacyLinkContract {
  selector: string;
  text: string;
  href: string;
}

export interface LegacyPermissionRuleContract {
  selector: string;
  attribute: string;
  value: string;
}

export interface LegacySurfaceContract {
  regions: LegacyRegionContract[];
  headings: LegacyHeadingContract[];
  controls: LegacyControlContract[];
  labels: LegacyLabelContract[];
  forms: LegacyFormContract[];
  tables: LegacyTableContract[];
  tabs: LegacyControlContract[];
  dialogs: LegacyOverlayContract[];
  drawers: LegacyOverlayContract[];
  links: LegacyLinkContract[];
  stableSelectors: string[];
  permissionRules: LegacyPermissionRuleContract[];
}

export interface LegacyApiRequestContract {
  method: string;
  pathname: string;
  query: Record<string, string[]>;
  body: unknown;
}

export interface LegacyFixtureGapContract extends LegacyApiRequestContract {
  reason: string;
}

export interface LegacyStorageMutationContract {
  storage: LegacyStorageArea;
  operation: LegacyStorageOperation;
  key: string | null;
  value: string | null;
}

export interface LegacyUrlChangeContract {
  kind: string;
  from: string;
  to: string;
}

export interface LegacySseSubscriptionContract {
  url: string;
  pathname: string;
  query: Record<string, string[]>;
  fixtureId: string | null;
  eventTypes: string[];
}

export interface LegacyScenarioContract {
  id: string;
  fixture: string;
  captureStatus: LegacyCaptureStatus;
  coverageGaps: string[];
  expectedHttpStatuses: number[];
  expectedLegacyErrors: string[];
  apiRequests: LegacyApiRequestContract[];
  fixtureGaps: LegacyFixtureGapContract[];
  storageMutations: LegacyStorageMutationContract[];
  urlChanges: LegacyUrlChangeContract[];
  sseSubscriptions: LegacySseSubscriptionContract[];
  consoleErrors: string[];
}

export interface LegacyContract {
  contractVersion: typeof LEGACY_CONTRACT_VERSION;
  page: LegacyContractPage;
  generatedAt: string;
  source: LegacySourceContract;
  surface: LegacySurfaceContract;
  scenarios: LegacyScenarioContract[];
  approvedExceptions: string[];
}

export interface LegacyContractValidationResult {
  valid: boolean;
  issues: string[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireRecord(value: unknown, path: string, issues: string[]): Record<string, unknown> | null {
  if (!isRecord(value)) {
    issues.push(`${path} must be an object`);
    return null;
  }
  return value;
}

function requireString(value: unknown, path: string, issues: string[], allowEmpty = false): void {
  if (typeof value !== 'string' || (!allowEmpty && value.trim().length === 0)) {
    issues.push(`${path} must be ${allowEmpty ? 'a string' : 'a non-empty string'}`);
  }
}

function requireNullableString(value: unknown, path: string, issues: string[]): void {
  if (value !== null && typeof value !== 'string') issues.push(`${path} must be a string or null`);
}

function requireSha256(value: unknown, path: string, issues: string[]): void {
  if (typeof value !== 'string' || !/^[a-f0-9]{64}$/.test(value)) {
    issues.push(`${path} must be a lowercase SHA-256 digest`);
  }
}

function requireBoolean(value: unknown, path: string, issues: string[]): void {
  if (typeof value !== 'boolean') issues.push(`${path} must be a boolean`);
}

function requireStringArray(value: unknown, path: string, issues: string[]): void {
  if (!Array.isArray(value)) {
    issues.push(`${path} must be an array`);
    return;
  }
  value.forEach((item, index) => requireString(item, `${path}[${index}]`, issues, true));
}

function requireHttpStatusArray(value: unknown, path: string, issues: string[]): void {
  if (!Array.isArray(value)) {
    issues.push(`${path} must be an array`);
    return;
  }
  value.forEach((item, index) => {
    if (!Number.isInteger(item) || Number(item) < 400 || Number(item) > 599) {
      issues.push(`${path}[${index}] must be an HTTP error status from 400 to 599`);
    }
  });
}

function requireArray(
  value: unknown,
  path: string,
  issues: string[],
  validateItem: (item: unknown, itemPath: string, itemIssues: string[]) => void,
): void {
  if (!Array.isArray(value)) {
    issues.push(`${path} must be an array`);
    return;
  }
  value.forEach((item, index) => validateItem(item, `${path}[${index}]`, issues));
}

function validateQuery(value: unknown, path: string, issues: string[]): void {
  const query = requireRecord(value, path, issues);
  if (!query) return;
  Object.entries(query).forEach(([key, values]) => requireStringArray(values, `${path}.${key}`, issues));
}

function validateAsset(value: unknown, path: string, issues: string[]): void {
  const asset = requireRecord(value, path, issues);
  if (!asset) return;
  if (!['script', 'stylesheet', 'asset', 'external', 'dynamic'].includes(String(asset.kind))) {
    issues.push(`${path}.kind is invalid`);
  }
  requireString(asset.reference, `${path}.reference`, issues);
  requireNullableString(asset.archivePath, `${path}.archivePath`, issues);
  requireBoolean(asset.exists, `${path}.exists`, issues);
  requireNullableString(asset.sha256, `${path}.sha256`, issues);
  if (asset.exists === true && asset.archivePath !== null && asset.sha256 === null) {
    issues.push(`${path}.sha256 is required for an existing archive asset`);
  }
  if (typeof asset.sha256 === 'string') requireSha256(asset.sha256, `${path}.sha256`, issues);
  if (typeof asset.archivePath === 'string' && (
    /^(?:[a-zA-Z]:|[/\\]|frontend[/\\])/.test(asset.archivePath)
    || asset.archivePath.split(/[\\/]/).includes('..')
  )) {
    issues.push(`${path}.archivePath must be archive-relative and must not use stale frontend/ paths`);
  }
  if (asset.archivePath === null && asset.exists !== false) {
    issues.push(`${path}.exists must be false for a non-archive reference`);
  }
}

function validateControl(value: unknown, path: string, issues: string[]): void {
  const control = requireRecord(value, path, issues);
  if (!control) return;
  requireString(control.selector, `${path}.selector`, issues);
  requireString(control.tag, `${path}.tag`, issues);
  requireString(control.type, `${path}.type`, issues, true);
  requireString(control.text, `${path}.text`, issues, true);
  requireNullableString(control.name, `${path}.name`, issues);
  requireNullableString(control.ariaLabel, `${path}.ariaLabel`, issues);
  requireBoolean(control.disabled, `${path}.disabled`, issues);
}

function validateSurface(value: unknown, path: string, issues: string[]): void {
  const surface = requireRecord(value, path, issues);
  if (!surface) return;
  requireArray(surface.regions, `${path}.regions`, issues, (item, itemPath, itemIssues) => {
    const region = requireRecord(item, itemPath, itemIssues);
    if (!region) return;
    requireString(region.selector, `${itemPath}.selector`, itemIssues);
    requireString(region.tag, `${itemPath}.tag`, itemIssues);
    requireNullableString(region.role, `${itemPath}.role`, itemIssues);
    requireString(region.text, `${itemPath}.text`, itemIssues, true);
    requireBoolean(region.visible, `${itemPath}.visible`, itemIssues);
  });
  requireArray(surface.headings, `${path}.headings`, issues, (item, itemPath, itemIssues) => {
    const heading = requireRecord(item, itemPath, itemIssues);
    if (!heading) return;
    requireString(heading.selector, `${itemPath}.selector`, itemIssues);
    if (!Number.isInteger(heading.level) || Number(heading.level) < 1 || Number(heading.level) > 6) {
      itemIssues.push(`${itemPath}.level must be an integer from 1 to 6`);
    }
    requireString(heading.text, `${itemPath}.text`, itemIssues, true);
  });
  requireArray(surface.controls, `${path}.controls`, issues, validateControl);
  requireArray(surface.labels, `${path}.labels`, issues, (item, itemPath, itemIssues) => {
    const label = requireRecord(item, itemPath, itemIssues);
    if (!label) return;
    requireString(label.selector, `${itemPath}.selector`, itemIssues);
    requireString(label.text, `${itemPath}.text`, itemIssues, true);
    requireNullableString(label.for, `${itemPath}.for`, itemIssues);
  });
  requireArray(surface.forms, `${path}.forms`, issues, (item, itemPath, itemIssues) => {
    const form = requireRecord(item, itemPath, itemIssues);
    if (!form) return;
    requireString(form.selector, `${itemPath}.selector`, itemIssues);
    requireString(form.method, `${itemPath}.method`, itemIssues);
    requireString(form.action, `${itemPath}.action`, itemIssues, true);
    requireStringArray(form.fields, `${itemPath}.fields`, itemIssues);
  });
  requireArray(surface.tables, `${path}.tables`, issues, (item, itemPath, itemIssues) => {
    const table = requireRecord(item, itemPath, itemIssues);
    if (!table) return;
    requireString(table.selector, `${itemPath}.selector`, itemIssues);
    requireStringArray(table.columns, `${itemPath}.columns`, itemIssues);
  });
  requireArray(surface.tabs, `${path}.tabs`, issues, validateControl);
  const validateOverlay = (item: unknown, itemPath: string, itemIssues: string[]) => {
    const overlay = requireRecord(item, itemPath, itemIssues);
    if (!overlay) return;
    requireString(overlay.selector, `${itemPath}.selector`, itemIssues);
    requireString(overlay.title, `${itemPath}.title`, itemIssues, true);
    requireBoolean(overlay.visible, `${itemPath}.visible`, itemIssues);
  };
  requireArray(surface.dialogs, `${path}.dialogs`, issues, validateOverlay);
  requireArray(surface.drawers, `${path}.drawers`, issues, validateOverlay);
  requireArray(surface.links, `${path}.links`, issues, (item, itemPath, itemIssues) => {
    const link = requireRecord(item, itemPath, itemIssues);
    if (!link) return;
    requireString(link.selector, `${itemPath}.selector`, itemIssues);
    requireString(link.text, `${itemPath}.text`, itemIssues, true);
    requireString(link.href, `${itemPath}.href`, itemIssues, true);
  });
  requireStringArray(surface.stableSelectors, `${path}.stableSelectors`, issues);
  requireArray(surface.permissionRules, `${path}.permissionRules`, issues, (item, itemPath, itemIssues) => {
    const rule = requireRecord(item, itemPath, itemIssues);
    if (!rule) return;
    requireString(rule.selector, `${itemPath}.selector`, itemIssues);
    requireString(rule.attribute, `${itemPath}.attribute`, itemIssues);
    requireString(rule.value, `${itemPath}.value`, itemIssues, true);
  });
}

function validateScenario(value: unknown, path: string, issues: string[]): void {
  const scenario = requireRecord(value, path, issues);
  if (!scenario) return;
  requireString(scenario.id, `${path}.id`, issues);
  requireString(scenario.fixture, `${path}.fixture`, issues);
  if (!['rendered', 'rendered-with-fixture-gaps', 'rendered-with-errors'].includes(String(scenario.captureStatus))) {
    issues.push(`${path}.captureStatus is invalid`);
  }
  requireStringArray(scenario.coverageGaps, `${path}.coverageGaps`, issues);
  requireHttpStatusArray(scenario.expectedHttpStatuses, `${path}.expectedHttpStatuses`, issues);
  requireStringArray(scenario.expectedLegacyErrors, `${path}.expectedLegacyErrors`, issues);
  requireArray(scenario.apiRequests, `${path}.apiRequests`, issues, (item, itemPath, itemIssues) => {
    const request = requireRecord(item, itemPath, itemIssues);
    if (!request) return;
    requireString(request.method, `${itemPath}.method`, itemIssues);
    requireString(request.pathname, `${itemPath}.pathname`, itemIssues);
    validateQuery(request.query, `${itemPath}.query`, itemIssues);
    if (!Object.hasOwn(request, 'body')) itemIssues.push(`${itemPath}.body is required`);
  });
  requireArray(scenario.fixtureGaps, `${path}.fixtureGaps`, issues, (item, itemPath, itemIssues) => {
    const gap = requireRecord(item, itemPath, itemIssues);
    if (!gap) return;
    requireString(gap.method, `${itemPath}.method`, itemIssues);
    requireString(gap.pathname, `${itemPath}.pathname`, itemIssues);
    validateQuery(gap.query, `${itemPath}.query`, itemIssues);
    if (!Object.hasOwn(gap, 'body')) itemIssues.push(`${itemPath}.body is required`);
    requireString(gap.reason, `${itemPath}.reason`, itemIssues);
  });
  if (scenario.captureStatus === 'rendered' && Array.isArray(scenario.fixtureGaps) && scenario.fixtureGaps.length > 0) {
    issues.push(`${path}.captureStatus cannot be rendered while fixture gaps exist`);
  }
  if (scenario.captureStatus === 'rendered' && Array.isArray(scenario.coverageGaps) && scenario.coverageGaps.length > 0) {
    issues.push(`${path}.captureStatus cannot be rendered while coverage gaps exist`);
  }
  requireArray(scenario.storageMutations, `${path}.storageMutations`, issues, (item, itemPath, itemIssues) => {
    const mutation = requireRecord(item, itemPath, itemIssues);
    if (!mutation) return;
    if (!['localStorage', 'sessionStorage'].includes(String(mutation.storage))) {
      itemIssues.push(`${itemPath}.storage is invalid`);
    }
    if (!['set', 'remove', 'clear'].includes(String(mutation.operation))) {
      itemIssues.push(`${itemPath}.operation is invalid`);
    }
    requireNullableString(mutation.key, `${itemPath}.key`, itemIssues);
    requireNullableString(mutation.value, `${itemPath}.value`, itemIssues);
  });
  requireArray(scenario.urlChanges, `${path}.urlChanges`, issues, (item, itemPath, itemIssues) => {
    const change = requireRecord(item, itemPath, itemIssues);
    if (!change) return;
    requireString(change.kind, `${itemPath}.kind`, itemIssues);
    requireString(change.from, `${itemPath}.from`, itemIssues);
    requireString(change.to, `${itemPath}.to`, itemIssues);
  });
  requireArray(scenario.sseSubscriptions, `${path}.sseSubscriptions`, issues, (item, itemPath, itemIssues) => {
    const subscription = requireRecord(item, itemPath, itemIssues);
    if (!subscription) return;
    requireString(subscription.url, `${itemPath}.url`, itemIssues);
    requireString(subscription.pathname, `${itemPath}.pathname`, itemIssues);
    validateQuery(subscription.query, `${itemPath}.query`, itemIssues);
    requireNullableString(subscription.fixtureId, `${itemPath}.fixtureId`, itemIssues);
    requireStringArray(subscription.eventTypes, `${itemPath}.eventTypes`, itemIssues);
    if (subscription.fixtureId !== null && Array.isArray(subscription.eventTypes) && subscription.eventTypes.length === 0) {
      itemIssues.push(`${itemPath}.eventTypes must contain named events for a matched SSE fixture`);
    }
  });
  requireStringArray(scenario.consoleErrors, `${path}.consoleErrors`, issues);
}

export function validateLegacyContract(value: unknown): LegacyContractValidationResult {
  const issues: string[] = [];
  const contract = requireRecord(value, '$', issues);
  if (!contract) return { valid: false, issues };

  if (contract.contractVersion !== LEGACY_CONTRACT_VERSION) {
    issues.push(`$.contractVersion must equal ${LEGACY_CONTRACT_VERSION}`);
  }
  if (!LEGACY_CONTRACT_PAGES.includes(contract.page as LegacyContractPage)) {
    issues.push('$.page must be one of the 21 explicit legacy pages');
  }
  requireString(contract.generatedAt, '$.generatedAt', issues);
  if (typeof contract.generatedAt === 'string' && Number.isNaN(Date.parse(contract.generatedAt))) {
    issues.push('$.generatedAt must be an ISO-compatible timestamp');
  }

  const source = requireRecord(contract.source, '$.source', issues);
  if (source) {
    requireString(source.html, '$.source.html', issues);
    if (typeof source.html === 'string' && !source.html.startsWith('html/')) {
      issues.push('$.source.html must be archive-relative under html/');
    }
    requireString(source.htmlSha256, '$.source.htmlSha256', issues);
    requireSha256(source.htmlSha256, '$.source.htmlSha256', issues);
    requireArray(source.scripts, '$.source.scripts', issues, validateAsset);
    requireArray(source.stylesheets, '$.source.stylesheets', issues, validateAsset);
    requireArray(source.assets, '$.source.assets', issues, validateAsset);
    requireString(source.sourceHash, '$.source.sourceHash', issues);
    requireSha256(source.sourceHash, '$.source.sourceHash', issues);
  }
  validateSurface(contract.surface, '$.surface', issues);
  requireArray(contract.scenarios, '$.scenarios', issues, validateScenario);
  if (Array.isArray(contract.scenarios) && contract.scenarios.length === 0) {
    issues.push('$.scenarios must contain at least one captured scenario');
  }
  requireStringArray(contract.approvedExceptions, '$.approvedExceptions', issues);

  return { valid: issues.length === 0, issues };
}

export function assertLegacyContract(value: unknown): asserts value is LegacyContract {
  const result = validateLegacyContract(value);
  if (!result.valid) {
    throw new Error(`Invalid legacy contract:\n${result.issues.map((issue) => `  - ${issue}`).join('\n')}`);
  }
}
