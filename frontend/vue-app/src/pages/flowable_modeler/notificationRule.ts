/** 调度通知节点规则：对齐 legacy fm:notificationRule */

export const FM_NAMESPACE = 'http://flight-monitor/schema/bpmn';
export const BPMN_NAMESPACE = 'http://www.omg.org/spec/BPMN/20100524/MODEL';

export type NotificationSeverity = 'info' | 'warning' | 'critical';

export interface NotificationRule {
  action: string;
  severity: NotificationSeverity;
  receiptRequired: boolean;
  appendExtraInfo: boolean;
  title: string;
  bodyTemplate: string;
  departmentIds: string[];
  departmentSnapshots: Record<string, string>;
  roles: string[];
  completionPolicy: string;
  rejectPolicy: string;
  resolverSource: string;
  emptyPolicy: string;
  deduplicate: boolean;
}

export type BusinessNodeType =
  | 'notification'
  | 'form_task'
  | 'wait_receipts'
  | 'dispatch_task'
  | 'business_case_action'
  | 'none';

export interface NodeRuleState {
  nodeType: BusinessNodeType;
  notificationRule?: NotificationRule;
}

const SEVERITIES: NotificationSeverity[] = ['info', 'warning', 'critical'];

function safeTrim(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function localNameOf(element: Element | null | undefined): string {
  if (!element) return '';
  return element.localName || element.tagName.split(':').pop() || '';
}

function directChild(parent: Element, localName: string): Element | null {
  return Array.from(parent.children).find((child) => localNameOf(child) === localName) ?? null;
}

export function defaultNotificationRule(): NotificationRule {
  return {
    action: 'dispatch_notify',
    severity: 'warning',
    receiptRequired: true,
    appendExtraInfo: true,
    title: '通知 ${flight_no}',
    bodyTemplate: '航班 ${flight_no} 需要处理。登机口：${gate}；触发原因：${trigger_reason}',
    departmentIds: [],
    departmentSnapshots: {},
    roles: ['dispatcher', 'supervisor'],
    completionPolicy: 'all_notified_acknowledged',
    rejectPolicy: 'fail_on_any_reject',
    resolverSource: 'department_roles',
    emptyPolicy: 'fail',
    deduplicate: true,
  };
}

export function normalizeNotificationRule(raw?: Partial<NotificationRule> | null): NotificationRule {
  const defaults = defaultNotificationRule();
  if (!raw || typeof raw !== 'object') return defaults;

  const severityRaw = safeTrim(raw.severity).toLowerCase();
  const severity = (SEVERITIES.includes(severityRaw as NotificationSeverity)
    ? severityRaw
    : defaults.severity) as NotificationSeverity;

  const roles = Array.isArray(raw.roles)
    ? Array.from(new Set(raw.roles.map((r) => safeTrim(r)).filter(Boolean)))
    : defaults.roles;

  const departmentIds = Array.isArray(raw.departmentIds)
    ? Array.from(new Set(raw.departmentIds.map((id) => safeTrim(id)).filter(Boolean)))
    : [];

  const snapshots: Record<string, string> = {};
  if (raw.departmentSnapshots && typeof raw.departmentSnapshots === 'object') {
    Object.entries(raw.departmentSnapshots).forEach(([id, name]) => {
      const key = safeTrim(id);
      if (key) snapshots[key] = safeTrim(name) || key;
    });
  }

  return {
    action: safeTrim(raw.action) || defaults.action,
    severity,
    receiptRequired: raw.receiptRequired !== undefined ? Boolean(raw.receiptRequired) : defaults.receiptRequired,
    appendExtraInfo: raw.appendExtraInfo !== undefined ? Boolean(raw.appendExtraInfo) : defaults.appendExtraInfo,
    title: safeTrim(raw.title) || defaults.title,
    bodyTemplate: safeTrim(raw.bodyTemplate) || defaults.bodyTemplate,
    departmentIds,
    departmentSnapshots: snapshots,
    roles: roles.length > 0 ? roles : defaults.roles,
    completionPolicy: safeTrim(raw.completionPolicy) || defaults.completionPolicy,
    rejectPolicy: safeTrim(raw.rejectPolicy) || defaults.rejectPolicy,
    resolverSource: safeTrim(raw.resolverSource) || defaults.resolverSource,
    emptyPolicy: safeTrim(raw.emptyPolicy) || defaults.emptyPolicy,
    deduplicate: raw.deduplicate !== undefined ? Boolean(raw.deduplicate) : defaults.deduplicate,
  };
}

export function parseNotificationRulesFromBpmnXml(xml: string): Record<string, NodeRuleState> {
  const doc = new DOMParser().parseFromString(xml, 'application/xml');
  const result: Record<string, NodeRuleState> = {};

  Array.from(doc.getElementsByTagName('*')).forEach((element) => {
    if (localNameOf(element) !== 'userTask') return;
    const taskId = safeTrim(element.getAttribute('id'));
    if (!taskId) return;

    const extensionElements = directChild(element, 'extensionElements');
    if (!extensionElements) {
      if (taskId === 'wait_receipts' || safeTrim(element.getAttribute('name')).includes('等待回执')) {
        result[taskId] = { nodeType: 'wait_receipts' };
      }
      return;
    }

    const notificationRuleEl = Array.from(extensionElements.children)
      .find((child) => localNameOf(child) === 'notificationRule');

    if (notificationRuleEl) {
      const departmentIds: string[] = [];
      const departmentSnapshots: Record<string, string> = {};
      let roles: string[] = [];

      const targets = Array.from(notificationRuleEl.getElementsByTagName('*'))
        .filter((node) => localNameOf(node) === 'target');
      targets.forEach((target) => {
        const departmentId = safeTrim(target.getAttribute('departmentId') || target.getAttribute('department'));
        const departmentName = safeTrim(target.getAttribute('department')) || departmentId;
        if (!departmentId) return;
        departmentIds.push(departmentId);
        departmentSnapshots[departmentId] = departmentName;
        const targetRoles = safeTrim(target.getAttribute('roles'))
          .split(/[,\n，]+/)
          .map((r) => r.trim())
          .filter(Boolean);
        if (targetRoles.length > 0) roles = targetRoles;
      });

      const receiptRule = Array.from(extensionElements.children)
        .find((child) => localNameOf(child) === 'receiptRule');
      const resolver = Array.from(extensionElements.children)
        .find((child) => localNameOf(child) === 'recipientResolver');

      result[taskId] = {
        nodeType: 'notification',
        notificationRule: normalizeNotificationRule({
          action: notificationRuleEl.getAttribute('action') || undefined,
          severity: (notificationRuleEl.getAttribute('severity') || undefined) as NotificationSeverity,
          receiptRequired: notificationRuleEl.getAttribute('receiptRequired') !== 'false',
          appendExtraInfo: notificationRuleEl.getAttribute('appendExtraInfo') === 'true',
          title: notificationRuleEl.getAttribute('title') || undefined,
          bodyTemplate: notificationRuleEl.getAttribute('bodyTemplate') || undefined,
          departmentIds,
          departmentSnapshots,
          roles,
          completionPolicy: receiptRule?.getAttribute('completionPolicy') || undefined,
          rejectPolicy: receiptRule?.getAttribute('rejectPolicy') || undefined,
          resolverSource: resolver?.getAttribute('source') || undefined,
          emptyPolicy: resolver?.getAttribute('emptyPolicy') || undefined,
          deduplicate: resolver?.getAttribute('deduplicate') !== 'false',
        }),
      };
      return;
    }

    const formBinding = Array.from(extensionElements.children)
      .find((child) => localNameOf(child) === 'formBinding');
    if (formBinding) {
      result[taskId] = { nodeType: 'form_task' };
      return;
    }

    if (taskId === 'wait_receipts' || safeTrim(element.getAttribute('name')).includes('等待回执')) {
      result[taskId] = { nodeType: 'wait_receipts' };
    }
  });

  return result;
}

function ensureExtensionElements(document: Document, parent: Element): Element {
  const existing = directChild(parent, 'extensionElements');
  if (existing) return existing;
  const extensionElements = document.createElementNS(BPMN_NAMESPACE, 'bpmn:extensionElements');
  const firstElementChild = Array.from(parent.childNodes).find((node) => node.nodeType === Node.ELEMENT_NODE);
  if (firstElementChild) parent.insertBefore(extensionElements, firstElementChild);
  else parent.appendChild(extensionElements);
  return extensionElements;
}

function removeNotificationChildren(extensionElements: Element): void {
  Array.from(extensionElements.children).forEach((child) => {
    const name = localNameOf(child);
    if (name === 'notificationRule' || name === 'receiptRule' || name === 'recipientResolver') {
      extensionElements.removeChild(child);
    }
  });
}

export function injectNotificationRulesIntoBpmnXml(
  xml: string,
  nodeRules: Record<string, NodeRuleState>,
): string {
  const doc = new DOMParser().parseFromString(xml, 'application/xml');
  if (doc.getElementsByTagName('parsererror').length > 0) {
    throw new Error('BPMN XML 解析失败，无法写入通知规则');
  }

  const definitions = Array.from(doc.getElementsByTagName('*'))
    .find((el) => localNameOf(el) === 'definitions');
  if (definitions) {
    definitions.setAttribute('xmlns:fm', FM_NAMESPACE);
  }

  Array.from(doc.getElementsByTagName('*')).forEach((taskEl) => {
    if (localNameOf(taskEl) !== 'userTask') return;
    const taskId = safeTrim(taskEl.getAttribute('id'));
    if (!taskId) return;

    const nodeRule = nodeRules[taskId];
    const extensionElements = directChild(taskEl, 'extensionElements');
    if (extensionElements) removeNotificationChildren(extensionElements);

    if (!nodeRule || nodeRule.nodeType !== 'notification') return;

    const rule = normalizeNotificationRule(nodeRule.notificationRule);
    const nextExt = extensionElements || ensureExtensionElements(doc, taskEl);

    const notificationRuleEl = doc.createElementNS(FM_NAMESPACE, 'fm:notificationRule');
    notificationRuleEl.setAttribute('action', rule.action);
    notificationRuleEl.setAttribute('severity', rule.severity);
    notificationRuleEl.setAttribute('receiptRequired', rule.receiptRequired ? 'true' : 'false');
    notificationRuleEl.setAttribute('appendExtraInfo', rule.appendExtraInfo ? 'true' : 'false');
    notificationRuleEl.setAttribute('title', rule.title);
    notificationRuleEl.setAttribute('bodyTemplate', rule.bodyTemplate);

    const targetsEl = doc.createElementNS(FM_NAMESPACE, 'fm:targets');
    rule.departmentIds.forEach((departmentId) => {
      const targetEl = doc.createElementNS(FM_NAMESPACE, 'fm:target');
      targetEl.setAttribute('departmentId', departmentId);
      targetEl.setAttribute(
        'department',
        rule.departmentSnapshots[departmentId] || departmentId,
      );
      targetEl.setAttribute('roles', rule.roles.join(','));
      targetsEl.appendChild(targetEl);
    });
    notificationRuleEl.appendChild(targetsEl);
    nextExt.appendChild(notificationRuleEl);

    const receiptRuleEl = doc.createElementNS(FM_NAMESPACE, 'fm:receiptRule');
    receiptRuleEl.setAttribute('completionPolicy', rule.completionPolicy);
    receiptRuleEl.setAttribute('rejectPolicy', rule.rejectPolicy);
    nextExt.appendChild(receiptRuleEl);

    const resolverEl = doc.createElementNS(FM_NAMESPACE, 'fm:recipientResolver');
    resolverEl.setAttribute('source', rule.resolverSource);
    resolverEl.setAttribute('emptyPolicy', rule.emptyPolicy);
    resolverEl.setAttribute('deduplicate', rule.deduplicate ? 'true' : 'false');
    nextExt.appendChild(resolverEl);
  });

  return new XMLSerializer().serializeToString(doc);
}

export function guessNodeTypeFromName(name: string, taskId: string): BusinessNodeType {
  const n = safeTrim(name);
  const id = safeTrim(taskId);
  if (id === 'wait_receipts' || n.includes('等待回执')) return 'wait_receipts';
  if (n.includes('发送调度通知') || n.includes('通知节点') || id.includes('notify')) return 'notification';
  if (n.includes('表单任务') || n.includes('填写处理表单')) return 'form_task';
  if (n.includes('临时加单') || n.includes('派工')) return 'dispatch_task';
  if (n.includes('结束业务事项')) return 'business_case_action';
  return 'none';
}
