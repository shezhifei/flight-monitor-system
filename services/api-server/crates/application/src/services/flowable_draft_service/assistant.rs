use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use roxmltree::Document as XmlDocument;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::error::FlowableDraftServiceError;
use super::stream::FlowableDraftAssistantStreamEvent;

#[derive(Debug, Clone)]
pub(super) struct DraftAiConfig {
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) default_model: String,
    pub(super) api_format: String,
    pub(super) timeout_seconds: u64,
    pub(super) max_tokens: u32,
    pub(super) max_retries: usize,
    pub(super) retry_delay_seconds: f64,
}

#[derive(Debug, Clone)]
pub(super) struct DraftAiOutput {
    pub(super) draft_bpmn_xml: String,
    pub(super) draft_summary_markdown: String,
    pub(super) extracted_requirements: Vec<String>,
    pub(super) warnings: Vec<String>,
    pub(super) model: String,
}

pub(super) const DRAFT_JSON_SYSTEM_PROMPT: &str = "你是 BPMN 2.0 建模助手。必须输出 JSON 对象，字段：draft_bpmn_xml、draft_summary_markdown、extracted_requirements(数组)、warnings(数组)。draft_bpmn_xml 必须是可解析 XML，且包含 definitions/process/startEvent/endEvent 以及 bpmndi:BPMNDiagram/bpmndi:BPMNPlane 图形信息。";

pub(super) fn build_synthetic_draft(
    resolved_key: &str,
    resolved_name: &str,
    case_type_code: Option<&str>,
    resolved_locale: &str,
    parsed: &super::document_parse::ParsedProcessDocument,
    requirements: &[String],
    warnings: &[String],
) -> DraftAiOutput {
    DraftAiOutput {
        draft_bpmn_xml: build_bpmn_xml(resolved_key, resolved_name, requirements),
        draft_summary_markdown: build_summary_markdown(
            resolved_key,
            resolved_name,
            case_type_code,
            resolved_locale,
            requirements,
            &parsed.source_meta.filename,
            warnings,
        ),
        extracted_requirements: requirements.to_vec(),
        warnings: warnings.to_vec(),
        model: "flowable-draft-assistant-v1".to_string(),
    }
}

pub(super) fn validate_bpmn_xml(xml_text: &str) -> Option<String> {
    if xml_text.trim().is_empty() {
        return Some("BPMN XML 为空".to_string());
    }
    let doc = XmlDocument::parse(xml_text).map_err(|error| format!("XML 解析失败: {error}"));
    let Ok(doc) = doc else {
        return doc.err();
    };

    let root = doc.root_element();
    if root.tag_name().name() != "definitions" {
        return Some("BPMN 根节点必须是 definitions".to_string());
    }
    let has_process = doc.descendants().any(|node| node.has_tag_name("process"));
    if !has_process {
        return Some("BPMN 缺少 process 节点".to_string());
    }
    let has_start = doc.descendants().any(|node| node.has_tag_name("startEvent"));
    if !has_start {
        return Some("BPMN 缺少 startEvent 节点".to_string());
    }
    let has_end = doc.descendants().any(|node| node.has_tag_name("endEvent"));
    if !has_end {
        return Some("BPMN 缺少 endEvent 节点".to_string());
    }
    let has_diagram = doc.descendants().any(|node| node.has_tag_name("BPMNDiagram"));
    let has_plane = doc.descendants().any(|node| node.has_tag_name("BPMNPlane"));
    if !has_diagram || !has_plane {
        return Some("BPMN 缺少 BPMNDiagram/BPMNPlane 图形信息".to_string());
    }
    None
}

pub(super) fn parse_ai_json(content: &str) -> Value {
    serde_json::from_str::<Value>(content)
        .ok()
        .filter(|value| value.is_object())
        .or_else(|| {
            let start = content.find('{')?;
            let end = content.rfind('}')?;
            serde_json::from_str::<Value>(&content[start..=end]).ok()
        })
        .unwrap_or_else(|| json!({}))
}

pub(super) fn extract_ai_message_content(response: &Value) -> String {
    response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            response.get("output").and_then(Value::as_array).and_then(|items| {
                items.iter().find_map(|item| {
                    item.get("content").and_then(Value::as_array).and_then(|content| {
                        content
                            .iter()
                            .find_map(|part| part.get("text").and_then(Value::as_str).map(str::to_string))
                    })
                })
            })
        })
        .unwrap_or_default()
}

