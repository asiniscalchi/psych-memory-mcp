//! Domain validation for epistemic tool inputs.

use chrono::NaiveDate;

use crate::errors::ValidationError;
use crate::model::{StoreJournalFactInput, ValidatedJournalFact};

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
