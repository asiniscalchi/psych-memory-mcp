//! Domain validation for epistemic tool inputs.

use chrono::NaiveDate;

use crate::errors::ValidationError;
use crate::model::interpretation::{HIGH_CONFIDENCE_THRESHOLD, MIN_FACTS_FOR_HIGH_CONFIDENCE};
use crate::model::{
    InterpretationStatus, StoreInterpretationInput, StoreJournalFactInput, ValidatedInterpretation,
    ValidatedJournalFact,
};

/// True iff `s` is a strict `YYYY-MM-DD` calendar date.
///
/// `NaiveDate::parse_from_str` alone is too lax (it accepts `2026-6-1`), so we
/// also require the exact 4-2-2 zero-padded shape before checking validity.
fn is_yyyy_mm_dd(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits_ok = s
        .char_indices()
        .filter(|&(i, _)| i != 4 && i != 7)
        .all(|(_, c)| c.is_ascii_digit());
    digits_ok && NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// Validate a `store_journal_fact` input into a [`ValidatedJournalFact`].
///
/// Emptiness checks are done after trimming, but the original (untrimmed) text
/// is preserved — we never silently rewrite the journal's words.
pub fn validate_store_journal_fact(
    input: &StoreJournalFactInput,
) -> Result<ValidatedJournalFact, ValidationError> {
    if input.source_excerpt.trim().is_empty() {
        return Err(ValidationError::EmptySourceExcerpt);
    }
    if input.normalized_statement.trim().is_empty() {
        return Err(ValidationError::EmptyNormalizedStatement);
    }

    let entry_ref = input
        .journal_entry_ref
        .as_ref()
        .ok_or(ValidationError::MissingJournalEntryRef)?;

    if entry_ref.entry_id.trim().is_empty() {
        return Err(ValidationError::EmptyEntryId);
    }
    if !is_yyyy_mm_dd(&entry_ref.entry_date) {
        return Err(ValidationError::InvalidJournalEntryDate);
    }

    if let Some(event_date) = &input.event_date {
        if !is_yyyy_mm_dd(event_date) {
            return Err(ValidationError::InvalidEventDate);
        }
    }

    if let (Some(start), Some(end)) = (entry_ref.char_start, entry_ref.char_end) {
        if start > end {
            return Err(ValidationError::InvalidCharRange);
        }
    }

    Ok(ValidatedJournalFact {
        source_excerpt: input.source_excerpt.clone(),
        normalized_statement: input.normalized_statement.clone(),
        fact_type: input.fact_type,
        journal_entry_ref: entry_ref.clone(),
        event_date: input.event_date.clone(),
    })
}

/// Sort and de-duplicate fact ids so identity and tags never depend on input
/// order or repetition.
fn canonicalize_fact_ids(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.to_vec();
    out.sort();
    out.dedup();
    out
}

/// Validate a `store_interpretation` input's shape and scalar fields into a
/// [`ValidatedInterpretation`]. Evidence resolution (checking the supporting
/// facts actually exist) happens separately, in the evidence resolver.
pub fn validate_store_interpretation(
    input: &StoreInterpretationInput,
) -> Result<ValidatedInterpretation, ValidationError> {
    if input.hypothesis.trim().is_empty() {
        return Err(ValidationError::EmptyHypothesis);
    }
    if input.falsification_question.trim().is_empty() {
        return Err(ValidationError::MissingFalsificationQuestion);
    }

    if input.supported_by_fact_ids.is_empty() {
        return Err(ValidationError::MissingSupportingFacts);
    }
    if input
        .supported_by_fact_ids
        .iter()
        .any(|id| id.trim().is_empty())
    {
        return Err(ValidationError::EmptySupportingFactId);
    }
    if input
        .contradicted_by_fact_ids
        .iter()
        .any(|id| id.trim().is_empty())
    {
        return Err(ValidationError::EmptyContradictedFactId);
    }

    if !input.confidence.is_finite() || !(0.0..=1.0).contains(&input.confidence) {
        return Err(ValidationError::InvalidConfidence);
    }

    let supported = canonicalize_fact_ids(&input.supported_by_fact_ids);
    if input.confidence > HIGH_CONFIDENCE_THRESHOLD
        && supported.len() < MIN_FACTS_FOR_HIGH_CONFIDENCE
    {
        return Err(ValidationError::OverconfidentInterpretation {
            threshold: HIGH_CONFIDENCE_THRESHOLD,
            min: MIN_FACTS_FOR_HIGH_CONFIDENCE,
        });
    }

    match input.status {
        None | Some(InterpretationStatus::Candidate) => {}
        Some(_) => return Err(ValidationError::UnsupportedInterpretationStatus),
    }

    if let Some(review_due) = &input.review_due {
        if !is_yyyy_mm_dd(review_due) {
            return Err(ValidationError::InvalidReviewDue);
        }
    }

    Ok(ValidatedInterpretation {
        hypothesis: input.hypothesis.clone(),
        interpretation_type: input.interpretation_type,
        supported_by_fact_ids: supported,
        contradicted_by_fact_ids: input.contradicted_by_fact_ids.clone(),
        confidence: input.confidence,
        falsification_question: input.falsification_question.clone(),
        review_due: input.review_due.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FactType, JournalEntryRef};

    fn valid_input() -> StoreJournalFactInput {
        StoreJournalFactInput {
            source_excerpt: "I felt very strong hunger.".into(),
            normalized_statement: "Ale reported feeling strong hunger.".into(),
            fact_type: FactType::SelfReport,
            journal_entry_ref: Some(JournalEntryRef {
                entry_id: "froid_2026_06_01_abc123".into(),
                entry_date: "2026-06-01".into(),
                char_start: Some(120),
                char_end: Some(190),
                content_hash: None,
            }),
            event_date: None,
        }
    }

    fn code(input: &StoreJournalFactInput) -> &'static str {
        validate_store_journal_fact(input).unwrap_err().error_code()
    }

    #[test]
    fn accepts_valid_journal_fact() {
        assert!(validate_store_journal_fact(&valid_input()).is_ok());
    }

    #[test]
    fn rejects_empty_source_excerpt() {
        let mut i = valid_input();
        i.source_excerpt = "   ".into();
        assert_eq!(code(&i), "empty_source_excerpt");
    }

    #[test]
    fn rejects_empty_normalized_statement() {
        let mut i = valid_input();
        i.normalized_statement = "".into();
        assert_eq!(code(&i), "empty_normalized_statement");
    }

    #[test]
    fn rejects_missing_journal_entry_ref() {
        let mut i = valid_input();
        i.journal_entry_ref = None;
        assert_eq!(code(&i), "missing_journal_entry_ref");
    }

    #[test]
    fn rejects_empty_entry_id() {
        let mut i = valid_input();
        i.journal_entry_ref.as_mut().unwrap().entry_id = " ".into();
        assert_eq!(code(&i), "empty_entry_id");
    }

    #[test]
    fn rejects_invalid_entry_date() {
        let mut i = valid_input();
        i.journal_entry_ref.as_mut().unwrap().entry_date = "banana".into();
        assert_eq!(code(&i), "invalid_journal_entry_date");
    }

    #[test]
    fn rejects_non_padded_entry_date() {
        let mut i = valid_input();
        i.journal_entry_ref.as_mut().unwrap().entry_date = "2026-6-1".into();
        assert_eq!(code(&i), "invalid_journal_entry_date");
    }

    #[test]
    fn rejects_invalid_event_date() {
        let mut i = valid_input();
        i.event_date = Some("banana".into());
        assert_eq!(code(&i), "invalid_event_date");
    }

    #[test]
    fn accepts_valid_event_date() {
        let mut i = valid_input();
        i.event_date = Some("2026-05-31".into());
        assert!(validate_store_journal_fact(&i).is_ok());
    }

    #[test]
    fn rejects_invalid_char_range() {
        let mut i = valid_input();
        let r = i.journal_entry_ref.as_mut().unwrap();
        r.char_start = Some(200);
        r.char_end = Some(100);
        assert_eq!(code(&i), "invalid_char_range");
    }
}

#[cfg(test)]
mod interpretation_tests {
    use super::*;
    use crate::model::InterpretationType;

    fn valid() -> StoreInterpretationInput {
        StoreInterpretationInput {
            hypothesis: "Hunger may have functioned as emotional discharge.".into(),
            interpretation_type: InterpretationType::PsychologicalHypothesis,
            supported_by_fact_ids: vec!["fact_aaa".into()],
            contradicted_by_fact_ids: vec![],
            confidence: 0.35,
            status: None,
            falsification_question: "Are there similar episodes without activation?".into(),
            review_due: Some("2026-06-09".into()),
        }
    }

    fn code(i: &StoreInterpretationInput) -> &'static str {
        validate_store_interpretation(i).unwrap_err().error_code()
    }

    #[test]
    fn accepts_valid_candidate_interpretation() {
        assert!(validate_store_interpretation(&valid()).is_ok());
    }

    #[test]
    fn defaults_missing_status_to_candidate() {
        // None status is accepted; the validated value carries no rejection.
        let v = validate_store_interpretation(&valid()).unwrap();
        assert_eq!(v.supported_by_fact_ids, vec!["fact_aaa".to_string()]);
    }

    #[test]
    fn rejects_empty_hypothesis() {
        let mut i = valid();
        i.hypothesis = "  ".into();
        assert_eq!(code(&i), "empty_hypothesis");
    }

    #[test]
    fn rejects_missing_falsification_question() {
        let mut i = valid();
        i.falsification_question = "".into();
        assert_eq!(code(&i), "missing_falsification_question");
    }

    #[test]
    fn rejects_missing_supporting_facts() {
        let mut i = valid();
        i.supported_by_fact_ids = vec![];
        assert_eq!(code(&i), "missing_supporting_facts");
    }

    #[test]
    fn rejects_empty_supporting_fact_id() {
        let mut i = valid();
        i.supported_by_fact_ids = vec!["fact_a".into(), " ".into()];
        assert_eq!(code(&i), "empty_supporting_fact_id");
    }

    #[test]
    fn rejects_empty_contradicted_fact_id() {
        let mut i = valid();
        i.contradicted_by_fact_ids = vec!["".into()];
        assert_eq!(code(&i), "empty_contradicted_fact_id");
    }

    #[test]
    fn rejects_invalid_confidence_above_one() {
        let mut i = valid();
        i.confidence = 1.5;
        assert_eq!(code(&i), "invalid_confidence");
    }

    #[test]
    fn rejects_invalid_confidence_nan_or_infinite() {
        let mut i = valid();
        i.confidence = f32::NAN;
        assert_eq!(code(&i), "invalid_confidence");
        i.confidence = f32::INFINITY;
        assert_eq!(code(&i), "invalid_confidence");
    }

    #[test]
    fn rejects_overconfident_with_too_few_facts() {
        let mut i = valid();
        i.confidence = 0.8;
        i.supported_by_fact_ids = vec!["fact_a".into()];
        assert_eq!(code(&i), "overconfident_interpretation");
    }

    #[test]
    fn accepts_high_confidence_with_enough_facts() {
        let mut i = valid();
        i.confidence = 0.8;
        i.supported_by_fact_ids = vec!["fact_a".into(), "fact_b".into(), "fact_c".into()];
        assert!(validate_store_interpretation(&i).is_ok());
    }

    #[test]
    fn rejects_non_candidate_status() {
        let mut i = valid();
        i.status = Some(InterpretationStatus::Accepted);
        assert_eq!(code(&i), "unsupported_interpretation_status");
    }

    #[test]
    fn rejects_invalid_review_due() {
        let mut i = valid();
        i.review_due = Some("banana".into());
        assert_eq!(code(&i), "invalid_review_due");
    }

    #[test]
    fn canonicalizes_supported_fact_ids_sorted_and_deduped() {
        let mut i = valid();
        i.supported_by_fact_ids = vec!["fact_c".into(), "fact_a".into(), "fact_c".into()];
        let v = validate_store_interpretation(&i).unwrap();
        assert_eq!(
            v.supported_by_fact_ids,
            vec!["fact_a".to_string(), "fact_c".to_string()]
        );
    }
}
