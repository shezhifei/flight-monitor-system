export const BPMN_NAMESPACE = 'http://www.omg.org/spec/BPMN/20100524/MODEL';
export const BPMNDI_NAMESPACE = 'http://www.omg.org/spec/BPMN/20100524/DI';
export const DC_NAMESPACE = 'http://www.omg.org/spec/DD/20100524/DC';
export const DI_NAMESPACE = 'http://www.omg.org/spec/DD/20100524/DI';

type Bounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type FlowNodeLayout = {
  id: string;
  kind: string;
  bounds: Bounds;
};

function parseXml(xml: string): Document {
  const document = new DOMParser().parseFromString(xml, 'application/xml');
  if (document.getElementsByTagName('parsererror').length > 0) {
    throw new Error('BPMN XML 解析失败');
  }
  return document;
}

function localNameOf(element: Element | null | undefined): string {
  if (!element) {
    return '';
  }
  return element.localName || element.tagName.split(':').pop() || '';
}

function findFirstElement(document: Document, localName: string): Element | null {
  return Array.from(document.getElementsByTagName('*')).find((element) => localNameOf(element) === localName) ?? null;
}

function hasDisplayDiagram(document: Document): boolean {
  return Boolean(findFirstElement(document, 'BPMNDiagram') && findFirstElement(document, 'BPMNPlane'));
}

function directChild(parent: Element, localName: string): Element | null {
  return Array.from(parent.children).find((child) => localNameOf(child) === localName) ?? null;
}

function ensureNamespace(definitions: Element, name: string, value: string): void {
  if (!definitions.hasAttribute(name)) {
    definitions.setAttribute(name, value);
  }
}

function elementId(element: Element): string {
  return element.getAttribute('id')?.trim() || '';
}

function isFlowNode(element: Element): boolean {
  const supported = new Set([
    'startEvent',
    'endEvent',
    'userTask',
    'task',
    'serviceTask',
    'scriptTask',
    'manualTask',
    'businessRuleTask',
    'sendTask',
    'receiveTask',
    'callActivity',
    'subProcess',
    'exclusiveGateway',
    'parallelGateway',
    'inclusiveGateway',
    'eventBasedGateway',
  ]);
  return supported.has(localNameOf(element)) && Boolean(elementId(element));
}

function nodeBounds(kind: string, index: number): Bounds {
  const gateway = kind.endsWith('Gateway');
  const event = kind === 'startEvent' || kind === 'endEvent';
  const width = gateway || event ? 50 : 118;
  const height = gateway || event ? 50 : 82;
  return {
    x: 152 + index * 168,
    y: event || gateway ? 136 : 120,
    width,
    height,
  };
}

function collectFlowNodes(process: Element): FlowNodeLayout[] {
  return Array.from(process.children)
    .filter(isFlowNode)
    .map((element, index) => ({
      id: elementId(element),
      kind: localNameOf(element),
      bounds: nodeBounds(localNameOf(element), index),
    }));
}

function collectIds(document: Document): Set<string> {
  const ids = new Set<string>();
  Array.from(document.getElementsByTagName('*')).forEach((element) => {
    const id = element.getAttribute('id')?.trim();
    if (id) {
      ids.add(id);
    }
  });
  return ids;
}

function uniqueId(base: string, usedIds: Set<string>, currentId?: string): string {
  const normalizedCurrent = currentId?.trim();
  if (normalizedCurrent) {
    usedIds.delete(normalizedCurrent);
  }
  if (!usedIds.has(base)) {
    usedIds.add(base);
    return base;
  }

  let index = 1;
  let candidate = `${base}_${index}`;
  while (usedIds.has(candidate)) {
    index += 1;
    candidate = `${base}_${index}`;
  }
  usedIds.add(candidate);
  return candidate;
}

function sanitizeExistingDiagram(document: Document, process: Element): boolean {
  const diagram = findFirstElement(document, 'BPMNDiagram');
  const plane = diagram ? directChild(diagram, 'BPMNPlane') : null;
  if (!diagram || !plane) {
    return false;
  }

  const processId = process.getAttribute('id')?.trim();
  if (processId && !plane.getAttribute('bpmnElement')?.trim()) {
    plane.setAttribute('bpmnElement', processId);
  }

  const diagramId = diagram.getAttribute('id')?.trim();
  const planeId = plane.getAttribute('id')?.trim();
  if (!planeId || planeId === diagramId) {
    const usedIds = collectIds(document);
    plane.setAttribute('id', uniqueId('BPMNPlane_1', usedIds, planeId));
  }

  return true;
}

