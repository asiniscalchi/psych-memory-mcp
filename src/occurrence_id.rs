//! Deterministic `occurrence_id` generation.
//!
//! Identity is the concrete episode claim: pattern, date, phase, the sorted
//! supporting `fact_id`s and linked `interpretation_id`s, and the (trimmed)
//! summary. It excludes `confidence` and `intensity`, which may be refined
//! without changing what the occurrence is. The summary *is* included (the same
//! pattern/date/facts can support two different episode claims), with the known
//! tradeoff that rewriting the summary yields a new id (no update workflow yet).

use crate::hashing::sha256_hex;
use crate::model::ValidatedPatternOccurrence;

const OCCURRENCE_ID_DOMAIN: &str = "psych-memory.occurrence_id.v1";

/// Generate the `occ_<sha256>` id for a validated occurrence. `fact_ids` and
/// `interpretation_ids` are assumed already sorted+deduped by validation.
pub fn generate_occurrence_id(occurrence: &ValidatedPatternOccurrence) -> String {
    let facts = occurrence.fact_ids.join("\n");
    let interps = occurrence.interpretation_ids.join("\n");

    let preimage = format!(
        "{domain}|pattern={pattern}|date={date}|phase={phase}|facts={facts}|interps={interps}|summary={summary}",
        domain = OCCURRENCE_ID_DOMAIN,
        pattern = sha256_hex(&occurrence.pattern_id),
        date = occurrence.occurrence_date,
        phase = occurrence.phase.as_str(),
        facts = sha256_hex(&facts),
        interps = sha256_hex(&interps),
        summary = sha256_hex(occurrence.summary.trim()),
    );

    format!("occ_{}", sha256_hex(&preimage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OccurrencePhase;

    fn occ() -> ValidatedPatternOccurrence {
        ValidatedPatternOccurrence {
            pattern_id: "pattern_savior".into(),
            fact_ids: vec!["fact_a".into(), "fact_b".into()],
            interpretation_ids: vec!["interp_a".into()],
            occurrence_date: "2026-06-01".into(),
            phase: OccurrencePhase::RecognizedBeforeAction,
            summary: "The rescue impulse appeared but was noticed.".into(),
            confidence: 0.5,
            intensity: Some(0.4),
        }
    }

    #[test]
    fn same_input_same_occurrence_id() {
        assert_eq!(
            generate_occurrence_id(&occ()),
            generate_occurrence_id(&occ())
        );
    }

    #[test]
    fn has_expected_prefix() {
        assert!(generate_occurrence_id(&occ()).starts_with("occ_"));
    }

    #[test]
    fn same_core_different_confidence_same_occurrence_id() {
        let mut a = occ();
        a.confidence = 0.1;
        let mut b = occ();
        b.confidence = 0.9;
        assert_eq!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    #[test]
    fn same_core_different_intensity_same_occurrence_id() {
        let mut a = occ();
        a.intensity = Some(0.1);
        let mut b = occ();
        b.intensity = None;
        assert_eq!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    #[test]
    fn different_fact_ids_different_occurrence_id() {
        let mut a = occ();
        a.fact_ids = vec!["fact_a".into()];
        let mut b = occ();
        b.fact_ids = vec!["fact_a".into(), "fact_z".into()];
        assert_ne!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    #[test]
    fn different_phase_different_occurrence_id() {
        let mut a = occ();
        a.phase = OccurrencePhase::Activated;
        let mut b = occ();
        b.phase = OccurrencePhase::Inhibited;
        assert_ne!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    #[test]
    fn different_summary_different_occurrence_id() {
        let mut a = occ();
        a.summary = "Summary one.".into();
        let mut b = occ();
        b.summary = "A wholly different summary.".into();
        assert_ne!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    #[test]
    fn same_summary_with_leading_or_trailing_whitespace_same_occurrence_id() {
        let mut a = occ();
        a.summary = "  trimmed summary  ".into();
        let mut b = occ();
        b.summary = "trimmed summary".into();
        assert_eq!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    // Order independence is guaranteed by validation's canonicalisation.
    use crate::model::RecordPatternOccurrenceInput;
    use crate::validators::validate_record_pattern_occurrence;

    fn input(facts: &[&str], interps: &[&str]) -> RecordPatternOccurrenceInput {
        RecordPatternOccurrenceInput {
            pattern_id: "pattern_savior".into(),
            fact_ids: facts.iter().map(|s| s.to_string()).collect(),
            interpretation_ids: interps.iter().map(|s| s.to_string()).collect(),
            occurrence_date: "2026-06-01".into(),
            phase: OccurrencePhase::Activated,
            summary: "Same summary.".into(),
            confidence: 0.3,
            intensity: None,
        }
    }

    #[test]
    fn same_fact_ids_different_order_same_occurrence_id() {
        let a = validate_record_pattern_occurrence(&input(&["fact_b", "fact_a"], &[])).unwrap();
        let b = validate_record_pattern_occurrence(&input(&["fact_a", "fact_b"], &[])).unwrap();
        assert_eq!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }

    #[test]
    fn same_interpretation_ids_different_order_same_occurrence_id() {
        let a = validate_record_pattern_occurrence(&input(&["fact_a"], &["interp_b", "interp_a"]))
            .unwrap();
        let b = validate_record_pattern_occurrence(&input(&["fact_a"], &["interp_a", "interp_b"]))
            .unwrap();
        assert_eq!(generate_occurrence_id(&a), generate_occurrence_id(&b));
    }
}
