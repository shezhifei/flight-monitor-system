use fms_domain::models::ai_context_envelope::*;
use fms_domain::models::ai_ontology::{OntologyActionDef, OntologySchema};
use fms_domain::models::ai_structured_output::*;
use serde_json::Value;

#[derive(Debug)]
pub enum ValidationError {
    SchemaInvalid(String),
    OntologyViolation(String),
    PermissionDenied(String),
    RiskExceeded(String),
    ObjectNotFound(String),
    ConstraintViolation(String),
}

pub struct AiOutputValidator {
    ontology_schema: OntologySchema,
}

impl AiOutputValidator {
    pub fn new(ontology_schema: OntologySchema) -> Self {
        Self { ontology_schema }
    }

    pub fn validate(
        &self,
        output: &AiStructuredOutput,
        envelope: &ContextEnvelope,
    ) -> Result<ValidatedOutput, Vec<ValidationError>> {
        let mut errors = Vec::new();

        if output.contract_version != "ai-structured-output.v1" {
            errors.push(ValidationError::SchemaInvalid(format!(
                "unsupported structured output contract_version: {}",
                output.contract_version
            )));
        }

        if output.run_id != envelope.run_id {
            errors.push(ValidationError::SchemaInvalid(format!(
                "output run_id {} does not match envelope run_id {}",
                output.run_id, envelope.run_id
            )));
        }

        if output.proposals.len() > 32 {
            errors.push(ValidationError::SchemaInvalid(format!(
                "too many proposals: {}",
                output.proposals.len()
            )));
        }

        for evidence in &output.evidence {
            if let Err(error) = self.validate_evidence(evidence, envelope) {
                errors.push(error);
            }
        }

        for proposal in &output.proposals {
            if let Err(error) = self.validate_proposal(proposal, envelope) {
                errors.push(error);
            }
        }

        if errors.is_empty() {
            Ok(ValidatedOutput {
                output: output.clone(),
                valid_proposals: output.proposals.clone(),
            })
        } else {
            Err(errors)
        }
    }

    fn validate_proposal(&self, proposal: &OutputProposal, envelope: &ContextEnvelope) -> Result<(), ValidationError> {
        let action_key = format!("{}.{}", proposal.object_type, proposal.action_name);

        if !envelope.ontology.allowed_object_types.contains(&proposal.object_type) {
            return Err(ValidationError::PermissionDenied(format!(
                "object type {} not allowed for this user",
                proposal.object_type
            )));
        }

        if !envelope.ontology.allowed_actions.contains(&action_key) {
            return Err(ValidationError::PermissionDenied(format!(
                "action {} not allowed for this user",
                action_key
            )));
        }

        let action_def = self
            .ontology_schema
            .objects
            .get(&proposal.object_type)
            .and_then(|obj| obj.actions.get(&proposal.action_name))
            .ok_or_else(|| {
                ValidationError::OntologyViolation(format!("action {} not found in ontology schema", action_key))
            })?;

        let proposal_risk = risk_level_order(&proposal.risk_level);
        let ceiling_risk = risk_level_order(&envelope.ontology.risk_ceiling);
        if proposal_risk > ceiling_risk {
            return Err(ValidationError::RiskExceeded(format!(
                "proposal risk {} exceeds ceiling {}",
                proposal.risk_level, envelope.ontology.risk_ceiling
            )));
        }

        if !(0.0..=1.0).contains(&proposal.confidence) {
            return Err(ValidationError::OntologyViolation(format!(
                "proposal confidence {} is outside 0..1",
                proposal.confidence
            )));
        }

        let object_present = envelope
            .context
            .objects
            .iter()
            .any(|object| object.object_type == proposal.object_type && object.object_id == proposal.object_id);
        if !object_present {
            return Err(ValidationError::ObjectNotFound(format!(
                "{} {} is not present in context envelope",
                proposal.object_type, proposal.object_id
            )));
        }

        for parameter in action_def.parameters.values().filter(|param| param.required) {
            if proposal.arguments.get(&parameter.name).is_none() {
                return Err(ValidationError::SchemaInvalid(format!(
                    "required argument {} is missing for {}",
                    parameter.name, action_key
                )));
            }
        }

        self.validate_action_arguments(action_def, proposal, &action_key)?;

        Ok(())
    }

