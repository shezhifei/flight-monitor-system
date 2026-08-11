use calamine::{Reader as CalamineReader, Xlsx};
use encoding_rs::GBK;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use roxmltree::Document as XmlDocument;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

use super::error::FlowableDraftServiceError;
use crate::schemas::flowable_draft_schemas::ProcessDraftSourceMeta;

pub(super) const MAX_FILE_SIZE_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_PDF_PAGES: usize = 30;
pub(super) const MAX_DOCX_CHARACTERS: usize = 200_000;
pub(super) const MAX_XLSX_SHEETS: usize = 10;
pub(super) const MAX_XLSX_ROWS: usize = 200;
pub(super) const MAX_XLSX_COLS: usize = 20;

#[derive(Debug, Clone)]
pub(super) struct ParsedProcessDocument {
    pub(super) text: String,
    pub(super) warnings: Vec<String>,
    pub(super) source_meta: ProcessDraftSourceMeta,
}

pub(super) fn parse_process_document(
    filename: &str,
    file_bytes: &[u8],
) -> Result<ParsedProcessDocument, FlowableDraftServiceError> {
    let safe_filename = filename.trim().to_string();
    let effective_filename = if safe_filename.is_empty() {
        "uploaded_file"
    } else {
        safe_filename.as_str()
    };
    let extension = Path::new(effective_filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default();

    match extension.as_str() {
        ".pdf" | ".docx" | ".xlsx" | ".txt" | ".md" => {}
        _ => {
            return Err(FlowableDraftServiceError::ProcessDocument {
                status_code: 415,
                code: "UNSUPPORTED_FILE_TYPE".to_string(),
                message: format!(
                    "不支持的文件类型: {}",
                    if extension.is_empty() { "(none)" } else { &extension }
                ),
            })
        }
    }

    let file_size = file_bytes.len();
    if file_size == 0 {
        return Err(FlowableDraftServiceError::ProcessDocument {
            status_code: 422,
            code: "EMPTY_FILE".to_string(),
            message: "上传文件为空".to_string(),
        });
    }
    if file_size > MAX_FILE_SIZE_BYTES {
        return Err(FlowableDraftServiceError::ProcessDocument {
            status_code: 413,
            code: "FILE_TOO_LARGE".to_string(),
            message: format!("文件大小超过限制（上限 {}MB）", MAX_FILE_SIZE_BYTES / (1024 * 1024)),
        });
    }

    let (source_text, warnings) = match extension.as_str() {
        ".txt" | ".md" => (decode_text(file_bytes)?, Vec::new()),
        ".docx" => parse_docx_content(file_bytes)?,
        ".xlsx" => parse_xlsx_content(file_bytes)?,
        ".pdf" => (parse_pdf_content(file_bytes)?, Vec::new()),
        _ => unreachable!(),
    };

    let normalized = source_text.replace('\u{0}', "").trim().to_string();
    if normalized.is_empty() {
        return Err(FlowableDraftServiceError::ProcessDocument {
            status_code: 422,
            code: "DOCUMENT_TEXT_EMPTY".to_string(),
            message: "文档中未解析到可用文本".to_string(),
        });
    }

    Ok(ParsedProcessDocument {
        text: normalized.clone(),
        warnings,
        source_meta: ProcessDraftSourceMeta {
            filename: effective_filename.to_string(),
            extension,
            parsed_characters: normalized.chars().count(),
        },
    })
}

fn decode_text(file_bytes: &[u8]) -> Result<String, FlowableDraftServiceError> {
    if let Ok(value) = std::str::from_utf8(file_bytes) {
        return Ok(value.to_string());
    }

    let (decoded, _, had_errors) = GBK.decode(file_bytes);
    if !had_errors {
        return Ok(decoded.into_owned());
    }

    Err(FlowableDraftServiceError::ProcessDocument {
        status_code: 422,
        code: "TEXT_DECODE_FAILED".to_string(),
        message: "文本文件编码解析失败（仅支持 UTF-8/GBK）".to_string(),
    })
}

fn parse_docx_content(file_bytes: &[u8]) -> Result<(String, Vec<String>), FlowableDraftServiceError> {
    let cursor = Cursor::new(file_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|error| FlowableDraftServiceError::ProcessDocument {
        status_code: 422,
        code: "DOCX_PARSE_FAILED".to_string(),
        message: format!("DOCX 解析失败: {error}"),
    })?;

    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|error| FlowableDraftServiceError::ProcessDocument {
            status_code: 422,
            code: "DOCX_PARSE_FAILED".to_string(),
            message: format!("DOCX 解析失败: {error}"),
        })?
        .read_to_string(&mut document_xml)
        .map_err(|error| FlowableDraftServiceError::ProcessDocument {
            status_code: 422,
            code: "DOCX_PARSE_FAILED".to_string(),
            message: format!("DOCX 解析失败: {error}"),
        })?;

    let mut reader = XmlReader::from_str(&document_xml);
    // Do not trim_text: spaces around GeneralRef entities would be lost.
    let mut buf = Vec::new();
    let mut fragments: Vec<String> = Vec::new();
    let mut current = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(text)) => {
                // Text nodes are raw (may still contain entities if not split).
                let value = text
                    .decode()
                    .ok()
                    .map(|decoded| match unescape(decoded.as_ref()) {
                        Ok(unescaped) => unescaped.into_owned(),
                        Err(_) => decoded.into_owned(),
                    })
                    .unwrap_or_default();
                if !value.is_empty() {
                    current.push_str(&value);
                }
            }
            Ok(Event::GeneralRef(entity_ref)) => {
                // quick-xml 0.41 emits entity references as separate events.
                if let Ok(Some(ch)) = entity_ref.resolve_char_ref() {
                    current.push(ch);
                } else if let Ok(name) = entity_ref.decode() {
                    let raw = format!("&{name};");
                    match unescape(&raw) {
                        Ok(unescaped) => current.push_str(&unescaped),
                        Err(_) => current.push_str(&raw),
                    }
                }
            }
            Ok(Event::End(end)) => match end.local_name().as_ref() {
                b"t" => {}
                b"tc" => {
                    if !current.ends_with(" | ") && !current.is_empty() {
                        current.push_str(" | ");
                    }
                }
                b"p" | b"tr" => {
                    let normalized = current.trim().trim_end_matches('|').trim().to_string();
                    if !normalized.is_empty() {
                        fragments.push(normalized);
                    }
                    current.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(FlowableDraftServiceError::ProcessDocument {
                    status_code: 422,
                    code: "DOCX_PARSE_FAILED".to_string(),
                    message: format!("DOCX 解析失败: {error}"),
                })
            }
            _ => {}
        }
        buf.clear();
    }

    let mut combined = fragments.join("\n").trim().to_string();
    let mut warnings = Vec::new();
    if combined.len() > MAX_DOCX_CHARACTERS {
        warnings.push(format!("DOCX 文本已超过 {} 字符，已截断到上限", MAX_DOCX_CHARACTERS));
        combined = combined.chars().take(MAX_DOCX_CHARACTERS).collect();
    }

    Ok((combined, warnings))
}

