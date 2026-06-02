//! Validation for `create_pattern_seed`: slug format, alias normalization, and
//! the identity-claim guard.
//!
//! The identity-claim guard is a cheap guardrail against the worst category
//! error — storing "<X> has the pattern" / "this pattern is active" as if a
//! seed were evidence of activation. It is intentionally not an NLP classifier;
//! it uses word-boundary regexes so ordinary words ("scale is", "rationale is")
//! are never flagged.

use std::sync::LazyLock;

use regex::Regex;

use crate::errors::ValidationError;
use crate::model::{CreatePatternSeedInput, ValidatedPatternSeed};

static SLUG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").unwrap());

static IDENTITY_CLAIM_RES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)\b(ale|alessandro)\s+has\s+(the\s+)?[a-z0-9_ -]*pattern\b",
        r"(?i)\b(ale|alessandro)\s+is\s+(a|an)\s+[a-z0-9_ -]+",
        r"(?i)\bthis\s+pattern\s+is\s+active\b",
        r"(?i)\bis\s+active\s+in\b",
        r"(?i)\bhas\s+this\s+pattern\b",
        r"(?i)\bhas\s+the\s+[a-z0-9_ -]*pattern\b",
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});

/// A slug is valid if it matches `^[a-z][a-z0-9_]*$` and has no `__`.
pub fn is_valid_slug(slug: &str) -> bool {
    SLUG_RE.is_match(slug) && !slug.contains("__")
}

/// Normalize an alias: trim, lowercase, whitespace/hyphens to `_`, collapse
/// repeated `_`, and strip leading/trailing `_`.
pub fn normalize_alias(alias: &str) -> String {
    let lowered = alias.trim().to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut prev_underscore = false;
    for ch in lowered.chars() {
        let mapped = if ch.is_whitespace() || ch == '-' || ch == '_' {
            '_'
        } else {
            ch
        };
        if mapped == '_' {
            if !prev_underscore {
                out.push('_');
            }
            prev_underscore = true;
        } else {
            out.push(mapped);
            prev_underscore = false;
        }
    }
    out.trim_matches('_').to_string()
}

/// True if `text` contains an obvious active-ownership / activation claim.
pub fn has_identity_claim(text: &str) -> bool {
    IDENTITY_CLAIM_RES.iter().any(|re| re.is_match(text))
}

