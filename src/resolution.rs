//! Shared typed lookup for resolving one epistemic record by an id-tag.
//!
//! Every entity type (pattern seed, journal fact, interpretation) is resolved
//! with the *same* semantics, so a single valid record stays usable even when
//! unrelated corrupt or tag-colliding records exist:
//!
//! * 0 raw matches                         -> `NotFound`
//! * exactly 1 valid (any extra invalid)   -> `Found`
//! * more than 1 valid                     -> `Ambiguous`
//! * matches exist but none valid          -> `InvalidMatch`

use crate::backend::{MemoryBackend, MemoryRecord};
use crate::errors::PsychMemoryError;

/// Outcome of resolving an id to a single valid memory by tag.
#[derive(Debug)]
pub enum TypedLookup {
    NotFound,
    Found(MemoryRecord),
    Ambiguous(Vec<MemoryRecord>),
    /// Tag matched, but no match passed validation (carries the raw matches so
    /// callers can classify the precise rejection reason).
    InvalidMatch(Vec<MemoryRecord>),
}

/// Resolve a single record by exact `tag`, validating each match with `is_valid`.
pub async fn resolve_one_by_tag(
    backend: &dyn MemoryBackend,
    tag: &str,
    is_valid: impl Fn(&MemoryRecord) -> bool,
) -> Result<TypedLookup, PsychMemoryError> {
    let matches = backend.find_memories_by_tag(tag).await?;
    if matches.is_empty() {
        return Ok(TypedLookup::NotFound);
    }

    let (mut valid, invalid): (Vec<MemoryRecord>, Vec<MemoryRecord>) =
        matches.into_iter().partition(&is_valid);

    Ok(match valid.len() {
        0 => TypedLookup::InvalidMatch(invalid),
        1 => TypedLookup::Found(valid.remove(0)),
        _ => TypedLookup::Ambiguous(valid),
    })
}
