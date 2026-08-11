import { shouldPersistFormTask } from './formTaskValidation';
import type { FormTaskBindingConfig, ParsedFormTaskBindingConfig } from './types';

export const BPMN_NAMESPACE = 'http://www.omg.org/spec/BPMN/20100524/MODEL';
export const FM_NAMESPACE = 'http://flight-monitor/schema/bpmn';

interface ParsedFormBindingResult {
  processDefinitionKey: string;
  bindings: Record<string, ParsedFormTaskBindingConfig>;
}

function parseXml(xml: string): Document {
  return new DOMParser().parseFromString(xml, 'application/xml');
}

function localNameOf(element: Element | null | undefined): string {
  if (!element) {
    return '';
  }
  return element.localName || element.tagName.split(':').pop() || '';
}

function directChild(parent: Element, localName: string): Element | null {
  return Array.from(parent.children).find((child) => localNameOf(child) === localName) ?? null;
}

function readAttribute(element: Element, name: string): string {
  return element.getAttribute(name)?.trim() || '';
}

function readBooleanAttribute(element: Element, name: string, fallback: boolean): boolean {
  const value = element.getAttribute(name);
  if (value == null) {
    return fallback;
  }
  const normalized = value.trim().toLowerCase();
  if (normalized === 'true') {
    return true;
  }
  if (normalized === 'false') {
    return false;
  }
  return fallback;
}