    fn validate_evidence(&self, evidence: &OutputEvidence, envelope: &ContextEnvelope) -> Result<(), ValidationError> {
        let object_present = envelope
            .context
            .objects
            .iter()
            .any(|object| object.object_type == evidence.object_type && object.object_id == evidence.object_id);
        if !object_present {
            return Err(ValidationError::ObjectNotFound(format!(
                "evidence references {} {} outside context envelope",
                evidence.object_type, evidence.object_id
            )));
        }

        let source_present = envelope.context.evidence.iter().any(|allowed| {
            allowed.object_type == evidence.object_type
                && allowed.object_id == evidence.object_id
                && allowed.source == evidence.source
        });
        if !source_present {
            return Err(ValidationError::ConstraintViolation(format!(
                "evidence source {} is not present in context envelope for {} {}",
                evidence.source, evidence.object_type, evidence.object_id
            )));
        }

        Ok(())
    }

    fn validate_action_arguments(
        &self,
        action_def: &OntologyActionDef,
        proposal: &OutputProposal,
        action_key: &str,
    ) -> Result<(), ValidationError> {
        validate_schema_subset(&action_def.parameters_schema, &proposal.arguments, action_key)?;

        for parameter in action_def.parameters.values() {
            if let Some(value) = proposal.arguments.get(&parameter.name) {
                validate_parameter_type(&parameter.name, &parameter.param_type, value, action_key)?;
            }
        }

        Ok(())
    }
}

fn risk_level_order(level: &str) -> i32 {
    match level.to_lowercase().as_str() {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        "critical" => 3,
        _ => 0,
    }
}

fn validate_schema_subset(schema: &Value, value: &Value, action_key: &str) -> Result<(), ValidationError> {
    if let Some(schema_type) = schema.get("type").and_then(Value::as_str) {
        validate_json_type("arguments", schema_type, value, action_key)?;
    }

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for item in required {
            if let Some(name) = item.as_str() {
                if value.get(name).is_none() {
                    return Err(ValidationError::SchemaInvalid(format!(
                        "required argument {} is missing for {}",
                        name, action_key
                    )));
                }
            }
        }
    }

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, property_schema) in properties {
            if let Some(argument_value) = value.get(name) {
                if let Some(property_type) = property_schema.get("type").and_then(Value::as_str) {
                    validate_json_type(name, property_type, argument_value, action_key)?;
                }
            }
        }
    }

    Ok(())
}

fn validate_parameter_type(
    name: &str,
    parameter_type: &str,
    value: &Value,
    action_key: &str,
) -> Result<(), ValidationError> {
    let normalized = match parameter_type.trim().to_ascii_lowercase().as_str() {
        "string" | "str" | "text" => "string",
        "integer" | "int" | "i32" | "i64" | "u32" | "u64" => "integer",
        "number" | "float" | "f32" | "f64" | "decimal" => "number",
        "boolean" | "bool" => "boolean",
        "array" | "list" => "array",
        "object" | "json" => "object",
        _ => return Ok(()),
    };
    validate_json_type(name, normalized, value, action_key)
}

fn validate_json_type(name: &str, expected_type: &str, value: &Value, action_key: &str) -> Result<(), ValidationError> {
    let valid = match expected_type {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        _ => true,
    };

    if valid {
        Ok(())
    } else {
        Err(ValidationError::SchemaInvalid(format!(
            "argument {} for {} must be {}",
            name, action_key, expected_type
        )))
    }
}

#[derive(Debug)]
pub struct ValidatedOutput {
    pub output: AiStructuredOutput,
    pub valid_proposals: Vec<OutputProposal>,
}

#[cfg(test)]
mod tests {
    use super::{AiOutputValidator, ValidationError};
    use fms_domain::models::ai_context_envelope::{
        ContextEnvelope, EnvelopeContext, EnvelopeEvidence, EnvelopeLimits, EnvelopeObject, EnvelopeOntology,
        EnvelopeRequester, EnvelopeTask,
    };
    use fms_domain::models::ai_structured_output::{AiStructuredOutput, OutputEvidence, OutputProposal};
    use fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema;
    use serde_json::json;

