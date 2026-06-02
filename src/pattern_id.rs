//! Deterministic `pattern_id` generation.
//!
//! Unlike facts and interpretations, a pattern seed is a *human-stable named
//! category*, so its id is simply `pattern_<slug>` — not a hash. Identity
//! depends only on the slug, never on the name, description, markers,
//! counter-markers, or aliases, which may all be refined over time.

use crate::model::ValidatedPatternSeed;

/// Generate the `pattern_<slug>` id for a validated pattern seed.
pub fn generate_pattern_id(seed: &ValidatedPatternSeed) -> String {
    format!("pattern_{}", seed.slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(slug: &str, description: &str) -> ValidatedPatternSeed {
        ValidatedPatternSeed {
            name: "Name".into(),
            slug: slug.into(),
            description: description.into(),
            markers: vec!["m".into()],
            counter_markers: vec!["c".into()],
            aliases: vec![],
        }
    }

    #[test]
    fn same_slug_same_pattern_id() {
        assert_eq!(
            generate_pattern_id(&seed("savior", "a")),
            generate_pattern_id(&seed("savior", "a"))
        );
    }

    #[test]
    fn same_slug_different_description_same_pattern_id() {
        assert_eq!(
            generate_pattern_id(&seed("savior", "one")),
            generate_pattern_id(&seed("savior", "completely different"))
        );
    }

    #[test]
    fn different_slug_different_pattern_id() {
        assert_ne!(
            generate_pattern_id(&seed("savior", "a")),
            generate_pattern_id(&seed("rejection_wound", "a"))
        );
    }

    #[test]
    fn format_is_pattern_underscore_slug() {
        assert_eq!(
            generate_pattern_id(&seed("hunger_as_discharge", "a")),
            "pattern_hunger_as_discharge"
        );
    }
}
