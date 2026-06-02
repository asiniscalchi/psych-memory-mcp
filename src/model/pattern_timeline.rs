//! Domain models for `query_pattern_timeline` (read-only).
//!
//! This tool answers "what occurrences are recorded for this pattern, and
//! when?" — descriptive data only. It never computes trend, never claims the
//! pattern is active, and never generates interpretation or advice.

use schemars::JsonSchema;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

use crate::model::OccurrencePhase;

/// Input to the `query_pattern_timeline` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QueryPatternTimelineInput {
    pub pattern_id: String,
    /// Inclusive lower bound on occurrence_date (`YYYY-MM-DD`).
    #[serde(default)]
    pub date_from: Option<String>,
    /// Inclusive upper bound on occurrence_date (`YYYY-MM-DD`).
    #[serde(default)]
    pub date_to: Option<String>,
    /// Phases to include; empty means all phases.
    #[serde(default)]
    pub phases: Vec<OccurrencePhase>,
    /// Whether to report invalid/corrupt records as warnings. Defaults to true.
    #[serde(default)]
    pub include_invalid_record_warnings: Option<bool>,
}

/// A `QueryPatternTimelineInput` that passed validation.
#[derive(Debug, Clone)]
pub struct ValidatedPatternTimelineQuery {
    pub pattern_id: String,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub phases: Vec<OccurrencePhase>,
    pub include_invalid_warnings: bool,
}

/// Pattern metadata echoed into the timeline output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatternTimelinePattern {
    pub pattern_id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
}

/// The filters that were applied, echoed back for clarity.
#[derive(Debug, Clone, Serialize)]
pub struct PatternTimelineFilters {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub phases: Vec<OccurrencePhase>,
}

/// One occurrence as surfaced in the timeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatternTimelineOccurrence {
    pub occurrence_id: String,
    pub pattern_id: String,
    pub occurrence_date: String,
    pub phase: OccurrencePhase,
    pub summary: String,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intensity: Option<f32>,
    pub fact_ids: Vec<String>,
    pub interpretation_ids: Vec<String>,
}

/// Occurrences grouped under one date.
#[derive(Debug, Clone, Serialize)]
pub struct PatternTimelineDateGroup {
    pub date: String,
    pub occurrences: Vec<PatternTimelineOccurrence>,
}

/// A non-fatal warning about a record encountered during the read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatternTimelineWarning {
    pub warning_code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_memory_id: Option<String>,
}

/// Descriptive per-phase counts (post-filter, post-dedup). Always lists every
/// phase, even at zero. Not a trend, not an activation signal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PhaseCounts {
    pub activated: usize,
    pub recognized_before_action: usize,
    pub recognized_after_action: usize,
    pub inhibited: usize,
    pub not_activated: usize,
    pub transformed: usize,
}

impl PhaseCounts {
    pub fn increment(&mut self, phase: OccurrencePhase) {
        match phase {
            OccurrencePhase::Activated => self.activated += 1,
            OccurrencePhase::RecognizedBeforeAction => self.recognized_before_action += 1,
            OccurrencePhase::RecognizedAfterAction => self.recognized_after_action += 1,
            OccurrencePhase::Inhibited => self.inhibited += 1,
            OccurrencePhase::NotActivated => self.not_activated += 1,
            OccurrencePhase::Transformed => self.transformed += 1,
        }
    }
}

/// Outcome of a `query_pattern_timeline` call.
///
/// The `Found` variant is intentionally larger than `Rejected`; this value is
/// built once and immediately serialized, so the size asymmetry is irrelevant.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum QueryPatternTimelineOutput {
    Found {
        pattern_id: String,
        pattern: PatternTimelinePattern,
        filters: PatternTimelineFilters,
        total_occurrences: usize,
        phase_counts: PhaseCounts,
        timeline: Vec<PatternTimelineDateGroup>,
        warnings: Vec<PatternTimelineWarning>,
    },
    Rejected {
        error_code: String,
        message: String,
    },
}

impl Serialize for QueryPatternTimelineOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            QueryPatternTimelineOutput::Found {
                pattern_id,
                pattern,
                filters,
                total_occurrences,
                phase_counts,
                timeline,
                warnings,
            } => {
                let mut map = serializer.serialize_map(Some(8))?;
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("pattern_id", pattern_id)?;
                map.serialize_entry("pattern", pattern)?;
                map.serialize_entry("filters", filters)?;
                map.serialize_entry("total_occurrences", total_occurrences)?;
                map.serialize_entry("phase_counts", phase_counts)?;
                map.serialize_entry("timeline", timeline)?;
                map.serialize_entry("warnings", warnings)?;
                map.end()
            }
            QueryPatternTimelineOutput::Rejected {
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