    fn envelope() -> ContextEnvelope {
        ContextEnvelope {
            contract_version: "ai-runtime.v1".to_string(),
            job_id: "job_001".to_string(),
            run_id: "run_001".to_string(),
            correlation_id: "corr_001".to_string(),
            requester: EnvelopeRequester {
                user_id: "user_001".to_string(),
                roles: vec!["flight:write".to_string()],
                department_id: Some("ops-1".to_string()),
                permission_version: Some("7".to_string()),
            },
            ontology: EnvelopeOntology {
                version: "flight-ops.v1".to_string(),
                allowed_object_types: vec!["Flight".to_string()],
                allowed_actions: vec!["Flight.update_status".to_string()],
                risk_ceiling: "medium".to_string(),
            },
            context: EnvelopeContext {
                objects: vec![EnvelopeObject {
                    object_type: "Flight".to_string(),
                    object_id: "flt_001".to_string(),
                    version: Some(3),
                    data: json!({"flight_id": "flt_001", "stand": "S1"}),
                }],
                relations: vec![],
                evidence: vec![EnvelopeEvidence {
                    source: "ai_query.v_flights".to_string(),
                    object_type: "Flight".to_string(),
                    object_id: "flt_001".to_string(),
                    retrieved_at: Some("2026-06-02T00:00:00Z".to_string()),
                }],
                limits: EnvelopeLimits {
                    max_objects: 32,
                    max_tokens: 8000,
                    redaction: "standard".to_string(),
                },
            },
            task: EnvelopeTask {
                task_type: "nl_query".to_string(),
                user_message: "调整机位".to_string(),
            },
        }
    }

    fn output(proposals: Vec<OutputProposal>) -> AiStructuredOutput {
        AiStructuredOutput {
            contract_version: "ai-structured-output.v1".to_string(),
            run_id: "run_001".to_string(),
            status: "succeeded".to_string(),
            answer: "建议调整机位".to_string(),
            reasoning_steps: vec![],
            evidence: vec![OutputEvidence {
                object_type: "Flight".to_string(),
                object_id: "flt_001".to_string(),
                field: Some("stand".to_string()),
                source: "ai_query.v_flights".to_string(),
            }],
            proposals,
            limitations: vec![],
            metrics: None,
            token_usage: None,
        }
    }

    fn update_status_proposal(arguments: serde_json::Value) -> OutputProposal {
        OutputProposal {
            proposal_id: None,
            object_type: "Flight".to_string(),
            object_id: "flt_001".to_string(),
            action_name: "update_status".to_string(),
            arguments,
            risk_level: "medium".to_string(),
            confidence: 0.88,
            reasoning: "flight status change".to_string(),
            requires_approval: true,
        }
    }

    #[test]
    fn rejects_structured_output_with_wrong_contract_version() {
        let validator = AiOutputValidator::new(build_flight_ops_v1_schema());
        let mut body = output(vec![]);
        body.contract_version = "legacy".to_string();

        let errors = validator.validate(&body, &envelope()).unwrap_err();

        assert!(matches!(errors[0], ValidationError::SchemaInvalid(_)));
    }

    #[test]
    fn rejects_proposal_arguments_that_do_not_match_ontology_schema() {
        let validator = AiOutputValidator::new(build_flight_ops_v1_schema());
        let body = output(vec![update_status_proposal(json!({"reason": "missing status"}))]);

        let errors = validator.validate(&body, &envelope()).unwrap_err();

        assert!(errors
            .iter()
            .any(|err| matches!(err, ValidationError::SchemaInvalid(message) if message.contains("new_status"))));
    }

    #[test]
    fn rejects_proposal_for_object_not_present_in_context_envelope() {
        let validator = AiOutputValidator::new(build_flight_ops_v1_schema());
        let mut proposal = update_status_proposal(json!({"new_status": "delayed"}));
        proposal.object_id = "flt_missing".to_string();
        let body = output(vec![proposal]);

        let errors = validator.validate(&body, &envelope()).unwrap_err();

        assert!(errors
            .iter()
            .any(|err| matches!(err, ValidationError::ObjectNotFound(message) if message.contains("flt_missing"))));
    }

    #[test]
    fn rejects_proposal_argument_type_mismatch() {
        let validator = AiOutputValidator::new(build_flight_ops_v1_schema());
        let body = output(vec![update_status_proposal(json!({"new_status": 42}))]);

        let errors = validator.validate(&body, &envelope()).unwrap_err();

        assert!(errors.iter().any(
            |err| matches!(err, ValidationError::SchemaInvalid(message) if message.contains("new_status") && message.contains("string"))
        ));
    }

    #[test]
    fn rejects_output_evidence_source_not_present_in_context_envelope() {
        let validator = AiOutputValidator::new(build_flight_ops_v1_schema());
        let mut body = output(vec![update_status_proposal(json!({"new_status": "delayed"}))]);
        body.evidence[0].source = "sidecar.direct_sql".to_string();

        let errors = validator.validate(&body, &envelope()).unwrap_err();

        assert!(errors.iter().any(
            |err| matches!(err, ValidationError::ConstraintViolation(message) if message.contains("sidecar.direct_sql"))
        ));
    }
}
