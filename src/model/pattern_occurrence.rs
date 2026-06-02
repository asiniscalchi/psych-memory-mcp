//! Domain models for `record_pattern_occurrence`.
//!
//! Epistemic stance: an occurrence is a *classification of one concrete dated
//! episode*, grounded in journal facts and tied to exactly one pattern seed. It
//! is never a fact, never the seed itself, and never a claim that the user
//! "has" the pattern. One occurrence does not activate a pattern.

use schemars::JsonSchema;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

/// Backend memory type stored for every occurrence.
pub const MEMORY_TYPE: &str = "pattern_occurrence";
/// Epistemic status: an evidence-linked episode classification.
pub const EPISTEMIC_STATUS: &str = "evidence_linked_occurrence";
/// Schema version stamped into occurrence metadata.
pub const SCHEMA_VERSION: &str = "psych-memory.pattern_occurrence.v1";

/// How the pattern category relates to the episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OccurrencePhase {
    Activated,
    RecognizedBeforeAction,
    RecognizedAfterAction,
    Inhibited,
    NotActivated,
    Transformed,
}

impl OccurrencePhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            OccurrencePhase::Activated => "activated",
            OccurrencePhase::RecognizedBeforeAction => "recognized_before_action",
            OccurrencePhase::RecognizedAfterAction => "recognized_after_action",
            OccurrencePhase::Inhibited => "inhibited",
            OccurrencePhase::NotActivated => "not_activated",
            OccurrencePhase::Transformed => "transformed",
        }
    }

    pub fn is_not_activated(&self) -> bool {
        matches!(self, OccurrencePhase::NotActivated)
    }
}

/// Input to the `record_pattern_occurrence` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RecordPatternOccurrenceInput {
    pub pattern_id: String,
    pub fact_ids: Vec<String>,
    #[serde(default)]
    pub interpretation_ids: Vec<String>,
    pub occurrence_date: String,
    pub phase: OccurrencePhase,
    pub summary: String,
    pub confidence: f32,
    #[serde(default)]
    pub intensity: Option<f32>,
}

/// A `RecordPatternOccurrenceInput` that passed shape/scalar validation.
/// `fact_ids` and `interpretation_ids` are canonicalised (sorted, de-duplicated)
/// so identity and tags do not depend on input order or repetition.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedPatternOccurrence {
    pub pattern_id: String,
    pub fact_ids: Vec<String>,
    pub interpretation_ids: Vec<String>,
    pub occurrence_date: String,
    pub phase: OccurrencePhase,
    pub summary: String,
    pub confidence: f32,
    pub intensity: Option<f32>,
}

/// Outcome of a `record_pattern_occurrence` call. Serializes to the shared
/// flat, `ok`-discriminated wire shape.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordPatternOccurrenceOutput {
    Stored {
        occurrence_id: String,
        backend_memory_id: Option<String>,
        status: String,
    },
    Rejected {
        error_code: String,
        message: String,
    },
}

impl Serialize for RecordPatternOccurrenceOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            RecordPatternOccurrenceOutput::Stored {
                occurrence_id,
                backend_memory_id,
                status,
            } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("occurrence_id", occurrence_id)?;
                map.serialize_entry("backend_memory_id", backend_memory_id)?;
                map.serialize_entry("status", status)?;
                map.end()
            }
            RecordPatternOccurrenceOutput::Rejected {
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
    fn phase_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(OccurrencePhase::RecognizedBeforeAction).unwrap(),
            json!("recognized_before_action")
        );
        assert!(OccurrencePhase::NotActivated.is_not_activated());
    }

    #[test]
    fn stored_wire_shape() {
        let out = RecordPatternOccurrenceOutput::Stored {
            occurrence_id: "occ_abc".into(),
            backend_memory_id: Some("backend_901".into()),
            status: "stored".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({"ok": true, "occurrence_id": "occ_abc", "backend_memory_id": "backend_901", "status": "stored"})
        );
    }

    #[test]
    fn rejected_wire_shape() {
        let out = RecordPatternOccurrenceOutput::Rejected {
            error_code: "unknown_pattern_seed".into(),
            message: "x".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({"ok": false, "error_code": "unknown_pattern_seed", "message": "x"})
        );
    }
}
