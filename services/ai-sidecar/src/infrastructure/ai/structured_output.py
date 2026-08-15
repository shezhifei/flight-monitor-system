from typing import Any

from pydantic import BaseModel, Field


class ReasoningStep(BaseModel):
    step: str
    summary: str


class OutputEvidence(BaseModel):
    object_type: str
    object_id: str
    field: str | None = None
    source: str
    as_of: str | None = None  # P1-1-A: ISO8601 timestamp when data was valid
    freshness_seconds: int | None = None  # P1-1-A: How fresh the data is


class OutputProposal(BaseModel):
    proposal_id: str | None = None
    object_type: str
    object_id: str
    action_name: str
    arguments: dict[str, Any] = Field(default_factory=dict)
    risk_level: str
    confidence: float = 0.0
    reasoning: str = ""
    requires_approval: bool = True


class OutputMetrics(BaseModel):
    model: str
    duration_ms: int


class TokenUsage(BaseModel):
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0


class AiStructuredOutput(BaseModel):
    contract_version: str = "ai-structured-output.v1"
    run_id: str = ""
    status: str = "completed"
    answer: str
    reasoning_steps: list[ReasoningStep] = Field(default_factory=list)
    evidence: list[OutputEvidence] = Field(default_factory=list)
    proposals: list[OutputProposal] = Field(default_factory=list)
    limitations: list[str] = Field(default_factory=list)
    metrics: OutputMetrics | None = None
    token_usage: TokenUsage | None = None
