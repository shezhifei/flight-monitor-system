-- Migration: 074_fix_baggage_check_bpmndi_plane_id
-- Date: 2026-05-11
-- Description: 修正登机口开包流程历史 BPMNDI 中 BPMNPlane 与 BPMNDiagram 重复 ID 的问题。
--              重复 ID 会导致 bpmn-moddle 丢弃 BPMNPlane，bpmn-js 导入时报 no diagram to display。

UPDATE business_case_types
SET bpmn_xml = replace(
        bpmn_xml,
        '<bpmndi:BPMNPlane id="BPMNDiagram_1" bpmnElement="baggage_check_01">',
        '<bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="baggage_check_01">'
    ),
    updated_at = NOW()
WHERE code = 'baggage_check_01'
  AND bpmn_xml LIKE '%<bpmndi:BPMNDiagram id="BPMNDiagram_1"%'
  AND bpmn_xml LIKE '%<bpmndi:BPMNPlane id="BPMNDiagram_1" bpmnElement="baggage_check_01">%';


