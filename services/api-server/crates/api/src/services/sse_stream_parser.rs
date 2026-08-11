//! SSE Stream Parser - Parses Server-Sent Events from AI runtime
//!
//! Handles events like:
//! - `run.complete` - Terminal event indicating successful completion
//! - `run.fail` - Terminal event indicating failure
//! - `tool.call` - Tool invocation event (ignored by terminal extraction)
//! - `tool.result` - Tool result event (ignored by terminal extraction)
//! - `token` - Token delta event (ignored by terminal extraction)

use serde_json;

/// SSE event types from AI runtime stream
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEventType {
    RunComplete,
    RunFail,
    ToolCall,
    ToolResult,
    Token,
    Progress,
    TransportAbort,
    Unknown,
}

impl From<&str> for SseEventType {
    fn from(s: &str) -> Self {
        match s {
            "run.complete" => SseEventType::RunComplete,
            "run.fail" => SseEventType::RunFail,
            "tool.call" => SseEventType::ToolCall,
            "tool.result" => SseEventType::ToolResult,
            "token" => SseEventType::Token,
            "progress" => SseEventType::Progress,
            "transport.abort" => SseEventType::TransportAbort,
            _ => SseEventType::Unknown,
        }
    }
}

/// Parsed SSE event
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: SseEventType,
    pub event_name: String,
    pub data: String,
}

/// Result of terminal event extraction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalStatus {
    Completed,
    Failed(String),
    Pending,
}

/// Terminal output extracted from run.complete/run.fail events
#[derive(Debug, Clone)]
pub enum StreamTerminal {
    Succeeded(serde_json::Value),
    Failed(serde_json::Value),
}

impl StreamTerminal {
    pub fn is_success(&self) -> bool {
        matches!(self, StreamTerminal::Succeeded(_))
    }

    pub fn into_output(self) -> serde_json::Value {
        match self {
            StreamTerminal::Succeeded(v) => v,
            StreamTerminal::Failed(v) => v,
        }
    }
}

/// Parse raw SSE string into a list of SseEvent (free function)
pub fn parse_sse_stream(raw_sse: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    for chunk in raw_sse.split("\n\n") {
        if chunk.trim().is_empty() {
            continue;
        }
        let mut event_name = String::new();
        let mut data = String::new();
        for line in chunk.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(value) = line.strip_prefix("event:") {
                event_name = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("data:") {
                data = value.trim().to_string();
            }
        }
        if !event_name.is_empty() {
            let event_type = SseEventType::from(event_name.as_str());
            events.push(SseEvent {
                event_type,
                event_name,
                data,
            });
        }
    }
    events
}

/// Extract terminal event from parsed events (free function).
/// When multiple terminal events exist, the last one wins.
pub fn extract_terminal(events: &[SseEvent]) -> Option<StreamTerminal> {
    let mut last_terminal: Option<StreamTerminal> = None;
    for event in events {
        match event.event_type {
            SseEventType::RunComplete => {
                if let Ok(value) = serde_json::from_str(&event.data) {
                    last_terminal = Some(StreamTerminal::Succeeded(value));
                }
            }
            SseEventType::RunFail => {
                if let Ok(value) = serde_json::from_str(&event.data) {
                    last_terminal = Some(StreamTerminal::Failed(value));
                }
            }
            _ => continue,
        }
    }
    last_terminal
}

/// Check if a succeeded terminal should be treated as degraded
pub fn is_degraded_success(terminal: &StreamTerminal) -> bool {
    match terminal {
        StreamTerminal::Succeeded(value) => {
            value.get("degraded").and_then(|v| v.as_bool()).unwrap_or(false)
                || value
                    .get("limitations")
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
        }
        StreamTerminal::Failed(_) => false,
    }
}

/// Extract transport abort message from SSE buffer
pub fn extract_transport_abort_message(buffer: &str) -> Option<String> {
    let events = parse_sse_stream(buffer);
    for event in events {
        if event.event_type == SseEventType::TransportAbort {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&event.data) {
                return value.get("message").and_then(|v| v.as_str()).map(String::from);
            }
        }
    }
    None
}

/// SSE stream parser (struct-based wrapper)
#[derive(Debug)]
pub struct SseStreamParser {
    events: Vec<SseEvent>,
}

impl SseStreamParser {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Parse raw SSE data string into events
    pub fn parse(&mut self, raw_sse: &str) -> Vec<SseEvent> {
        let events = parse_sse_stream(raw_sse);
        self.events = events.clone();
        events
    }

    /// Extract terminal status - only looks for run.complete/run.fail
    /// Tool events are ignored
    pub fn extract_terminal(&self) -> Option<StreamTerminal> {
        extract_terminal(&self.events)
    }

    pub fn events(&self) -> &[SseEvent] {
        &self.events
    }

    pub fn event_counts(&self) -> EventCounts {
        let mut counts = EventCounts::default();
        for event in &self.events {
            match event.event_type {
                SseEventType::RunComplete => counts.run_complete += 1,
                SseEventType::RunFail => counts.run_fail += 1,
                SseEventType::ToolCall => counts.tool_call += 1,
                SseEventType::ToolResult => counts.tool_result += 1,
                SseEventType::Token => counts.token += 1,
                SseEventType::Progress => counts.progress += 1,
                SseEventType::TransportAbort => counts.transport_abort += 1,
                SseEventType::Unknown => counts.unknown += 1,
            }
        }
        counts
    }
}

