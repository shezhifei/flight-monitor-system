from typing import Any

from pydantic import BaseModel, Field


class EnvelopeRequester(BaseModel):
    user_id: str
    roles: list[str] = Field(default_factory=list)
    permissions: list[str] = Field(default_factory=list)
    department_id: str | None = None
    permission_version: str | None = None


class EnvelopeOntology(BaseModel):
    version: str = "flight-ops.v1"
    allowed_object_types: list[str] = Field(default_factory=list)
    allowed_actions: list[str] = Field(default_factory=list)
    risk_ceiling: str = "medium"


class EnvelopeObject(BaseModel):
    object_type: str
    object_id: str
    version: int | None = None
    data: dict[str, Any] = Field(default_factory=dict)


class EnvelopeRelation(BaseModel):
    from_type: str
    from_id: str
    to_type: str
    to_id: str
    relation_type: str


class EnvelopeEvidence(BaseModel):
    source: str
    object_type: str
    object_id: str
    retrieved_at: str | None = None


class EnvelopeLimits(BaseModel):
    max_objects: int = 100
    max_tokens: int = 12000
    redaction: str = "standard"


class EnvelopeContext(BaseModel):
    objects: list[EnvelopeObject] = Field(default_factory=list)
    relations: list[EnvelopeRelation] = Field(default_factory=list)
    evidence: list[EnvelopeEvidence] = Field(default_factory=list)
    limits: EnvelopeLimits = Field(default_factory=EnvelopeLimits)


class EnvelopeTask(BaseModel):
    task_type: str
    user_message: str


class ContextEnvelope(BaseModel):
    contract_version: str = "ai-runtime.v1"
    job_id: str = ""
    run_id: str = ""
    correlation_id: str = ""
    requester: EnvelopeRequester
    ontology: EnvelopeOntology
    context: EnvelopeContext
    task: EnvelopeTask
    # Prior conversation turns (each a chat message dict: {"role", "content", ...}).
    # Canonical multi-turn history source: when non-empty, the runtime splices these
    # between the (freshly built) system prompt and the current user turn, which is
    # what activates budget-driven context compression. Any caller-supplied "system"
    # role entries are ignored — the system prompt is always rebuilt from config.
    conversation_history: list[dict[str, Any]] = Field(default_factory=list)
