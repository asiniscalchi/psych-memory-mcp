//! Deterministic `interpretation_id` generation.
//!
//! Identity rule: an interpretation *is* its core claim plus its primary
//! evidence. The id is derived only from the (whitespace-trimmed) hypothesis,
//! the `interpretation_type`, and the sorted+de-duplicated supporting
//! `fact_id`s. It deliberately excludes `confidence`, `status`,
//! `falsification_question`, `review_due`, and `contradicted_by_fact_ids` —
//! those change or accrue over time without changing what the interpretation
//! is.

use crate::hashing::sha256_hex;
use crate::model::ValidatedInterpretation;

/// Domain separator for this hashing scheme.
const INTERPRETATION_ID_DOMAIN: &str = "psych-memory.interpretation_id.v1";

/// Generate the deterministic `interp_<sha256>` id for a validated
/// interpretation. `supported_by_fact_ids` is assumed already canonicalised
/// (sorted + de-duplicated) by validation.
pub fn generate_interpretation_id(interpretation: &ValidatedInterpretation) -> String {
    // fact_ids are `fact_<hex>` with no `\n`, so newline-joining is unambiguous;
    // variable-length text fields are folded in via their own SHA-256.
    let facts_joined = interpretation.supported_by_fact_ids.join("\n");

    let preimage = format!(
        "{domain}|type={interpretation_type}|hypothesis={hypothesis}|facts={facts}",
        domain = INTERPRETATION_ID_DOMAIN,
        interpretation_type = interpretation.interpretation_type.as_str(),
        hypothesis = sha256_hex(interpretation.hypothesis.trim()),
        facts = sha256_hex(&facts_joined),
    );

    format!("interp_{}", sha256_hex(&preimage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InterpretationType, StoreInterpretationInput};
    use crate::validators::validate_store_interpretation;

    fn interp() -> ValidatedInterpretation {
        ValidatedInterpretation {
            hypothesis: "Hunger may have functioned as emotional discharge.".into(),
            interpretation_type: InterpretationType::PsychologicalHypothesis,
            supported_by_fact_ids: vec!["fact_a".into(), "fact_b".into()],
            contradicted_by_fact_ids: vec![],
            confidence: 0.35,
            falsification_question: "Q?".into(),
            review_due: None,
        }
    }

    #[test]
    fn same_input_same_interpretation_id() {
        assert_eq!(
            generate_interpretation_id(&interp()),
            generate_interpretation_id(&interp())
        );
    }

    #[test]
    fn has_expected_prefix() {
        assert!(generate_interpretation_id(&interp()).starts_with("interp_"));
    }

    #[test]
    fn same_core_different_confidence_same_interpretation_id() {
        let mut a = interp();
        a.confidence = 0.1;
        let mut b = interp();
        b.confidence = 0.6;
        assert_eq!(
            generate_interpretation_id(&a),
            generate_interpretation_id(&b)
        );
    }

    #[test]
    fn same_core_different_falsification_question_same_interpretation_id() {
        let mut a = interp();
        a.falsification_question = "one".into();
        let mut b = interp();
        b.falsification_question = "two".into();
        assert_eq!(
            generate_interpretation_id(&a),
            generate_interpretation_id(&b)
        );
    }

    #[test]
    fn same_core_different_contradicted_by_same_interpretation_id() {
        let mut a = interp();
        a.contradicted_by_fact_ids = vec![];
        let mut b = interp();
        b.contradicted_by_fact_ids = vec!["fact_c".into()];
        assert_eq!(
            generate_interpretation_id(&a),
            generate_interpretation_id(&b)
        );
    }

    #[test]
    fn same_hypothesis_different_supporting_facts_different_interpretation_id() {
        let mut a = interp();
        a.supported_by_fact_ids = vec!["fact_a".into()];
        let mut b = interp();
        b.supported_by_fact_ids = vec!["fact_a".into(), "fact_b".into()];
        assert_ne!(
            generate_interpretation_id(&a),
            generate_interpretation_id(&b)
        );
    }

    #[test]
    fn same_supporting_facts_different_hypothesis_different_interpretation_id() {
        let mut a = interp();
        a.hypothesis = "Hypothesis one.".into();
        let mut b = interp();
        b.hypothesis = "A completely different hypothesis.".into();
        assert_ne!(
            generate_interpretation_id(&a),
            generate_interpretation_id(&b)
        );
    }

    fn input(order: &[&str]) -> StoreInterpretationInput {
        StoreInterpretationInput {
            hypothesis: "Same hypothesis.".into(),
            interpretation_type: InterpretationType::PsychologicalHypothesis,
            supported_by_fact_ids: order.iter().map(|s| s.to_string()).collect(),
            contradicted_by_fact_ids: vec![],
            confidence: 0.3,
            status: None,
            falsification_question: "Q?".into(),
            review_due: None,
        }
    }

    #[test]
    fn same_supporting_facts_different_order_same_interpretation_id() {
        // Order independence is guaranteed by validation's canonicalisation.
        let a = validate_store_interpretation(&input(&["fact_b", "fact_a"])).unwrap();
        let b = validate_store_interpretation(&input(&["fact_a", "fact_b"])).unwrap();
        assert_eq!(
            generate_interpretation_id(&a),
            generate_interpretation_id(&b)
        );
    }
}
