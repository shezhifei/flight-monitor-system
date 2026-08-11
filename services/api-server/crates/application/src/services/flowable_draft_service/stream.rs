#[derive(Debug, Clone)]
pub enum FlowableDraftAssistantStreamEvent {
    Progress {
        stage: String,
        message: String,
        mode: String,
    },
    Error {
        mode: String,
        message: String,
    },
    TextDelta {
        mode: String,
        delta: String,
        accumulated_chars: usize,
    },
    Completed {
        mode: String,
        warning_count: usize,
        model: String,
    },
}
