//! Domain models for `store_interpretation`.
//!
//! Epistemic stance: an interpretation is a *hypothesis* derived from journal
//! facts, never a fact itself. It must be grounded in at least one existing
//! `fact_id`, must be falsifiable, and stays revisable. `epistemic_status` is
//! always `hypothesis`; Story 2 only accepts `status = candidate`.

use schemars::JsonSchema;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

/// Backend memory type stored for every interpretation.
pub const MEMORY_TYPE: &str = "interpretation";
/// Epistemic status stored for every interpretation.
pub const EPISTEMIC_STATUS: &str = "hypothesis";
/// Schema version stamped into interpretation metadata.
pub const SCHEMA_VERSION: &str = "psych-memory.interpretation.v1";

/// Above this confidence, more supporting facts are required (see below).
pub const HIGH_CONFIDENCE_THRESHOLD: f32 = 0.7;
/// Minimum supporting facts required once confidence exceeds the threshold.
///
/// Conservative product policy for this early system, not a universal law: a
/// later review workflow may promote high-confidence interpretations backed by
/// fewer but stronger facts.
pub const MIN_FACTS_FOR_HIGH_CONFIDENCE: usize = 3;

/// The kind of interpretation hypothesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterpretationType {
    PsychologicalHypothesis,
    PatternHypothesis,
    AlternativeExplanation,
    ProgressHypothesis,
    ContradictionHypothesis,
}

impl InterpretationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InterpretationType::PsychologicalHypothesis => "psychological_hypothesis",
            InterpretationType::PatternHypothesis => "pattern_hypothesis",
            InterpretationType::AlternativeExplanation => "alternative_explanation",
            InterpretationType::ProgressHypothesis => "progress_hypothesis",
            InterpretationType::ContradictionHypothesis => "contradiction_hypothesis",
        }
    }
}

/// Lifecycle status of an interpretation. Story 2 only accepts `Candidate`;
/// the others require review/update workflows that do not exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterpretationStatus {
    Candidate,
    Accepted,
    Weakened,
    Superseded,
    Retired,
}

impl InterpretationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InterpretationStatus::Candidate => "candidate",
            InterpretationStatus::Accepted => "accepted",
            InterpretationStatus::Weakened => "weakened",
            InterpretationStatus::Superseded => "superseded",
            InterpretationStatus::Retired => "retired",
        }
    }
}

/// Input to the `store_interpretation` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StoreInterpretationInput {
    pub hypothesis: String,
    pub interpretation_type: InterpretationType,
    pub supported_by_fact_ids: Vec<String>,
    #[serde(default)]
    pub contradicted_by_fact_ids: Vec<String>,
    pub confidence: f32,
    /// Optional; omitted/null means `candidate`. Any explicit non-candidate
    /// value is rejected in Story 2.
    #[serde(default)]
    pub status: Option<InterpretationStatus>,
    pub falsification_question: String,
    /// Optional review date (`YYYY-MM-DD`).
    #[serde(default)]
    pub review_due: Option<String>,
}

/// A `StoreInterpretationInput` that has passed shape/scalar validation.
///
/// `supported_by_fact_ids` is canonicalised (sorted, de-duplicated) so identity
/// and tags do not depend on input order or repetition. Status is implicitly
/// `candidate` for everything reaching this stage.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedInterpretation {
    pub hypothesis: String,
    pub interpretation_type: InterpretationType,
    pub supported_by_fact_ids: Vec<String>,
    pub contradicted_by_fact_ids: Vec<String>,
    pub confidence: f32,
    pub falsification_question: String,
    pub review_due: Option<String>,
}

/// Outcome of a `store_interpretation` call. Serializes to the same flat,
/// `ok`-discriminated wire shape as Story 1's fact output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreInterpretationOutput {
    Stored {
        interpretation_id: String,
        backend_memory_id: Option<String>,
        status: String,
    },
    Rejected {
        error_code: String,
        message: String,
    },
}

impl Serialize for StoreInterpretationOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            StoreInterpretationOutput::Stored {
                interpretation_id,
                backend_memory_id,
                status,
            } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("interpretation_id", interpretation_id)?;
                map.serialize_entry("backend_memory_id", backend_memory_id)?;
                map.serialize_entry("status", status)?;
                map.end()
            }
            StoreInterpretationOutput::Rejected {
                error_code,
                message,
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("ok", &false)?;
                map.serialize_entry("error_code", error_code)?;
                map.serialize_entry("message", message)?;
                map.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn type_and_status_serialize_snake_case() {
        assert_eq!(
            serde_json::to_value(InterpretationType::PsychologicalHypothesis).unwrap(),
            json!("psychological_hypothesis")
        );
        assert_eq!(InterpretationStatus::Candidate.as_str(), "candidate");
    }

    #[test]
    fn stored_output_wire_shape() {
        let out = StoreInterpretationOutput::Stored {
            interpretation_id: "interp_abc".into(),
            backend_memory_id: Some("backend_456".into()),
            status: "stored".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "ok": true,
                "interpretation_id": "interp_abc",
                "backend_memory_id": "backend_456",
                "status": "stored"
            })
        );
    }

    #[test]
    fn rejected_output_wire_shape() {
        let out = StoreInterpretationOutput::Rejected {
            error_code: "missing_supporting_facts".into(),
            message: "Interpretation requires at least one supporting fact_id.".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "ok": false,
                "error_code": "missing_supporting_facts",
                "message": "Interpretation requires at least one supporting fact_id."
            })
        );
    }
}