fn parse_xlsx_content(file_bytes: &[u8]) -> Result<(String, Vec<String>), FlowableDraftServiceError> {
    let cursor = Cursor::new(file_bytes.to_vec());
    let mut workbook: Xlsx<_> = Xlsx::new(cursor).map_err(|error| FlowableDraftServiceError::ProcessDocument {
        status_code: 422,
        code: "XLSX_PARSE_FAILED".to_string(),
        message: format!("XLSX 解析失败: {error}"),
    })?;

    let mut warnings = Vec::new();
    let mut lines = Vec::new();
    let mut sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.len() > MAX_XLSX_SHEETS {
        warnings.push(format!(
            "XLSX 工作表超过 {} 个，仅解析前 {} 个",
            MAX_XLSX_SHEETS, MAX_XLSX_SHEETS
        ));
        sheet_names.truncate(MAX_XLSX_SHEETS);
    }

    for sheet_name in sheet_names {
        let range =
            workbook
                .worksheet_range(&sheet_name)
                .map_err(|error| FlowableDraftServiceError::ProcessDocument {
                    status_code: 422,
                    code: "XLSX_PARSE_FAILED".to_string(),
                    message: format!("XLSX 解析失败: {error}"),
                })?;
        let (row_count, col_count) = range.get_size();
        lines.push(format!("[Sheet] {sheet_name}"));
        if row_count > MAX_XLSX_ROWS || col_count > MAX_XLSX_COLS {
            warnings.push(format!(
                "Sheet {} 超出 {}x{} 限制，仅解析上限范围",
                sheet_name, MAX_XLSX_ROWS, MAX_XLSX_COLS
            ));
        }

        for row in range.rows().take(MAX_XLSX_ROWS) {
            let values = row
                .iter()
                .take(MAX_XLSX_COLS)
                .map(|cell| cell.to_string())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            if !values.is_empty() {
                lines.push(values.join(" | "));
            }
        }
    }

    Ok((lines.join("\n"), warnings))
}

fn parse_pdf_content(file_bytes: &[u8]) -> Result<String, FlowableDraftServiceError> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(file_bytes).map_err(|error| {
        FlowableDraftServiceError::ProcessDocument {
            status_code: 422,
            code: "PDF_PARSE_FAILED".to_string(),
            message: format!("PDF 解析失败: {error}"),
        }
    })?;

    if pages.len() > MAX_PDF_PAGES {
        return Err(FlowableDraftServiceError::ProcessDocument {
            status_code: 413,
            code: "PDF_PAGE_LIMIT_EXCEEDED".to_string(),
            message: format!("PDF 页数超过限制（上限 {} 页）", MAX_PDF_PAGES),
        });
    }

    let fragments = pages
        .into_iter()
        .map(|page| page.trim().to_string())
        .filter(|page| !page.is_empty())
        .collect::<Vec<_>>();
    if fragments.is_empty() {
        return Err(FlowableDraftServiceError::ProcessDocument {
            status_code: 422,
            code: "PDF_TEXT_NOT_FOUND".to_string(),
            message: "PDF 未检测到文本层（扫描件 OCR 暂不支持）".to_string(),
        });
    }

    Ok(fragments.join("\n"))
}
