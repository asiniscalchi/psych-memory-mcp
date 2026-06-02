//! Deterministic, source-anchored `fact_id` generation.
//!
//! Identity rule: *the same journal source span is the same fact*. The id is
//! derived only from the stable source anchor — `entry_id`, the character
//! offsets (when present), and a hash of the `source_excerpt`. It deliberately
//! excludes `normalized_statement` (model-generated, may be re-normalized),
//! `fact_type` (may be corrected), `event_date`, and `content_hash`, so the id
//! stays stable as those are refined.

use sha2::{Digest, Sha256};

use crate::model::ValidatedJournalFact;

/// Domain separator so this hashing scheme can evolve without colliding with
/// other uses of SHA-256 in the system.
const FACT_ID_DOMAIN: &str = "psych-memory.fact_id.v1";

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Generate the deterministic `fact_<sha256>` id for a validated journal fact.
///
/// Variable-length strings (`entry_id`, `source_excerpt`) are folded into the
/// preimage via their own fixed-length SHA-256, so no field boundary can be
/// ambiguous (e.g. `entry_id="a"` + `start=1` cannot collide with
/// `entry_id="a1"`).
pub fn generate_fact_id(fact: &ValidatedJournalFact) -> String {
    let entry_ref = &fact.journal_entry_ref;
    let char_start = entry_ref
        .char_start
        .map(|v| v.to_string())
        .unwrap_or_else(|| "none".to_string());
    let char_end = entry_ref
        .char_end
        .map(|v| v.to_string())
        .unwrap_or_else(|| "none".to_string());

    let preimage = format!(
        "{domain}|entry_id={entry_id}|char_start={char_start}|char_end={char_end}|excerpt={excerpt}",
        domain = FACT_ID_DOMAIN,
        entry_id = sha256_hex(&entry_ref.entry_id),
        excerpt = sha256_hex(&fact.source_excerpt),
    );

    format!("fact_{}", sha256_hex(&preimage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FactType, JournalEntryRef};

    fn fact() -> ValidatedJournalFact {
        ValidatedJournalFact {
            source_excerpt: "I felt very strong hunger.".into(),
            normalized_statement: "Ale reported feeling strong hunger.".into(),
            fact_type: FactType::SelfReport,
            journal_entry_ref: JournalEntryRef {
                entry_id: "froid_2026_06_01_abc123".into(),
                entry_date: "2026-06-01".into(),
                char_start: Some(120),
                char_end: Some(190),
                content_hash: None,
            },
            event_date: None,
        }
    }

    #[test]
    fn same_input_same_fact_id() {
        assert_eq!(generate_fact_id(&fact()), generate_fact_id(&fact()));
    }

    #[test]
    fn fact_id_has_expected_prefix() {
        assert!(generate_fact_id(&fact()).starts_with("fact_"));
    }

    #[test]
    fn same_statement_different_entry_id_different_fact_id() {
        let mut a = fact();
        a.journal_entry_ref.entry_id = "entry_a".into();
        let mut b = fact();
        b.journal_entry_ref.entry_id = "entry_b".into();
        assert_ne!(generate_fact_id(&a), generate_fact_id(&b));
    }

    #[test]
    fn same_entry_and_source_different_normalized_statement_same_fact_id() {
        let mut a = fact();
        a.normalized_statement = "phrasing one".into();
        let mut b = fact();
        b.normalized_statement = "completely different phrasing two".into();
        assert_eq!(generate_fact_id(&a), generate_fact_id(&b));
    }

    #[test]
    fn same_entry_and_source_different_fact_type_same_fact_id() {
        let mut a = fact();
        a.fact_type = FactType::SelfReport;
        let mut b = fact();
        b.fact_type = FactType::ReportedEvent;
        assert_eq!(generate_fact_id(&a), generate_fact_id(&b));
    }

    #[test]
    fn same_entry_different_char_range_different_fact_id() {
        let mut a = fact();
        a.journal_entry_ref.char_start = Some(0);
        a.journal_entry_ref.char_end = Some(10);
        let mut b = fact();
        b.journal_entry_ref.char_start = Some(20);
        b.journal_entry_ref.char_end = Some(30);
        assert_ne!(generate_fact_id(&a), generate_fact_id(&b));
    }

    #[test]
    fn same_entry_without_offsets_same_source_same_fact_id() {
        let mut a = fact();
        a.journal_entry_ref.char_start = None;
        a.journal_entry_ref.char_end = None;
        let mut b = a.clone();
        b.normalized_statement = "re-normalized later".into();
        assert_eq!(generate_fact_id(&a), generate_fact_id(&b));
    }

    #[test]
    fn different_source_excerpt_different_fact_id() {
        let mut a = fact();
        a.source_excerpt = "one".into();
        let mut b = fact();
        b.source_excerpt = "two".into();
        assert_ne!(generate_fact_id(&a), generate_fact_id(&b));
    }
}
