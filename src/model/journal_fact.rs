//! Domain models for `store_journal_fact`.
//!
//! Epistemic stance: every fact stored here is *journal-reported*. The system
//! never claims to have observed reality — only that the Froid journal reported
//! something. `fact_type` carries the finer distinction; `epistemic_status` is
//! always `journal_reported`.

use schemars::JsonSchema;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

/// Memory type stored in the backend for every journal fact.
pub const MEMORY_TYPE: &str = "fact";
/// Epistemic status stored for every journal fact (see module docs).
pub const EPISTEMIC_STATUS: &str = "journal_reported";
/// Schema version stamped into metadata so later migrations can detect shape.
pub const SCHEMA_VERSION: &str = "psych-memory.journal_fact.v1";

/// The kind of journal-reported fact claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactType {
    SelfReport,
    ReportedSpeech,
    ReportedEvent,
    ReportedBehavior,
}

impl FactType {
    /// The wire/tag spelling (`self_report`, ...).
    pub fn as_str(&self) -> &'static str {
        match self {
            FactType::SelfReport => "self_report",
            FactType::ReportedSpeech => "reported_speech",
            FactType::ReportedEvent => "reported_event",
            FactType::ReportedBehavior => "reported_behavior",
        }
    }
}

/// A reference to the Froid journal entry a fact was derived from.
///
/// Note: `source_excerpt` deliberately lives on [`StoreJournalFactInput`], not
/// here — the ref points at *where* the text is, the excerpt is the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct JournalEntryRef {
    pub entry_id: String,
    pub entry_date: String,
    #[serde(default)]
    pub char_start: Option<u32>,
    #[serde(default)]
    pub char_end: Option<u32>,
    #[serde(default)]
    pub content_hash: Option<String>,
}

/// Input to the `store_journal_fact` MCP tool.
///
/// `journal_entry_ref` is optional at the deserialization boundary on purpose:
/// a missing source must surface as a domain-level validation error
/// (`missing_journal_entry_ref`), not as a schema parse failure.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StoreJournalFactInput {
    /// Raw or near-raw text taken from the journal. The primary source anchor.
    pub source_excerpt: String,
    /// A normalized claim derived from the excerpt. Never used to derive
    /// `fact_id`.
    pub normalized_statement: String,
    pub fact_type: FactType,
    #[serde(default)]
    pub journal_entry_ref: Option<JournalEntryRef>,
    /// Optional date of the described event (`YYYY-MM-DD`). Null if unknown.
    #[serde(default)]
    pub event_date: Option<String>,
}

/// A `StoreJournalFactInput` that has passed validation: its
/// `journal_entry_ref` is guaranteed present and its dates are well-formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedJournalFact {
    pub source_excerpt: String,
    pub normalized_statement: String,
    pub fact_type: FactType,
    pub journal_entry_ref: JournalEntryRef,
    pub event_date: Option<String>,
}

/// Outcome of a `store_journal_fact` call.
///
/// Serializes to a flat, `ok`-discriminated wire shape:
/// `{ "ok": true, "fact_id", "backend_memory_id", "status" }` or
/// `{ "ok": false, "error_code", "message" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreJournalFactOutput {
    Stored {
        fact_id: String,
        backend_memory_id: Option<String>,
        status: String,
    },
    Rejected {
        error_code: String,
        message: String,
    },
}

impl Serialize for StoreJournalFactOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            StoreJournalFactOutput::Stored {
                fact_id,
                backend_memory_id,
                status,
            } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("fact_id", fact_id)?;
                map.serialize_entry("backend_memory_id", backend_memory_id)?;
                map.serialize_entry("status", status)?;
                map.end()
            }
            StoreJournalFactOutput::Rejected {
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
    fn fact_type_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(FactType::SelfReport).unwrap(),
            json!("self_report")
        );
        assert_eq!(FactType::ReportedBehavior.as_str(), "reported_behavior");
    }

    #[test]
    fn stored_output_wire_shape() {
        let out = StoreJournalFactOutput::Stored {
            fact_id: "fact_abc".into(),
            backend_memory_id: Some("backend_123".into()),
            status: "stored".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "ok": true,
                "fact_id": "fact_abc",
                "backend_memory_id": "backend_123",
                "status": "stored"
            })
        );
    }

    #[test]
    fn rejected_output_wire_shape() {
        let out = StoreJournalFactOutput::Rejected {
            error_code: "missing_journal_entry_ref".into(),
            message: "Journal fact requires a usable JournalEntryRef.".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "ok": false,
                "error_code": "missing_journal_entry_ref",
                "message": "Journal fact requires a usable JournalEntryRef."
            })
        );
    }
}