/// Validate a `create_pattern_seed` input into a [`ValidatedPatternSeed`] with
/// normalized aliases.
pub fn validate_create_pattern_seed(
    input: &CreatePatternSeedInput,
) -> Result<ValidatedPatternSeed, ValidationError> {
    if input.name.trim().is_empty() {
        return Err(ValidationError::EmptyPatternName);
    }
    if !is_valid_slug(&input.slug) {
        return Err(ValidationError::InvalidPatternSlug);
    }
    if input.description.trim().is_empty() {
        return Err(ValidationError::EmptyPatternDescription);
    }

    if input.markers.is_empty() {
        return Err(ValidationError::MissingPatternMarkers);
    }
    if input.markers.iter().any(|m| m.trim().is_empty()) {
        return Err(ValidationError::EmptyPatternMarker);
    }
    if input.counter_markers.is_empty() {
        return Err(ValidationError::MissingPatternCounterMarkers);
    }
    if input.counter_markers.iter().any(|m| m.trim().is_empty()) {
        return Err(ValidationError::EmptyPatternCounterMarker);
    }

    if has_identity_claim(&input.name) || has_identity_claim(&input.description) {
        return Err(ValidationError::PatternIdentityClaim);
    }

    let mut aliases = Vec::with_capacity(input.aliases.len());
    for raw in &input.aliases {
        let normalized = normalize_alias(raw);
        if normalized.is_empty() {
            return Err(ValidationError::EmptyPatternAlias);
        }
        if normalized == input.slug {
            return Err(ValidationError::AliasEqualsSlug);
        }
        if aliases.contains(&normalized) {
            return Err(ValidationError::DuplicatePatternAlias);
        }
        aliases.push(normalized);
    }

    Ok(ValidatedPatternSeed {
        name: input.name.clone(),
        slug: input.slug.clone(),
        description: input.description.clone(),
        markers: input.markers.clone(),
        counter_markers: input.counter_markers.clone(),
        aliases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> CreatePatternSeedInput {
        CreatePatternSeedInput {
            name: "Savior".into(),
            slug: "savior".into(),
            description: "A tendency to feel urgency to rescue or fix the other person.".into(),
            markers: vec!["urgency to intervene".into()],
            counter_markers: vec!["ability to wait".into()],
            aliases: vec!["rescuer".into(), "rescue impulse".into()],
        }
    }

    fn code(i: &CreatePatternSeedInput) -> &'static str {
        validate_create_pattern_seed(i).unwrap_err().error_code()
    }

    #[test]
    fn accepts_valid_pattern_seed() {
        let v = validate_create_pattern_seed(&valid()).unwrap();
        assert_eq!(
            v.aliases,
            vec!["rescuer".to_string(), "rescue_impulse".to_string()]
        );
    }

    #[test]
    fn rejects_empty_pattern_name() {
        let mut i = valid();
        i.name = "  ".into();
        assert_eq!(code(&i), "empty_pattern_name");
    }

    #[test]
    fn rejects_empty_pattern_description() {
        let mut i = valid();
        i.description = "".into();
        assert_eq!(code(&i), "empty_pattern_description");
    }

    #[test]
    fn rejects_invalid_slug_with_hyphen() {
        let mut i = valid();
        i.slug = "rejection-wound".into();
        assert_eq!(code(&i), "invalid_pattern_slug");
    }

    #[test]
    fn rejects_invalid_slug_with_space() {
        let mut i = valid();
        i.slug = "rejection wound".into();
        assert_eq!(code(&i), "invalid_pattern_slug");
    }

    #[test]
    fn rejects_invalid_slug_uppercase() {
        let mut i = valid();
        i.slug = "Savior".into();
        assert_eq!(code(&i), "invalid_pattern_slug");
    }

    #[test]
    fn rejects_consecutive_underscores() {
        let mut i = valid();
        i.slug = "foo__bar".into();
        assert_eq!(code(&i), "invalid_pattern_slug");
    }

    #[test]
    fn rejects_missing_markers() {
        let mut i = valid();
        i.markers = vec![];
        assert_eq!(code(&i), "missing_pattern_markers");
    }

    #[test]
    fn rejects_empty_marker() {
        let mut i = valid();
        i.markers = vec!["ok".into(), " ".into()];
        assert_eq!(code(&i), "empty_pattern_marker");
    }

    #[test]
    fn rejects_missing_counter_markers() {
        let mut i = valid();
        i.counter_markers = vec![];
        assert_eq!(code(&i), "missing_pattern_counter_markers");
    }

    #[test]
    fn rejects_empty_counter_marker() {
        let mut i = valid();
        i.counter_markers = vec!["".into()];
        assert_eq!(code(&i), "empty_pattern_counter_marker");
    }

    #[test]
    fn rejects_duplicate_alias_after_normalization() {
        let mut i = valid();
        i.aliases = vec!["rescue impulse".into(), "rescue-impulse".into()];
        assert_eq!(code(&i), "duplicate_pattern_alias");
    }

    #[test]
    fn rejects_alias_equal_to_slug() {
        let mut i = valid();
        i.aliases = vec!["savior".into()];
        assert_eq!(code(&i), "alias_equals_slug");
    }

    #[test]
    fn accepts_omitted_aliases_as_empty() {
        let mut i = valid();
        i.aliases = vec![];
        let v = validate_create_pattern_seed(&i).unwrap();
        assert!(v.aliases.is_empty());
    }

    // --- alias normalization ---

    #[test]
    fn normalizes_alias_whitespace_to_underscore() {
        assert_eq!(normalize_alias("rescue impulse"), "rescue_impulse");
    }

    #[test]
    fn normalizes_alias_hyphen_to_underscore() {
        assert_eq!(normalize_alias("rescue-impulse"), "rescue_impulse");
    }

    #[test]
    fn collapses_alias_underscores() {
        assert_eq!(normalize_alias("rescue   impulse"), "rescue_impulse");
        assert_eq!(normalize_alias("--rescue--"), "rescue");
    }

    // --- identity-claim guard ---

    #[test]
    fn does_not_reject_scale_is() {
        let mut i = valid();
        i.description = "This scale is about urgency to intervene.".into();
        assert!(validate_create_pattern_seed(&i).is_ok());
    }

    #[test]
    fn does_not_reject_rationale_is() {
        let mut i = valid();
        i.description = "The rationale is to track future occurrences.".into();
        assert!(validate_create_pattern_seed(&i).is_ok());
    }

    #[test]
    fn does_not_reject_whale_is() {
        let mut i = valid();
        i.description = "Like the way a whale is large, urgency can feel huge.".into();
        assert!(validate_create_pattern_seed(&i).is_ok());
    }

    #[test]
    fn rejects_ale_has_the_pattern() {
        let mut i = valid();
        i.description = "Ale has the Savior pattern.".into();
        assert_eq!(code(&i), "pattern_identity_claim");
    }

    #[test]
    fn rejects_alessandro_is_a_savior() {
        let mut i = valid();
        i.description = "Alessandro is a savior.".into();
        assert_eq!(code(&i), "pattern_identity_claim");
    }

    #[test]
    fn rejects_this_pattern_is_active() {
        let mut i = valid();
        i.description = "This pattern is active.".into();
        assert_eq!(code(&i), "pattern_identity_claim");
    }

    #[test]
    fn rejects_active_in_ale() {
        let mut i = valid();
        i.description = "This is active in Ale right now.".into();
        assert_eq!(code(&i), "pattern_identity_claim");
    }

    #[test]
    fn rejects_has_this_pattern() {
        let mut i = valid();
        i.name = "He has this pattern".into();
        assert_eq!(code(&i), "pattern_identity_claim");
    }
}