function readNumberAttribute(element: Element, name: string, fallback: number): number {
  const value = Number(element.getAttribute(name));
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function parseRoles(value: string): string[] {
  const seen = new Set<string>();
  return value
    .split(/[,\n，]+/)
    .map((item) => item.trim())
    .filter((item) => {
      if (!item || seen.has(item)) {
        return false;
      }
      seen.add(item);
      return true;
    });
}

function ensureExtensionElements(document: Document, parent: Element): Element {
  const existing = directChild(parent, 'extensionElements');
  if (existing) {
    return existing;
  }

  const extensionElements = document.createElementNS(BPMN_NAMESPACE, 'bpmn:extensionElements');
  const firstElementChild = Array.from(parent.childNodes).find((node) => node.nodeType === Node.ELEMENT_NODE);
  if (firstElementChild) {
    parent.insertBefore(extensionElements, firstElementChild);
  } else {
    parent.appendChild(extensionElements);
  }
  return extensionElements;
}

function removeFormBindingChildren(extensionElements: Element): void {
  Array.from(extensionElements.children).forEach((child) => {
    if (localNameOf(child) === 'formBinding') {
      extensionElements.removeChild(child);
    }
  });
}

function pruneEmptyExtensionElements(parent: Element): void {
  const extensionElements = directChild(parent, 'extensionElements');
  if (!extensionElements) {
    return;
  }

  const hasElementChildren = Array.from(extensionElements.childNodes)
    .some((node) => node.nodeType === Node.ELEMENT_NODE);
  if (!hasElementChildren) {
    parent.removeChild(extensionElements);
  }
}

function collectExtensionChildrenById(document: Document): Map<string, Element[]> {
  const map = new Map<string, Element[]>();
  Array.from(document.getElementsByTagName('*')).forEach((element) => {
    const id = element.getAttribute('id')?.trim();
    if (!id) {
      return;
    }

    const extensionElements = directChild(element, 'extensionElements');
    if (!extensionElements) {
      return;
    }

    map.set(id, Array.from(extensionElements.children));
  });
  return map;
}

function mergeExistingExtensionElements(source: Document | null, target: Document): void {
  if (!source) {
    return;
  }

  const sourceExtensions = collectExtensionChildrenById(source);
  Array.from(target.getElementsByTagName('*')).forEach((element) => {
    const id = element.getAttribute('id')?.trim();
    if (!id) {
      return;
    }

    const sourceChildren = sourceExtensions.get(id);
    if (!sourceChildren || sourceChildren.length === 0) {
      return;
    }

    const targetExtensionElements = ensureExtensionElements(target, element);
    while (targetExtensionElements.firstChild) {
      targetExtensionElements.removeChild(targetExtensionElements.firstChild);
    }

    sourceChildren.forEach((child) => {
      targetExtensionElements.appendChild(target.importNode(child, true));
    });
  });
}

function safeTrim(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function createFormBindingElement(document: Document, binding: FormTaskBindingConfig): Element {
  const element = document.createElementNS(FM_NAMESPACE, 'fm:formBinding');
  const title = safeTrim(binding.title) || '表单任务节点';
  const templateCode = safeTrim(binding.templateCode);
  const formCode = safeTrim(binding.formCode);
  const department = safeTrim(binding.department);
  const writeBackKey = safeTrim(binding.writeBackKey);
  const description = safeTrim(binding.description);
  const roles = Array.isArray(binding.roles)
    ? binding.roles.map((role) => safeTrim(role)).filter(Boolean)
    : [];
  const version = Number.isFinite(Number(binding.version)) && Number(binding.version) > 0
    ? Number(binding.version)
    : 1;

  element.setAttribute('title', title);
  element.setAttribute('templateCode', templateCode);
  element.setAttribute('formCode', formCode);
  element.setAttribute('version', String(version));
  element.setAttribute('department', department);
  element.setAttribute('roles', roles.join(','));
  element.setAttribute('writeBackKey', writeBackKey);
  element.setAttribute('completeTaskOnSubmit', String(Boolean(binding.completeTaskOnSubmit)));
  element.setAttribute('allowResubmit', String(Boolean(binding.allowResubmit)));
  if (description) {
    element.setAttribute('description', description);
  }
  return element;
}

export function parseFormBindingsFromBpmnXml(xml: string): ParsedFormBindingResult {
  const document = parseXml(xml);
  const bindings: Record<string, ParsedFormTaskBindingConfig> = {};

  Array.from(document.getElementsByTagName('*')).forEach((element) => {
    if (localNameOf(element) !== 'userTask') {
      return;
    }

    const taskId = element.getAttribute('id')?.trim();
    if (!taskId) {
      return;
    }

    const extensionElements = directChild(element, 'extensionElements');
    const formBinding = extensionElements
      ? Array.from(extensionElements.children).find((child) => localNameOf(child) === 'formBinding')
      : undefined;

    if (!formBinding) {
      return;
    }

    bindings[taskId] = {
      title: readAttribute(formBinding, 'title') || readAttribute(element, 'name'),
      templateCode: readAttribute(formBinding, 'templateCode'),
      formCode: readAttribute(formBinding, 'formCode'),
      version: readNumberAttribute(formBinding, 'version', 1),
      department: readAttribute(formBinding, 'department'),
      roles: parseRoles(readAttribute(formBinding, 'roles')),
      writeBackKey: readAttribute(formBinding, 'writeBackKey'),
      completeTaskOnSubmit: readBooleanAttribute(formBinding, 'completeTaskOnSubmit', true),
      allowResubmit: readBooleanAttribute(formBinding, 'allowResubmit', false),
      description: readAttribute(formBinding, 'description'),
    };
  });

  const processDefinitionKey = Array.from(document.getElementsByTagName('*'))
    .find((element) => localNameOf(element) === 'process')
    ?.getAttribute('id')
    ?.trim() || '';

  return {
    processDefinitionKey,
    bindings,
  };
}

export function extractProcessDefinitionKey(xml: string): string {
  return parseFormBindingsFromBpmnXml(xml).processDefinitionKey;
}

export function extractUserTaskIds(xml: string): string[] {
  const document = parseXml(xml);
  return Array.from(document.getElementsByTagName('*'))
    .filter((element) => localNameOf(element) === 'userTask')
    .map((element) => element.getAttribute('id')?.trim() || '')
    .filter(Boolean);
}

export function injectFormBindingsIntoBpmnXml(
  xml: string,
  sourceXml: string | null,
  bindings: Record<string, FormTaskBindingConfig>,
): string {
  const targetDocument = parseXml(xml);
  const sourceDocument = sourceXml ? parseXml(sourceXml) : null;

  mergeExistingExtensionElements(sourceDocument, targetDocument);

  const definitions = Array.from(targetDocument.getElementsByTagName('*'))
    .find((element) => localNameOf(element) === 'definitions');

  const persistedBindings = Object.entries(bindings)
    .filter(([, binding]) => shouldPersistFormTask(binding));

  if (definitions && (persistedBindings.length > 0 || sourceXml?.includes('xmlns:fm='))) {
    definitions.setAttribute('xmlns:fm', FM_NAMESPACE);
  }

  const bindingMap = new Map(persistedBindings);

  Array.from(targetDocument.getElementsByTagName('*')).forEach((element) => {
    if (localNameOf(element) !== 'userTask') {
      return;
    }

    const taskId = element.getAttribute('id')?.trim();
    if (!taskId) {
      return;
    }

    const extensionElements = directChild(element, 'extensionElements');
    if (extensionElements) {
      removeFormBindingChildren(extensionElements);
    }

    const binding = bindingMap.get(taskId);
    if (binding) {
      const nextExtensionElements = extensionElements || ensureExtensionElements(targetDocument, element);
      nextExtensionElements.appendChild(createFormBindingElement(targetDocument, binding));
    }

    pruneEmptyExtensionElements(element);
  });

  return new XMLSerializer().serializeToString(targetDocument);
}
