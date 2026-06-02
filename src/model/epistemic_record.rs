//! Domain models for `get_epistemic_record` (read-only, typed read-through).
//!
//! Records are *deserialized* from a backend memory's metadata into typed
//! structs (serde gives required-field / enum / shape validation for free), so
//! the returned payload is a validated typed record, never a raw JSON blob.

use schemars::JsonSchema;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

use crate::model::{FactType, InterpretationType, JournalEntryRef, OccurrencePhase};

/// Input to the `get_epistemic_record` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetEpistemicRecordInput {
    pub id: String,
}

/// The kind of epistemic record, as routed from the id prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpistemicRecordType {
    JournalFact,
    Interpretation,
    PatternSeed,
    PatternOccurrence,
}

impl EpistemicRecordType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EpistemicRecordType::JournalFact => "journal_fact",
            EpistemicRecordType::Interpretation => "interpretation",
            EpistemicRecordType::PatternSeed => "pattern_seed",
            EpistemicRecordType::PatternOccurrence => "pattern_occurrence",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JournalFactRecord {
    pub fact_id: String,
    pub source_excerpt: String,
    pub normalized_statement: String,
    pub fact_type: FactType,
    pub epistemic_status: String,
    pub journal_entry_ref: JournalEntryRef,
    #[serde(default)]
    pub event_date: Option<String>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterpretationRecord {
    pub interpretation_id: String,
    pub hypothesis: String,
    pub interpretation_type: InterpretationType,
    pub epistemic_status: String,
    pub status: String,
    pub supported_by_fact_ids: Vec<String>,
    #[serde(default)]
    pub contradicted_by_fact_ids: Vec<String>,
    pub confidence: f32,
    pub falsification_question: String,
    #[serde(default)]
    pub review_due: Option<String>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatternSeedRecord {
    pub pattern_id: String,
    /// May be absent in sparse metadata; a fallback is applied after parsing.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    pub description: String,
    #[serde(default)]
    pub markers: Vec<String>,
    #[serde(default)]
    pub counter_markers: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub epistemic_status: String,
    pub status: String,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatternOccurrenceRecord {
    pub occurrence_id: String,
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
    pub epistemic_status: String,
    pub schema_version: String,
}

/// A typed epistemic record. Serializes to the bare inner record (untagged), so
/// the `record` payload matches the stored metadata shape.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum EpistemicRecord {
    JournalFact(JournalFactRecord),
    Interpretation(InterpretationRecord),
    PatternSeed(PatternSeedRecord),
    PatternOccurrence(PatternOccurrenceRecord),
}

/// Outcome of a `get_epistemic_record` call.
///
/// The `Found` variant is larger than `Rejected`; this value is built once and
/// immediately serialized, so the size asymmetry is irrelevant.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum GetEpistemicRecordOutput {
    Found {
        record_type: EpistemicRecordType,
        id: String,
        backend_memory_id: Option<String>,
        record: EpistemicRecord,
    },
    Rejected {
        error_code: String,
        message: String,
    },
}

impl Serialize for GetEpistemicRecordOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            GetEpistemicRecordOutput::Found {
                record_type,
                id,
                backend_memory_id,
                record,
            } => {
                let mut map = serializer.serialize_map(Some(5))?;
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("record_type", record_type.as_str())?;
                map.serialize_entry("id", id)?;
                map.serialize_entry("backend_memory_id", backend_memory_id)?;
                map.serialize_entry("record", record)?;
                map.end()
            }
            GetEpistemicRecordOutput::Rejected {
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
