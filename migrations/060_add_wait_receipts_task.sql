-- 迁移：添加 wait_receipts 用户任务到登机口开包流程
-- 日期：2026-04-05
-- 说明：将流程从 [发送通知] → ◇网关◇ 改为 [发送通知] → [wait_receipts] → ◇网关◇
--       使 ReceiptDrivenWorkflowCoordinator 能够处理回执事件并更新 workflowOutcome

UPDATE business_case_types
SET bpmn_xml = '<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" xmlns:fm="http://flight-monitor/schema/bpmn" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="baggage_check_01" name="登机口开包流程" isExecutable="true">
    <bpmn:extensionElements>
      <fm:workflowTemplate templateCode="baggage_check_01" caseType="baggage_check_01" flightContextPolicy="standard_flight_snapshot"/>
      <fm:contextMapping extraInfoFields="gate,trigger_reason,extra_info" flightFields="flight_id,flight_no,gate,stand,terminal,scheduled_departure,estimated_departure,registration,status"/>
    </bpmn:extensionElements>
    <bpmn:startEvent id="StartEvent_1" name="启动流程">
      <bpmn:outgoing>Flow_1</bpmn:outgoing>
    </bpmn:startEvent>
    <bpmn:userTask id="notify_departments" name="发送调度通知">
      <bpmn:extensionElements>
        <fm:notificationRule action="dispatch_notify" severity="critical" receiptRequired="true" appendExtraInfo="false" title="通知 ${flight_no}开包" bodyTemplate="航班 ${flight_no}座位号 ${extra_info}开包">
          <fm:targets>
            <fm:target departmentId="01KM5RXB9WNNG3Q0XQ79DYN9TH" department="国内服务科" roles="dispatcher"/>
            <fm:target departmentId="01KMZNT4D2WP3V867VST7K09Q9" department="现场监管科" roles="dispatcher"/>
            <fm:target departmentId="01KMZNVYYKN2DW3BJ47YJ004BX" department="装卸科" roles="dispatcher"/>
          </fm:targets>
        </fm:notificationRule>
        <fm:receiptRule completionPolicy="all_notified_acknowledged" rejectPolicy="fail_on_any_reject"/>
        <fm:recipientResolver source="department_roles" emptyPolicy="fail" deduplicate="true"/>
      </bpmn:extensionElements>
      <bpmn:incoming>Flow_1</bpmn:incoming>
      <bpmn:outgoing>Flow_2</bpmn:outgoing>
    </bpmn:userTask>
    <bpmn:userTask id="wait_receipts" name="等待回执">
      <bpmn:incoming>Flow_2</bpmn:incoming>
      <bpmn:outgoing>Flow_3</bpmn:outgoing>
    </bpmn:userTask>
    <bpmn:exclusiveGateway id="gateway_outcome" name="回执结果">
      <bpmn:incoming>Flow_3</bpmn:incoming>
      <bpmn:outgoing>Flow_4</bpmn:outgoing>
      <bpmn:outgoing>Flow_5</bpmn:outgoing>
    </bpmn:exclusiveGateway>
    <bpmn:userTask id="complete_business_case" name="结束业务事项(成功)">
      <bpmn:extensionElements>
        <fm:businessCaseAction action="complete_case" targetStatus="COMPLETED" reasonTemplate="所有通知对象已确认，${flight_no} 事项完成。" writeFinishedAt="true" requireCaseId="true"/>
      </bpmn:extensionElements>
      <bpmn:incoming>Flow_4</bpmn:incoming>
      <bpmn:outgoing>Flow_6</bpmn:outgoing>
    </bpmn:userTask>
    <bpmn:endEvent id="EndEvent_1" name="成功结束">
      <bpmn:incoming>Flow_6</bpmn:incoming>
    </bpmn:endEvent>
    <bpmn:userTask id="fail_business_case" name="结束业务事项(失败)">
      <bpmn:extensionElements>
        <fm:businessCaseAction action="fail_case" targetStatus="FAILED" reasonTemplate="收到拒收回执，${flight_no} 事项失败。原因：${failedReason}" writeFinishedAt="true" requireCaseId="true"/>
      </bpmn:extensionElements>
      <bpmn:incoming>Flow_5</bpmn:incoming>
      <bpmn:outgoing>Flow_7</bpmn:outgoing>
    </bpmn:userTask>
    <bpmn:endEvent id="EndEvent_2" name="失败结束">
      <bpmn:incoming>Flow_7</bpmn:incoming>
    </bpmn:endEvent>
    <bpmn:sequenceFlow id="Flow_1" sourceRef="StartEvent_1" targetRef="notify_departments"/>
    <bpmn:sequenceFlow id="Flow_2" sourceRef="notify_departments" targetRef="wait_receipts"/>
    <bpmn:sequenceFlow id="Flow_3" sourceRef="wait_receipts" targetRef="gateway_outcome"/>
    <bpmn:sequenceFlow id="Flow_4" sourceRef="gateway_outcome" targetRef="complete_business_case">
      <bpmn:conditionExpression xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="bpmn:tFormalExpression">${workflowOutcome == ''confirmed''}</bpmn:conditionExpression>
    </bpmn:sequenceFlow>
    <bpmn:sequenceFlow id="Flow_5" sourceRef="gateway_outcome" targetRef="fail_business_case">
      <bpmn:conditionExpression xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="bpmn:tFormalExpression">${workflowOutcome != ''confirmed''}</bpmn:conditionExpression>
    </bpmn:sequenceFlow>
    <bpmn:sequenceFlow id="Flow_6" sourceRef="complete_business_case" targetRef="EndEvent_1"/>
    <bpmn:sequenceFlow id="Flow_7" sourceRef="fail_business_case" targetRef="EndEvent_2"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="baggage_check_01">
      <bpmndi:BPMNShape id="_BPMNShape_StartEvent_1" bpmnElement="StartEvent_1">
        <dc:Bounds x="130" y="178" width="36" height="36"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="notify_departments_di" bpmnElement="notify_departments">
        <dc:Bounds x="220" y="156" width="130" height="80"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="wait_receipts_di" bpmnElement="wait_receipts">
        <dc:Bounds x="400" y="156" width="130" height="80"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="gateway_outcome_di" bpmnElement="gateway_outcome" isMarkerVisible="true">
        <dc:Bounds x="600" y="171" width="50" height="50"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="complete_business_case_di" bpmnElement="complete_business_case">
        <dc:Bounds x="720" y="70" width="140" height="80"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="EndEvent_1_di" bpmnElement="EndEvent_1">
        <dc:Bounds x="930" y="92" width="36" height="36"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="fail_business_case_di" bpmnElement="fail_business_case">
        <dc:Bounds x="720" y="270" width="140" height="80"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="EndEvent_2_di" bpmnElement="EndEvent_2">
        <dc:Bounds x="930" y="292" width="36" height="36"/>
      </bpmndi:BPMNShape>
      <bpmndi:BPMNEdge id="Flow_1_di" bpmnElement="Flow_1">
        <di:waypoint x="166" y="196"/>
        <di:waypoint x="220" y="196"/>
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Flow_2_di" bpmnElement="Flow_2">
        <di:waypoint x="350" y="196"/>
        <di:waypoint x="400" y="196"/>
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Flow_3_di" bpmnElement="Flow_3">
        <di:waypoint x="530" y="196"/>
        <di:waypoint x="600" y="196"/>
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Flow_4_di" bpmnElement="Flow_4">
        <di:waypoint x="650" y="196"/>
        <di:waypoint x="685" y="196"/>
        <di:waypoint x="685" y="110"/>
        <di:waypoint x="720" y="110"/>
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Flow_5_di" bpmnElement="Flow_5">
        <di:waypoint x="650" y="196"/>
        <di:waypoint x="685" y="196"/>
        <di:waypoint x="685" y="310"/>
        <di:waypoint x="720" y="310"/>
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Flow_6_di" bpmnElement="Flow_6">
        <di:waypoint x="860" y="110"/>
        <di:waypoint x="930" y="110"/>
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Flow_7_di" bpmnElement="Flow_7">
        <di:waypoint x="860" y="310"/>
        <di:waypoint x="930" y="310"/>
      </bpmndi:BPMNEdge>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>'
WHERE code = 'baggage_check_01';