pub(super) fn extract_ai_stream_delta(response: &Value) -> Option<String> {
    response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let event_type = response.get("type").and_then(Value::as_str)?;
            if matches!(event_type, "response.text.delta" | "response.output_text.delta") {
                response.get("delta").and_then(Value::as_str).map(str::to_string)
            } else {
                None
            }
        })
}

pub(super) fn extract_model_name(response: &Value) -> Option<String> {
    response
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            response
                .get("response")
                .and_then(|response| response.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

pub(super) async fn build_streamed_markdown(
    mode: &str,
    sections: &[String],
    warning_count: usize,
    model: &str,
    event_sender: Option<&mpsc::Sender<FlowableDraftAssistantStreamEvent>>,
) -> String {
    let mut rendered_sections = Vec::with_capacity(sections.len());
    let mut accumulated_chars = 0usize;

    for section in sections.iter() {
        if let Some(sender) = event_sender {
            let chunks = chunk_markdown(section, 240);
            for chunk in chunks.iter() {
                accumulated_chars += chunk.chars().count();
                let _ = sender.try_send(FlowableDraftAssistantStreamEvent::TextDelta {
                    mode: mode.to_string(),
                    delta: chunk.clone(),
                    accumulated_chars,
                });
                tokio::task::yield_now().await;
            }
        }

        rendered_sections.push(section.clone());
    }

    if let Some(sender) = event_sender {
        let _ = sender.try_send(FlowableDraftAssistantStreamEvent::Completed {
            mode: mode.to_string(),
            warning_count,
            model: model.to_string(),
        });
    }

    rendered_sections.join("\n\n")
}

fn chunk_markdown(markdown: &str, chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in markdown.lines() {
        let candidate_len = current.len() + line.len() + 1;
        if !current.is_empty() && candidate_len > chunk_size {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() && !markdown.is_empty() {
        chunks.push(markdown.to_string());
    }
    chunks
}

pub(super) fn normalize_process_key(
    process_key: Option<&str>,
    process_name: Option<&str>,
    case_type_code: Option<&str>,
    filename: &str,
) -> String {
    let raw = process_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| case_type_code.map(str::trim).filter(|value| !value.is_empty()))
        .or_else(|| process_name.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(filename);
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized
        .trim_matches('_')
        .to_lowercase()
        .chars()
        .take(64)
        .collect::<String>()
        .trim_matches('_')
        .to_string()
        .chars()
        .collect::<String>()
        .if_empty_then("generated_process")
}

pub(super) fn humanize_process_name(process_key: &str) -> String {
    process_key
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .if_empty_then("Generated Process")
}

pub(super) fn extract_requirements(text: &str) -> Vec<String> {
    let mut items = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| line.chars().count() >= 6)
        .take(8)
        .map(|line| {
            line.trim_start_matches([
                '-', '*', '•', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '.', '、', ')', '(',
            ])
            .trim()
            .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    items.dedup();
    if items.is_empty() {
        items.push("根据上传文档完成流程建模".to_string());
    }
    items
}

pub(super) fn build_warnings(text: &str, requirements: &[String]) -> Vec<String> {
    let mut warnings = Vec::new();
    if requirements.len() <= 1 {
        warnings.push("文档中可提炼的结构化要求较少，建议人工补充节点与分支条件。".to_string());
    }
    if !text.contains("审批") && !text.contains("审核") {
        warnings.push("文档未明确审批节点，草案中仅生成主干流程。".to_string());
    }
    if !text.contains("异常") && !text.contains("回退") && !text.contains("超时") {
        warnings.push("文档未描述异常/超时处理，建议人工补充兜底分支。".to_string());
    }
    warnings
}

pub(super) fn build_bpmn_xml(process_key: &str, process_name: &str, requirements: &[String]) -> String {
    let selected_requirements = requirements.iter().take(4).collect::<Vec<_>>();
    let start_outgoing = if selected_requirements.is_empty() {
        "Flow_Start_End"
    } else {
        "Flow_Start_1"
    };
    let end_incoming = if selected_requirements.is_empty() {
        "Flow_Start_End"
    } else {
        "Flow_End_1"
    };

    let mut task_nodes = String::new();
    for (idx, requirement) in selected_requirements.iter().enumerate() {
        let task_id = format!("Task_{:02}", idx + 1);
        let incoming_flow = if idx == 0 {
            "Flow_Start_1".to_string()
        } else {
            format!("Flow_Task_{:02}", idx)
        };
        let outgoing_flow = if idx + 1 == selected_requirements.len() {
            "Flow_End_1".to_string()
        } else {
            format!("Flow_Task_{:02}", idx + 1)
        };
        task_nodes.push_str(&format!(
            r#"    <bpmn:userTask id="{task_id}" name="{name}">
      <bpmn:incoming>{incoming_flow}</bpmn:incoming>
      <bpmn:outgoing>{outgoing_flow}</bpmn:outgoing>
    </bpmn:userTask>
"#,
            task_id = task_id,
            name = xml_escape(requirement),
            incoming_flow = incoming_flow,
            outgoing_flow = outgoing_flow,
        ));
    }

    let mut sequence_flows = String::new();
    if selected_requirements.is_empty() {
        sequence_flows.push_str(
            r#"    <bpmn:sequenceFlow id="Flow_Start_End" sourceRef="StartEvent_1" targetRef="EndEvent_1" />
"#,
        );
    } else {
        for idx in 0..selected_requirements.len() {
            let task_id = format!("Task_{:02}", idx + 1);
            let flow_id = if idx == 0 {
                "Flow_Start_1".to_string()
            } else {
                format!("Flow_Task_{:02}", idx)
            };
            let source_id = if idx == 0 {
                "StartEvent_1".to_string()
            } else {
                format!("Task_{:02}", idx)
            };
            sequence_flows.push_str(&format!(
                r#"    <bpmn:sequenceFlow id="{flow_id}" sourceRef="{source_id}" targetRef="{task_id}" />
"#,
                flow_id = flow_id,
                source_id = source_id,
                task_id = task_id,
            ));
        }
        let last_task_id = format!("Task_{:02}", selected_requirements.len());
        sequence_flows.push_str(&format!(
            r#"    <bpmn:sequenceFlow id="Flow_End_1" sourceRef="{last_task_id}" targetRef="EndEvent_1" />
"#,
            last_task_id = last_task_id,
        ));
    }

    let mut shapes = String::from(
        r#"      <bpmndi:BPMNShape id="StartEvent_1_di" bpmnElement="StartEvent_1">
        <dc:Bounds x="152" y="142" width="36" height="36" />
      </bpmndi:BPMNShape>
"#,
    );
    for idx in 0..selected_requirements.len() {
        let task_id = format!("Task_{:02}", idx + 1);
        let x = 252 + (idx as i32 * 160);
        shapes.push_str(&format!(
            r#"      <bpmndi:BPMNShape id="{task_id}_di" bpmnElement="{task_id}">
        <dc:Bounds x="{x}" y="120" width="110" height="80" />
      </bpmndi:BPMNShape>
"#,
            task_id = task_id,
            x = x,
        ));
    }
    let end_x = 252 + (selected_requirements.len() as i32 * 160);
    shapes.push_str(&format!(
        r#"      <bpmndi:BPMNShape id="EndEvent_1_di" bpmnElement="EndEvent_1">
        <dc:Bounds x="{end_x}" y="142" width="36" height="36" />
      </bpmndi:BPMNShape>
"#,
        end_x = end_x,
    ));

    let mut edges = String::new();
    let flow_count = selected_requirements.len() + 1;
    for idx in 0..flow_count {
        let (flow_id, from_x, to_x) = if selected_requirements.is_empty() {
            ("Flow_Start_End".to_string(), 188, end_x)
        } else if idx == 0 {
            ("Flow_Start_1".to_string(), 188, 252)
        } else if idx == selected_requirements.len() {
            ("Flow_End_1".to_string(), 252 + ((idx - 1) as i32 * 160) + 110, end_x)
        } else {
            (
                format!("Flow_Task_{:02}", idx),
                252 + ((idx - 1) as i32 * 160) + 110,
                252 + (idx as i32 * 160),
            )
        };
        edges.push_str(&format!(
            r#"      <bpmndi:BPMNEdge id="{flow_id}_di" bpmnElement="{flow_id}">
        <di:waypoint x="{from_x}" y="160" />
        <di:waypoint x="{to_x}" y="160" />
      </bpmndi:BPMNEdge>
"#,
            flow_id = flow_id,
            from_x = from_x,
            to_x = to_x,
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL"
  xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI"
  xmlns:dc="http://www.omg.org/spec/DD/20100524/DC"
  xmlns:di="http://www.omg.org/spec/DD/20100524/DI"
  targetNamespace="http://flight-monitor-system/flowable">
  <bpmn:process id="{process_key}" name="{process_name}" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1" name="开始">
      <bpmn:outgoing>{start_outgoing}</bpmn:outgoing>
    </bpmn:startEvent>
{task_nodes}{sequence_flows}    <bpmn:endEvent id="EndEvent_1" name="结束">
      <bpmn:incoming>{end_incoming}</bpmn:incoming>
    </bpmn:endEvent>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="GeneratedBPMNDiagram">
    <bpmndi:BPMNPlane id="GeneratedBPMNPlane" bpmnElement="{process_key}">
{shapes}{edges}    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>
"#,
        process_key = xml_escape(process_key),
        process_name = xml_escape(process_name),
        start_outgoing = start_outgoing,
        task_nodes = task_nodes,
        sequence_flows = sequence_flows,
        end_incoming = end_incoming,
        shapes = shapes,
        edges = edges,
    )
}

pub(super) fn build_summary_markdown(
    process_key: &str,
    process_name: &str,
    case_type_code: Option<&str>,
    locale: &str,
    requirements: &[String],
    filename: &str,
    warnings: &[String],
) -> String {
    let requirement_lines = requirements
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let warning_lines = if warnings.is_empty() {
        "- 无".to_string()
    } else {
        warnings
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "# {process_name}\n\n\
         - Process Key: `{process_key}`\n\
         - Case Type: `{case_type}`\n\
         - Locale: `{locale}`\n\
         - Source File: `{filename}`\n\n\
         ## 提炼需求\n{requirements}\n\n\
         ## 风险与人工复核点\n{warnings}\n",
        process_name = process_name,
        process_key = process_key,
        case_type = case_type_code.unwrap_or("unknown"),
        locale = locale,
        filename = filename,
        requirements = requirement_lines,
        warnings = warning_lines,
    )
}

pub(super) fn build_general_sections(message: &str, user_id: &str) -> Vec<String> {
    vec![
        format!(
            "## 建议\n围绕 `{message}`，建议先把流程目标、责任角色和异常回退说清楚，再进入 BPMN 细化。"
        ),
        "## 推荐下一步\n1. 明确开始条件、结束条件，以及流程何时需要人工接管。\n2. 为每个任务节点补齐输入、输出、责任人和系统动作。\n3. 把超时、驳回、撤回这类分支单独建模，避免只写在备注里。"
            .to_string(),
        format!(
            "## 复核提示\n- 是否存在跨系统回写、消息通知或子流程调用。\n- 关键网关是否有明确判定条件。\n- 当前请求用户: `{user_id}`。"
        ),
    ]
}

pub(super) fn build_contextual_sections(
    message: &str,
    user_id: &str,
    context: Option<&crate::schemas::flowable_draft_schemas::FlowableDraftAssistantContext>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    if context.is_none() {
        warnings.push("未提供流程草案上下文，已降级为通用建议。".to_string());
        return build_general_sections(message, user_id);
    }
    let process_name = context
        .and_then(|ctx| ctx.process_name.as_deref())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("当前流程草案");
    let summary = context
        .and_then(|ctx| ctx.draft_summary_markdown.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("未提供草案摘要。");
    if context
        .and_then(|ctx| ctx.draft_summary_markdown.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        warnings.push("上下文缺少草案摘要，建议先生成流程草案后再提问。".to_string());
    }
    if context
        .and_then(|ctx| ctx.draft_bpmn_xml.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        warnings.push("上下文缺少 BPMN XML，回答将无法引用具体节点。".to_string());
    }
    vec![
        format!("## 建议\n围绕 `{process_name}`，当前问题 `{message}` 更像是在收敛节点边界和分支条件。"),
        format!(
            "## 结合现有草案可先检查\n- 当前摘要覆盖的主链路：\n{summary}\n- 与问题直接相关的节点，是否已经补齐责任人、输入变量和出口条件。\n- 异常、回退、超时场景是否已经建成显式分支。"
        ),
        format!(
            "## 建模建议\n- 需要复用已有流程时再考虑 `callActivity`，不要过早拆子流程。\n- 网关条件尽量落到明确字段或表达式，避免写成笼统说明。\n- 当前请求用户: `{user_id}`。"
        ),
    ]
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

trait StringExt {
    fn if_empty_then(self, fallback: &str) -> String;
}

impl StringExt for String {
    fn if_empty_then(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