impl Default for SseStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Event count summary
#[derive(Debug, Default)]
pub struct EventCounts {
    pub run_complete: usize,
    pub run_fail: usize,
    pub tool_call: usize,
    pub tool_result: usize,
    pub token: usize,
    pub progress: usize,
    pub transport_abort: usize,
    pub unknown: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_call_and_result_events() {
        let raw_sse = "event: tool.call\ndata: {\"tool\":\"fetch_data\",\"args\":{\"url\":\"https://api.example.com\"}}\n\n\
                       event: tool.result\ndata: {\"tool\":\"fetch_data\",\"result\":{\"status\":200,\"body\":\"OK\"}}\n\n\
                       event: run.complete\ndata: {\"run_id\":\"run_123\",\"status\":\"success\"}\n\n";

        let events = parse_sse_stream(raw_sse);

        assert_eq!(events.len(), 3, "Should parse exactly 3 events");

        assert_eq!(events[0].event_type, SseEventType::ToolCall);
        assert_eq!(events[0].event_name, "tool.call");
        assert!(events[0].data.contains("fetch_data"));

        assert_eq!(events[1].event_type, SseEventType::ToolResult);
        assert_eq!(events[1].event_name, "tool.result");
        assert!(events[1].data.contains("status\":200"));

        assert_eq!(events[2].event_type, SseEventType::RunComplete);
        assert_eq!(events[2].event_name, "run.complete");

        let terminal = extract_terminal(&events);
        assert!(terminal.is_some());
        assert!(terminal.unwrap().is_success());
    }

    #[test]
    fn test_sse_event_type_from_str() {
        assert_eq!(SseEventType::from("run.complete"), SseEventType::RunComplete);
        assert_eq!(SseEventType::from("run.fail"), SseEventType::RunFail);
        assert_eq!(SseEventType::from("tool.call"), SseEventType::ToolCall);
        assert_eq!(SseEventType::from("tool.result"), SseEventType::ToolResult);
        assert_eq!(SseEventType::from("token"), SseEventType::Token);
        assert_eq!(SseEventType::from("progress"), SseEventType::Progress);
        assert_eq!(SseEventType::from("transport.abort"), SseEventType::TransportAbort);
        assert_eq!(SseEventType::from("unknown.event"), SseEventType::Unknown);
    }

    #[test]
    fn test_terminal_extraction_with_failed_run() {
        let raw_sse = "event: tool.call\ndata: {\"tool\":\"test\"}\n\n\
                       event: tool.result\ndata: {\"result\":\"error\"}\n\n\
                       event: run.fail\ndata: {\"error\":\"Test failed\"}\n\n";

        let events = parse_sse_stream(raw_sse);
        let terminal = extract_terminal(&events);
        match terminal {
            Some(StreamTerminal::Failed(v)) => {
                let msg = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
                assert!(msg.contains("Test failed"));
            }
            _ => panic!("Expected Failed terminal"),
        }
    }

    #[test]
    fn test_terminal_extraction_pending() {
        let raw_sse = "event: tool.call\ndata: {\"tool\":\"test\"}\n\n";
        let events = parse_sse_stream(raw_sse);
        let terminal = extract_terminal(&events);
        assert!(terminal.is_none());
    }

    #[test]
    fn test_event_counts() {
        let raw_sse = "event: tool.call\ndata: {}\n\n\
                       event: tool.result\ndata: {}\n\n\
                       event: tool.call\ndata: {}\n\n\
                       event: run.complete\ndata: {}\n\n";

        let mut parser = SseStreamParser::new();
        parser.parse(raw_sse);

        let counts = parser.event_counts();
        assert_eq!(counts.tool_call, 2);
        assert_eq!(counts.tool_result, 1);
        assert_eq!(counts.run_complete, 1);
    }

    #[test]
    fn test_is_degraded_success() {
        let succeeded_with_limitations = serde_json::json!({
            "status": "succeeded",
            "limitations": ["LLM not configured"]
        });
        let terminal = StreamTerminal::Succeeded(succeeded_with_limitations);
        assert!(is_degraded_success(&terminal));

        let succeeded_clean = serde_json::json!({
            "status": "succeeded"
        });
        let terminal_clean = StreamTerminal::Succeeded(succeeded_clean);
        assert!(!is_degraded_success(&terminal_clean));

        let failed = serde_json::json!({"status": "failed"});
        let terminal_failed = StreamTerminal::Failed(failed);
        assert!(!is_degraded_success(&terminal_failed));
    }

    #[test]
    fn test_extract_transport_abort_message() {
        let buffer = "event: transport.abort\ndata: {\"message\":\"provider timeout\"}\n\n";
        let msg = extract_transport_abort_message(buffer);
        assert_eq!(msg, Some("provider timeout".to_string()));

        let no_abort = "event: token\ndata: {\"delta\":\"hello\"}\n\n";
        assert!(extract_transport_abort_message(no_abort).is_none());
    }

    // W0-1: Malformed / version-drift sidecar payload must never panic the process.
    // extract_terminal must silently skip unparseable JSON and return None.

    #[test]
    fn test_extract_terminal_malformed_run_complete_json_does_not_panic() {
        let raw_sse = "event: run.complete\ndata: {not valid json\n\n";
        let events = parse_sse_stream(raw_sse);
        let terminal = extract_terminal(&events);
        assert!(
            terminal.is_none(),
            "malformed run.complete JSON must yield no terminal, not panic"
        );
    }

    #[test]
    fn test_extract_terminal_malformed_run_fail_json_does_not_panic() {
        let raw_sse = "event: run.fail\ndata: {{{broken\n\n";
        let events = parse_sse_stream(raw_sse);
        let terminal = extract_terminal(&events);
        assert!(
            terminal.is_none(),
            "malformed run.fail JSON must yield no terminal, not panic"
        );
    }
}
