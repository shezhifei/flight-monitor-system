// @vitest-environment jsdom

import { describe, expect, it } from 'vitest';
import { ensureRenderableBpmnXml } from './bpmnXml';

const PROCESS_ONLY_XML = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <bpmn:process id="flight_delay_notice" name="航班延误通知" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1" name="开始">
      <bpmn:outgoing>Flow_1</bpmn:outgoing>
    </bpmn:startEvent>
    <bpmn:userTask id="Task_01" name="通知保障部门">
      <bpmn:incoming>Flow_1</bpmn:incoming>
      <bpmn:outgoing>Flow_End_1</bpmn:outgoing>
    </bpmn:userTask>
    <bpmn:endEvent id="EndEvent_1" name="结束">
      <bpmn:incoming>Flow_End_1</bpmn:incoming>
    </bpmn:endEvent>
    <bpmn:sequenceFlow id="Flow_1" sourceRef="StartEvent_1" targetRef="Task_01" />
    <bpmn:sequenceFlow id="Flow_End_1" sourceRef="Task_01" targetRef="EndEvent_1" />
  </bpmn:process>
</bpmn:definitions>`;

describe('ensureRenderableBpmnXml', () => {
  it('adds BPMNDI diagram elements when persisted BPMN only has process semantics', () => {
    const xml = ensureRenderableBpmnXml(PROCESS_ONLY_XML, 'flight_delay_notice', '航班延误通知');

    expect(xml).toContain('<bpmndi:BPMNDiagram');
    expect(xml).toContain('<bpmndi:BPMNPlane');
    expect(xml).toContain('bpmnElement="flight_delay_notice"');
    expect(xml).toContain('bpmnElement="StartEvent_1"');
    expect(xml).toContain('bpmnElement="Task_01"');
    expect(xml).toContain('bpmnElement="EndEvent_1"');
    expect(xml).toContain('bpmnElement="Flow_1"');
    expect(xml).toContain('bpmnElement="Flow_End_1"');
  });

  it('leaves existing diagram information intact', () => {
    const withDiagram = `${PROCESS_ONLY_XML.replace('</bpmn:definitions>', '')}
  <bpmndi:BPMNDiagram xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" id="ExistingDiagram">
    <bpmndi:BPMNPlane id="ExistingPlane" bpmnElement="flight_delay_notice" />
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

    const xml = ensureRenderableBpmnXml(withDiagram, 'flight_delay_notice', '航班延误通知');

    expect(xml).toContain('id="ExistingDiagram"');
    expect(xml).not.toContain('GeneratedBPMNDiagram');
  });

  it('repairs duplicate BPMNDiagram and BPMNPlane ids from legacy persisted XML', () => {
    const withDuplicateDiagramIds = `${PROCESS_ONLY_XML.replace('</bpmn:definitions>', '')}
  <bpmndi:BPMNDiagram xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNDiagram_1" bpmnElement="flight_delay_notice" />
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

    const xml = ensureRenderableBpmnXml(withDuplicateDiagramIds, 'flight_delay_notice', '航班延误通知');

    expect(xml).toContain('id="BPMNDiagram_1"');
    expect(xml).toContain('id="BPMNPlane_1"');
    expect(xml).not.toContain('<bpmndi:BPMNPlane id="BPMNDiagram_1"');
  });
});
