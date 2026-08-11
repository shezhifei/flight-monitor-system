use super::assistant::{build_bpmn_xml, build_contextual_sections, validate_bpmn_xml};
use super::document_parse::ParsedProcessDocument;
use super::{FlowableDraftService, FlowableDraftServiceError};
use crate::schemas::flowable_draft_schemas::{FlowableDraftAssistantChatRequest, ProcessDraftSourceMeta};
use serde_json::json;

#[tokio::test]
async fn generate_from_file_returns_empty_file_contract() {
    let service = FlowableDraftService::new();
    let error = service
        .generate_from_file("demo.txt", b"", None, None, None, None)
        .await
        .expect_err("empty file should fail");

    match error {
        FlowableDraftServiceError::ProcessDocument {
            status_code,
            code,
            message,
        } => {
            assert_eq!(status_code, 422);
            assert_eq!(code, "EMPTY_FILE");
            assert_eq!(message, "上传文件为空");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn generate_from_file_returns_unsupported_type_contract() {
    let service = FlowableDraftService::new();
    let error = service
        .generate_from_file("demo.csv", b"a,b", None, None, None, None)
        .await
        .expect_err("unsupported type should fail");

    match error {
        FlowableDraftServiceError::ProcessDocument {
            status_code,
            code,
            message,
        } => {
            assert_eq!(status_code, 415);
            assert_eq!(code, "UNSUPPORTED_FILE_TYPE");
            assert!(message.contains(".csv"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn generate_from_file_returns_ai_unavailable_when_synthetic_disabled() {
    std::env::set_var("FLOWABLE_DRAFT_SYNTHETIC_AI", "false");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("AZURE_OPENAI_API_KEY");
    std::env::remove_var("ANTHROPIC_API_KEY");

    let service = FlowableDraftService::new();
    let error = service
        .generate_from_file("demo.txt", b"boarding process", None, None, None, None)
        .await
        .expect_err("ai unavailable should fail");

    std::env::remove_var("FLOWABLE_DRAFT_SYNTHETIC_AI");

    match error {
        FlowableDraftServiceError::AIUnavailable(message) => {
            assert_eq!(message, "AI 不可用：未检测到有效 AI 配置");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn invalid_bpmn_draft_code_matches_python_contract() {
    let error = FlowableDraftServiceError::BpmnDraftValidation {
        code: "INVALID_BPMN_DRAFT".to_string(),
        message: "invalid bpmn".to_string(),
    };

    match error {
        FlowableDraftServiceError::BpmnDraftValidation { code, message } => {
            assert_eq!(code, "INVALID_BPMN_DRAFT");
            assert_eq!(message, "invalid bpmn");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn assistant_answer_always_appends_review_note() {
    let service = FlowableDraftService::new().with_synthetic_ai_fallback(true);
    let result = service
        .chat_assistant(
            FlowableDraftAssistantChatRequest {
                message: "帮我审查草案".to_string(),
                mode: "general".to_string(),
                request_id: None,
                context: None,
            },
            "user_001",
        )
        .await
        .expect("assistant result");

    assert!(result
        .answer_markdown
        .contains("提示：AI 输出仅供参考，部署前请务必人工检阅。"));
}

#[test]
fn normalize_ai_draft_output_supports_python_alias_keys() {
    let service = FlowableDraftService::new();
    let output = service.normalize_ai_draft_output(
        &json!({
            "bpmn_xml": "<definitions><process/><startEvent/><endEvent/></definitions>",
            "summary_markdown": "# Summary",
            "warnings": ["w1"]
        }),
        "model-x",
        &["fallback".to_string()],
    );

    assert_eq!(output.draft_summary_markdown, "# Summary");
    assert!(output.draft_bpmn_xml.contains("definitions"));
    assert_eq!(output.warnings, vec!["w1".to_string()]);
}

#[test]
fn contextual_mode_without_context_downgrades_and_warns_like_python() {
    let mut warnings = Vec::new();
    let sections = build_contextual_sections("帮我审查", "user_001", None, &mut warnings);
    assert!(!sections.is_empty());
    assert!(warnings.contains(&"未提供流程草案上下文，已降级为通用建议。".to_string()));
}

#[test]
fn generation_instructions_include_python_subprocess_guidance() {
    let service = FlowableDraftService::new();
    let prompt = service.build_generation_instructions(
        Some("proc_key"),
        Some("Proc Name"),
        Some("case_type"),
        "zh-CN",
        &ParsedProcessDocument {
            text: "审批流程说明".to_string(),
            warnings: vec![],
            source_meta: ProcessDraftSourceMeta {
                filename: "demo.txt".to_string(),
                extension: ".txt".to_string(),
                parsed_characters: 6,
            },
        },
        &["需要子流程".to_string()],
    );

    assert!(prompt.contains("callActivity"));
    assert!(prompt.contains("exclusiveGateway"));
    assert!(prompt.contains("conditionExpression"));
}

#[test]
fn corrected_requirements_merge_like_python() {
    let service = FlowableDraftService::new();
    let mut original = service.normalize_ai_draft_output(
        &json!({
            "draft_bpmn_xml": "<definitions><process/><startEvent/><endEvent/></definitions>",
            "draft_summary_markdown": "old summary",
            "extracted_requirements": ["r1"],
            "warnings": ["w1"]
        }),
        "model-a",
        &[],
    );
    let corrected = service.normalize_ai_draft_output(
        &json!({
            "draft_bpmn_xml": "<definitions><process/><startEvent/><endEvent/></definitions>",
            "summary_markdown": "",
            "extracted_requirements": ["r2"],
            "warnings": ["w2"]
        }),
        "model-b",
        &[],
    );

    let mut merged_requirements = original.extracted_requirements.clone();
    merged_requirements.extend(corrected.extracted_requirements.clone());
    merged_requirements.sort();
    merged_requirements.dedup();
    original.extracted_requirements = merged_requirements;
    original.warnings.extend(corrected.warnings.clone());
    original.warnings.sort();
    original.warnings.dedup();
    if corrected.draft_summary_markdown.trim().is_empty() {
        assert_eq!(original.draft_summary_markdown, "old summary");
    }
    assert_eq!(
        original.extracted_requirements,
        vec!["r1".to_string(), "r2".to_string()]
    );
    assert_eq!(original.warnings, vec!["w1".to_string(), "w2".to_string()]);
}

#[test]
fn generated_bpmn_contains_display_diagram_for_modeler_import() {
    let xml = build_bpmn_xml(
        "flight_delay_notice",
        "航班延误通知",
        &["通知保障部门".to_string(), "确认旅客通知结果".to_string()],
    );

    assert!(xml.contains("<bpmndi:BPMNDiagram"));
    assert!(xml.contains("<bpmndi:BPMNPlane"));
    assert!(xml.contains("bpmnElement=\"flight_delay_notice\""));
    assert!(xml.contains("bpmnElement=\"StartEvent_1\""));
    assert!(xml.contains("bpmnElement=\"Task_01\""));
    assert!(xml.contains("bpmnElement=\"EndEvent_1\""));
    assert!(validate_bpmn_xml(&xml).is_none());
}

#[test]
fn validation_rejects_bpmn_without_display_diagram() {
    let error = validate_bpmn_xml(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <bpmn:process id="missing_di" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1" />
    <bpmn:endEvent id="EndEvent_1" />
  </bpmn:process>
</bpmn:definitions>"#,
    );

    assert_eq!(error.as_deref(), Some("BPMN 缺少 BPMNDiagram/BPMNPlane 图形信息"));
}

#[test]
fn parse_process_document_rejects_oversized_payload() {
    use super::document_parse::{parse_process_document, MAX_FILE_SIZE_BYTES};

    let oversized = vec![0u8; MAX_FILE_SIZE_BYTES + 1];
    let error = parse_process_document("big.txt", &oversized).expect_err("oversized must fail");
    match error {
        FlowableDraftServiceError::ProcessDocument { status_code, code, .. } => {
            assert_eq!(status_code, 413);
            assert_eq!(code, "FILE_TOO_LARGE");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn parse_process_document_rejects_malformed_xml_docx_shell() {
    use super::document_parse::parse_process_document;

    let error = parse_process_document("bad.docx", b"not-a-zip").expect_err("invalid docx must fail");
    match error {
        FlowableDraftServiceError::ProcessDocument { status_code, code, .. } => {
            assert_eq!(status_code, 422);
            assert!(code.contains("DOCX") || code == "DOCX_PARSE_FAILED");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn parse_process_document_rejects_malformed_pdf() {
    use super::document_parse::parse_process_document;

    let error = parse_process_document("bad.pdf", b"%PDF-1.4 not-a-real-pdf").expect_err("invalid pdf must fail");
    match error {
        FlowableDraftServiceError::ProcessDocument { status_code, code, .. } => {
            assert_eq!(status_code, 422);
            assert_eq!(code, "PDF_PARSE_FAILED");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn parse_docx_unescapes_xml_entities() {
    use super::document_parse::parse_process_document;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    // Minimal DOCX: ZIP with word/document.xml containing &amp; entity.
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", opts).expect("start");
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Alpha &amp; Beta &lt;Gamma&gt;</w:t></w:r></w:p>
  </w:body>
</w:document>"#,
        )
        .expect("write xml");
        zip.finish().expect("finish");
    }

    let parsed = parse_process_document("entities.docx", &buf).expect("docx parse");
    assert!(
        parsed.text.contains("Alpha & Beta <Gamma>"),
        "expected unescaped entities, got: {:?}",
        parsed.text
    );
    assert!(
        !parsed.text.contains("&amp;"),
        "raw entity must not remain: {:?}",
        parsed.text
    );
}
