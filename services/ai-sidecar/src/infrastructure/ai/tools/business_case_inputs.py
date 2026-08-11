"""Infrastructure-side validation models for business-case AI tools."""

from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field, constr


class BusinessCaseCreateInput(BaseModel):
    case_type: constr(min_length=1, max_length=50) = Field(...)
    flight_id: constr(min_length=1, max_length=36) = Field(...)
    description: constr(min_length=1, max_length=500) = Field(...)
    context: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime | None = Field(None)
