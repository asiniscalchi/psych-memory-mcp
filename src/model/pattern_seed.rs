//! Domain models for `create_pattern_seed`.
//!
//! Epistemic stance: a pattern seed is only a *named category for future
//! observation* — never a fact, never an interpretation, and never a claim
//! that the pattern is active. Activation requires future Occurrence records.

use schemars::JsonSchema;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

/// Backend memory type stored for every pattern seed.
pub const MEMORY_TYPE: &str = "pattern_seed";
/// Epistemic status: a category of observation, not evidence of activation.
pub const EPISTEMIC_STATUS: &str = "observation_category";
/// Lifecycle status. Story 3 only ever stores `seed`.
pub const STATUS: &str = "seed";
/// Schema version stamped into pattern-seed metadata.
pub const SCHEMA_VERSION: &str = "psych-memory.pattern_seed.v1";

/// Input to the `create_pattern_seed` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CreatePatternSeedInput {
    /// Human-readable pattern name.
    pub name: String,
    /// Stable machine-readable identifier (`^[a-z][a-z0-9_]*$`, no `__`).
    pub slug: String,
    /// Concise description of what this category observes.
    pub description: String,
    /// Concrete signs that may suggest this pattern in future episodes.
    pub markers: Vec<String>,
    /// Concrete signs of non-activation / weakening / interruption.
    pub counter_markers: Vec<String>,
    /// Alternative names for the same category. Optional.
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// A `CreatePatternSeedInput` that passed validation. `aliases` are normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPatternSeed {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub markers: Vec<String>,
    pub counter_markers: Vec<String>,
    pub aliases: Vec<String>,
}

/// Outcome of a `create_pattern_seed` call. Serializes to the flat,
/// `ok`-discriminated wire shape shared by the other epistemic tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePatternSeedOutput {
    /// A new seed was stored.
    Stored {
        pattern_id: String,
        backend_memory_id: Option<String>,
    },
    /// A seed with this `pattern_id` already existed; nothing was stored.
    AlreadyExists {
        pattern_id: String,
        backend_memory_id: Option<String>,
    },
    Rejected {
        error_code: String,
        message: String,
    },
}

impl Serialize for CreatePatternSeedOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            CreatePatternSeedOutput::Stored {
                pattern_id,
                backend_memory_id,
            } => serialize_stored(serializer, pattern_id, backend_memory_id, "stored"),
            CreatePatternSeedOutput::AlreadyExists {
                pattern_id,
                backend_memory_id,
            } => serialize_stored(serializer, pattern_id, backend_memory_id, "already_exists"),
            CreatePatternSeedOutput::Rejected {
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

fn serialize_stored<S>(
    serializer: S,
    pattern_id: &str,
    backend_memory_id: &Option<String>,
    status: &str,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut map = serializer.serialize_map(Some(4))?;
    map.serialize_entry("ok", &true)?;
    map.serialize_entry("pattern_id", pattern_id)?;
    map.serialize_entry("backend_memory_id", backend_memory_id)?;
    map.serialize_entry("status", status)?;
    map.end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stored_wire_shape() {
        let out = CreatePatternSeedOutput::Stored {
            pattern_id: "pattern_savior".into(),
            backend_memory_id: Some("backend_789".into()),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({"ok": true, "pattern_id": "pattern_savior", "backend_memory_id": "backend_789", "status": "stored"})
        );
    }

    #[test]
    fn already_exists_wire_shape() {
        let out = CreatePatternSeedOutput::AlreadyExists {
            pattern_id: "pattern_savior".into(),
            backend_memory_id: None,
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({"ok": true, "pattern_id": "pattern_savior", "backend_memory_id": null, "status": "already_exists"})
        );
    }

    #[test]
    fn rejected_wire_shape() {
        let out = CreatePatternSeedOutput::Rejected {
            error_code: "invalid_pattern_slug".into(),
            message: "bad slug".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({"ok": false, "error_code": "invalid_pattern_slug", "message": "bad slug"})
        );
    }
}