function appendBounds(document: Document, shape: Element, bounds: Bounds): void {
  const boundsElement = document.createElementNS(DC_NAMESPACE, 'dc:Bounds');
  boundsElement.setAttribute('x', String(bounds.x));
  boundsElement.setAttribute('y', String(bounds.y));
  boundsElement.setAttribute('width', String(bounds.width));
  boundsElement.setAttribute('height', String(bounds.height));
  shape.appendChild(boundsElement);
}

function appendWaypoint(document: Document, edge: Element, x: number, y: number): void {
  const waypoint = document.createElementNS(DI_NAMESPACE, 'di:waypoint');
  waypoint.setAttribute('x', String(x));
  waypoint.setAttribute('y', String(y));
  edge.appendChild(waypoint);
}

function edgeAnchor(bounds: Bounds, side: 'left' | 'right'): { x: number; y: number } {
  return {
    x: side === 'left' ? bounds.x : bounds.x + bounds.width,
    y: bounds.y + bounds.height / 2,
  };
}

export function cleanBpmnXml(xml: string | null | undefined): string | undefined {
  if (!xml) {
    return undefined;
  }
  return xml.replace(/`/g, '').trim();
}

export function ensureRenderableBpmnXml(
  rawXml: string,
  fallbackProcessKey: string,
  fallbackProcessName: string,
): string {
  const cleaned = cleanBpmnXml(rawXml);
  if (!cleaned) {
    throw new Error('BPMN XML 为空');
  }

  const document = parseXml(cleaned);
  const definitions = findFirstElement(document, 'definitions');
  const process = findFirstElement(document, 'process');
  if (!definitions || !process) {
    throw new Error('BPMN XML 缺少 definitions/process 节点');
  }

  if (!process.getAttribute('id')?.trim()) {
    process.setAttribute('id', fallbackProcessKey);
  }
  if (!process.getAttribute('name')?.trim() && fallbackProcessName) {
    process.setAttribute('name', fallbackProcessName);
  }

  ensureNamespace(definitions, 'xmlns:bpmndi', BPMNDI_NAMESPACE);
  ensureNamespace(definitions, 'xmlns:dc', DC_NAMESPACE);
  ensureNamespace(definitions, 'xmlns:di', DI_NAMESPACE);

  if (hasDisplayDiagram(document) && sanitizeExistingDiagram(document, process)) {
    return new XMLSerializer().serializeToString(document);
  }

  const diagram = document.createElementNS(BPMNDI_NAMESPACE, 'bpmndi:BPMNDiagram');
  diagram.setAttribute('id', 'GeneratedBPMNDiagram');
  const plane = document.createElementNS(BPMNDI_NAMESPACE, 'bpmndi:BPMNPlane');
  plane.setAttribute('id', 'GeneratedBPMNPlane');
  plane.setAttribute('bpmnElement', process.getAttribute('id')?.trim() || fallbackProcessKey);
  diagram.appendChild(plane);

  const nodes = collectFlowNodes(process);
  const nodeMap = new Map(nodes.map((node) => [node.id, node]));
  nodes.forEach((node) => {
    const shape = document.createElementNS(BPMNDI_NAMESPACE, 'bpmndi:BPMNShape');
    shape.setAttribute('id', `${node.id}_di`);
    shape.setAttribute('bpmnElement', node.id);
    if (node.kind.endsWith('Gateway')) {
      shape.setAttribute('isMarkerVisible', 'true');
    }
    appendBounds(document, shape, node.bounds);
    plane.appendChild(shape);
  });

  Array.from(process.children)
    .filter((element) => localNameOf(element) === 'sequenceFlow' && Boolean(elementId(element)))
    .forEach((flow) => {
      const source = nodeMap.get(flow.getAttribute('sourceRef')?.trim() || '');
      const target = nodeMap.get(flow.getAttribute('targetRef')?.trim() || '');
      if (!source || !target) {
        return;
      }

      const edge = document.createElementNS(BPMNDI_NAMESPACE, 'bpmndi:BPMNEdge');
      const flowId = elementId(flow);
      edge.setAttribute('id', `${flowId}_di`);
      edge.setAttribute('bpmnElement', flowId);
      const sourceAnchor = edgeAnchor(source.bounds, 'right');
      const targetAnchor = edgeAnchor(target.bounds, 'left');
      appendWaypoint(document, edge, sourceAnchor.x, sourceAnchor.y);
      appendWaypoint(document, edge, targetAnchor.x, targetAnchor.y);
      plane.appendChild(edge);
    });

  definitions.appendChild(diagram);
  return new XMLSerializer().serializeToString(document);
}
