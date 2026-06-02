//! Read-only construction of a pattern occurrence timeline.
//!
//! `build_pattern_timeline` is a pure function: given the records returned by a
//! single `pattern_id:<id>` tag lookup and a validated query, it resolves the
//! pattern seed, validates/dedups/filters/sorts/groups the occurrences, and
//! produces descriptive output. It never writes and never concludes.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::backend::MemoryRecord;
use crate::errors::ValidationError;
use crate::evidence::is_valid_pattern_seed;
use crate::model::pattern_occurrence::SCHEMA_VERSION as OCC_SCHEMA_VERSION;
use crate::model::pattern_timeline::{
    PatternTimelineDateGroup, PatternTimelineFilters, PatternTimelineOccurrence,
    PatternTimelinePattern, PatternTimelineWarning, PhaseCounts, QueryPatternTimelineOutput,
};
use crate::model::{OccurrencePhase, ValidatedPatternTimelineQuery};
use crate::validators::is_yyyy_mm_dd;

fn has_tag(memory: &MemoryRecord, tag: &str) -> bool {
    memory.tags.iter().any(|t| t == tag)
}

fn meta_str<'a>(memory: &'a MemoryRecord, key: &str) -> Option<&'a str> {
    memory.metadata.get(key).and_then(Value::as_str)
}

/// Read an optional string-array metadata field. Absent/null -> empty list;
/// present-but-not-an-array-of-strings -> `None` (the record is invalid).
fn meta_str_array(memory: &MemoryRecord, key: &str) -> Option<Vec<String>> {
    match memory.metadata.get(key) {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => None,
    }
}

/// Validate a tag-matched record as a `PatternTimelineOccurrence`, or `None` if
/// it is not a structurally valid occurrence for `pattern_id`.
fn validate_occurrence_record(
    memory: &MemoryRecord,
    pattern_id: &str,
) -> Option<PatternTimelineOccurrence> {
    if memory.memory_type != "pattern_occurrence"
        || !has_tag(memory, "epistemic:pattern_occurrence")
        || !has_tag(memory, "epistemic_status:evidence_linked_occurrence")
        || !has_tag(memory, &format!("pattern_id:{pattern_id}"))
    {
        return None;
    }
    if meta_str(memory, "schema_version") != Some(OCC_SCHEMA_VERSION) {
        return None;
    }
    if meta_str(memory, "pattern_id") != Some(pattern_id) {
        return None;
    }

    let occurrence_id = meta_str(memory, "occurrence_id")?.to_string();
    if !has_tag(memory, &format!("occurrence_id:{occurrence_id}")) {
        return None;
    }

    let occurrence_date = meta_str(memory, "occurrence_date")?.to_string();
    if !is_yyyy_mm_dd(&occurrence_date) {
        return None;
    }

    let phase: OccurrencePhase = serde_json::from_value(json!(meta_str(memory, "phase")?)).ok()?;
    let summary = meta_str(memory, "summary")?.to_string();

    let confidence = memory.metadata.get("confidence").and_then(Value::as_f64)?;
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return None;
    }

    let intensity = match memory.metadata.get("intensity") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let f = value.as_f64()?;
            if !f.is_finite() || !(0.0..=1.0).contains(&f) {
                return None;
            }
            Some(f as f32)
        }
    };

    Some(PatternTimelineOccurrence {
        occurrence_id,
        pattern_id: pattern_id.to_string(),
        occurrence_date,
        phase,
        summary,
        confidence: confidence as f32,
        intensity,
        fact_ids: meta_str_array(memory, "fact_ids")?,
        interpretation_ids: meta_str_array(memory, "interpretation_ids")?,
    })
}

fn build_pattern_meta(seed: &MemoryRecord, pattern_id: &str) -> PatternTimelinePattern {
    PatternTimelinePattern {
        pattern_id: pattern_id.to_string(),
        name: meta_str(seed, "name")
            .map(str::to_string)
            .unwrap_or_else(|| pattern_id.to_string()),
        slug: meta_str(seed, "slug")
            .map(str::to_string)
            .unwrap_or_else(|| {
                pattern_id
                    .strip_prefix("pattern_")
                    .unwrap_or(pattern_id)
                    .to_string()
            }),
        status: meta_str(seed, "status")
            .map(str::to_string)
            .unwrap_or_else(|| "seed".to_string()),
    }
}

fn reject(err: ValidationError) -> QueryPatternTimelineOutput {
    QueryPatternTimelineOutput::Rejected {
        error_code: err.error_code().to_string(),
        message: err.to_string(),
    }
}

/// Build the descriptive timeline from one tag-lookup result set.
pub fn build_pattern_timeline(
    records: Vec<MemoryRecord>,
    query: &ValidatedPatternTimelineQuery,
) -> QueryPatternTimelineOutput {
    let pattern_id = query.pattern_id.as_str();

    // Resolve the seed from the same snapshot.
    let valid_seeds: Vec<&MemoryRecord> = records
        .iter()
        .filter(|m| is_valid_pattern_seed(m, pattern_id))
        .collect();
    let seed = match valid_seeds.len() {
        1 => valid_seeds[0],
        0 => {
            let seed_like = records.iter().any(|m| m.memory_type == "pattern_seed");
            return reject(if seed_like {
                ValidationError::InvalidPatternSeedMatch
            } else {
                ValidationError::UnknownPatternSeed
            });
        }
        _ => return reject(ValidationError::AmbiguousPatternSeed),
    };
    let pattern = build_pattern_meta(seed, pattern_id);

    let mut warnings: Vec<PatternTimelineWarning> = Vec::new();
    // (backend_memory_id, occurrence) so dedup can keep a deterministic winner.
    let mut candidates: Vec<(String, PatternTimelineOccurrence)> = Vec::new();

    for memory in &records {
        if is_valid_pattern_seed(memory, pattern_id) {
            continue; // the seed itself: metadata source, never an occurrence
        }
        match validate_occurrence_record(memory, pattern_id) {
            Some(occ) => candidates.push((memory.content_hash.clone(), occ)),
            None => {
                if memory.memory_type == "pattern_seed" {
                    continue; // a corrupt seed-like record; not an occurrence
                }
                if query.include_invalid_warnings {
                    let (code, message) = if memory.memory_type == "pattern_occurrence" {
                        (
                            "invalid_occurrence_record",
                            "A record looked like a pattern occurrence but failed validation.",
                        )
                    } else {
                        (
                            "unexpected_pattern_id_tag_match",
                            "A record carried the pattern_id tag but was neither the pattern seed \
                             nor a valid occurrence.",
                        )
                    };
                    warnings.push(PatternTimelineWarning {
                        warning_code: code.to_string(),
                        message: message.to_string(),
                        backend_memory_id: Some(memory.content_hash.clone()),
                    });
                }
            }
        }
    }

    // Deduplicate by occurrence_id, keeping the lowest backend_memory_id.
    candidates.sort_by(|a, b| {
        a.1.occurrence_id
            .cmp(&b.1.occurrence_id)
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut warned_dups: HashSet<String> = HashSet::new();
    let mut deduped: Vec<PatternTimelineOccurrence> = Vec::new();
    for (backend_memory_id, occ) in candidates {
        if !seen_ids.insert(occ.occurrence_id.clone()) {
            if query.include_invalid_warnings && warned_dups.insert(occ.occurrence_id.clone()) {
                warnings.push(PatternTimelineWarning {
                    warning_code: "duplicate_occurrence_id".to_string(),
                    message: "Multiple valid occurrence records had the same occurrence_id; only \
                              one was included."
                        .to_string(),
                    backend_memory_id: Some(backend_memory_id),
                });
            }
            continue;
        }
        deduped.push(occ);
    }

    // Filter by date range and phase.
    let mut filtered: Vec<PatternTimelineOccurrence> = deduped
        .into_iter()
        .filter(|o| {
            if let Some(from) = &query.date_from {
                if o.occurrence_date < *from {
                    return false;
                }
            }
            if let Some(to) = &query.date_to {
                if o.occurrence_date > *to {
                    return false;
                }
            }
            if !query.phases.is_empty() && !query.phases.contains(&o.phase) {
                return false;
            }
            true
        })
        .collect();

    // Sort: date asc, phase lexical, occurrence_id asc.
    filtered.sort_by(|a, b| {
        a.occurrence_date
            .cmp(&b.occurrence_date)
            .then_with(|| a.phase.as_str().cmp(b.phase.as_str()))
            .then_with(|| a.occurrence_id.cmp(&b.occurrence_id))
    });

    let mut phase_counts = PhaseCounts::default();
    for o in &filtered {
        phase_counts.increment(o.phase);
    }
    let total_occurrences = filtered.len();

    // Group by date (input already date-sorted).
    let mut timeline: Vec<PatternTimelineDateGroup> = Vec::new();
    for o in filtered {
        match timeline.last_mut() {
            Some(group) if group.date == o.occurrence_date => group.occurrences.push(o),
            _ => timeline.push(PatternTimelineDateGroup {
                date: o.occurrence_date.clone(),
                occurrences: vec![o],
            }),
        }
    }

    QueryPatternTimelineOutput::Found {
        pattern_id: pattern_id.to_string(),
        pattern,
        filters: PatternTimelineFilters {
            date_from: query.date_from.clone(),
            date_to: query.date_to.clone(),
            phases: query.phases.clone(),
        },
        total_occurrences,
        phase_counts,
        timeline,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pattern_occurrence::SCHEMA_VERSION as OCC_SV;
    use crate::model::pattern_seed::SCHEMA_VERSION as SEED_SV;

    fn query(pattern_id: &str) -> ValidatedPatternTimelineQuery {
        ValidatedPatternTimelineQuery {
            pattern_id: pattern_id.into(),
            date_from: None,
            date_to: None,
            phases: vec![],
            include_invalid_warnings: true,
        }
    }

    fn seed(pattern_id: &str) -> MemoryRecord {
        MemoryRecord {
            content: "Savior — desc".into(),
            memory_type: "pattern_seed".into(),
            tags: vec![
                "epistemic:pattern_seed".into(),
                "status:seed".into(),
                format!("pattern_id:{pattern_id}"),
            ],
            content_hash: format!("seed_{pattern_id}"),
            metadata: json!({
                "pattern_id": pattern_id, "name": "Savior", "slug": "savior",
                "status": "seed", "schema_version": SEED_SV,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn occ(
        hash: &str,
        pattern_id: &str,
        occ_id: &str,
        date: &str,
        phase: &str,
        facts: Vec<&str>,
        interps: Vec<&str>,
    ) -> MemoryRecord {
        MemoryRecord {
            content: format!("summary {occ_id}"),
            memory_type: "pattern_occurrence".into(),
            tags: vec![
                "epistemic:pattern_occurrence".into(),
                "epistemic_status:evidence_linked_occurrence".into(),
                format!("pattern_id:{pattern_id}"),
                format!("occurrence_id:{occ_id}"),
                format!("phase:{phase}"),
            ],
            content_hash: hash.into(),
            metadata: json!({
                "occurrence_id": occ_id, "pattern_id": pattern_id, "occurrence_date": date,
                "phase": phase, "summary": format!("summary {occ_id}"), "confidence": 0.5,
                "fact_ids": facts, "interpretation_ids": interps,
                "schema_version": OCC_SV,
            }),
        }
    }

    fn found(
        out: QueryPatternTimelineOutput,
    ) -> (
        usize,
        Vec<PatternTimelineDateGroup>,
        Vec<PatternTimelineWarning>,
        PhaseCounts,
        PatternTimelinePattern,
    ) {
        match out {
            QueryPatternTimelineOutput::Found {
                total_occurrences,
                timeline,
                warnings,
                phase_counts,
                pattern,
                ..
            } => (total_occurrences, timeline, warnings, phase_counts, pattern),
            QueryPatternTimelineOutput::Rejected { error_code, .. } => {
                panic!("rejected: {error_code}")
            }
        }
    }

    fn reject_code(out: QueryPatternTimelineOutput) -> String {
        match out {
            QueryPatternTimelineOutput::Rejected { error_code, .. } => error_code,
            QueryPatternTimelineOutput::Found { .. } => panic!("expected rejection"),
        }
    }

    #[test]
    fn returns_empty_timeline_for_existing_pattern_with_no_occurrences() {
        let out = build_pattern_timeline(vec![seed("pattern_savior")], &query("pattern_savior"));
        let (total, timeline, warnings, counts, pattern) = found(out);
        assert_eq!(total, 0);
        assert!(timeline.is_empty());
        assert!(warnings.is_empty());
        assert_eq!(counts, PhaseCounts::default());
        assert_eq!(pattern.name, "Savior");
        assert_eq!(pattern.status, "seed");
    }

    #[test]
    fn rejects_unknown_pattern_seed() {
        let out = build_pattern_timeline(vec![], &query("pattern_x"));
        assert_eq!(reject_code(out), "unknown_pattern_seed");
    }

    #[test]
    fn rejects_invalid_pattern_seed_match() {
        // A pattern_seed record carrying the tag but with mismatched metadata.
        let mut bad = seed("pattern_x");
        bad.metadata = json!({ "pattern_id": "pattern_OTHER", "schema_version": SEED_SV });
        let out = build_pattern_timeline(vec![bad], &query("pattern_x"));
        assert_eq!(reject_code(out), "invalid_pattern_seed_match");
    }

    #[test]
    fn rejects_ambiguous_pattern_seed() {
        let out = build_pattern_timeline(
            vec![seed("pattern_x"), seed("pattern_x")],
            &query("pattern_x"),
        );
        assert_eq!(reject_code(out), "ambiguous_pattern_seed");
    }

    #[test]
    fn excludes_pattern_seed_record_from_occurrences() {
        let out = build_pattern_timeline(
            vec![
                seed("pattern_x"),
                occ(
                    "h1",
                    "pattern_x",
                    "occ_a",
                    "2026-06-01",
                    "activated",
                    vec![],
                    vec![],
                ),
            ],
            &query("pattern_x"),
        );
        let (total, _, warnings, ..) = found(out);
        assert_eq!(total, 1);
        assert!(warnings.is_empty(), "seed must not warn");
    }

    #[test]
    fn sorts_and_groups_by_date_then_phase_then_id() {
        let records = vec![
            seed("p"),
            occ(
                "h1",
                "p",
                "occ_z",
                "2026-06-02",
                "activated",
                vec![],
                vec![],
            ),
            occ(
                "h2",
                "p",
                "occ_a",
                "2026-06-01",
                "transformed",
                vec![],
                vec![],
            ),
            occ(
                "h3",
                "p",
                "occ_b",
                "2026-06-01",
                "activated",
                vec![],
                vec![],
            ),
        ];
        let (total, timeline, ..) = found(build_pattern_timeline(records, &query("p")));
        assert_eq!(total, 3);
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].date, "2026-06-01");
        // within 2026-06-01: phase "activated" < "transformed"
        assert_eq!(timeline[0].occurrences[0].occurrence_id, "occ_b");
        assert_eq!(timeline[0].occurrences[1].occurrence_id, "occ_a");
        assert_eq!(timeline[1].date, "2026-06-02");
    }

    #[test]
    fn filters_by_date_range() {
        let records = vec![
            seed("p"),
            occ("h1", "p", "o1", "2026-05-31", "activated", vec![], vec![]),
            occ("h2", "p", "o2", "2026-06-15", "activated", vec![], vec![]),
            occ("h3", "p", "o3", "2026-07-01", "activated", vec![], vec![]),
        ];
        let mut q = query("p");
        q.date_from = Some("2026-06-01".into());
        q.date_to = Some("2026-06-30".into());
        let (total, timeline, ..) = found(build_pattern_timeline(records, &q));
        assert_eq!(total, 1);
        assert_eq!(timeline[0].occurrences[0].occurrence_id, "o2");
    }

    #[test]
    fn filters_by_phase() {
        let records = vec![
            seed("p"),
            occ("h1", "p", "o1", "2026-06-01", "activated", vec![], vec![]),
            occ(
                "h2",
                "p",
                "o2",
                "2026-06-02",
                "not_activated",
                vec![],
                vec![],
            ),
        ];
        let mut q = query("p");
        q.phases = vec![OccurrencePhase::NotActivated];
        let (total, timeline, _, counts, _) = found(build_pattern_timeline(records, &q));
        assert_eq!(total, 1);
        assert_eq!(
            timeline[0].occurrences[0].phase,
            OccurrencePhase::NotActivated
        );
        assert_eq!(counts.not_activated, 1);
        assert_eq!(counts.activated, 0);
    }

    #[test]
    fn computes_phase_counts_for_all_phases() {
        let records = vec![
            seed("p"),
            occ("h1", "p", "o1", "2026-06-01", "activated", vec![], vec![]),
            occ(
                "h2",
                "p",
                "o2",
                "2026-06-02",
                "not_activated",
                vec![],
                vec![],
            ),
        ];
        let (_, _, _, counts, _) = found(build_pattern_timeline(records, &query("p")));
        assert_eq!(counts.activated, 1);
        assert_eq!(counts.not_activated, 1);
        assert_eq!(counts.inhibited, 0);
        assert_eq!(counts.transformed, 0);
    }

    #[test]
    fn excludes_and_warns_on_invalid_occurrence_record() {
        let mut bad = occ("h1", "p", "o1", "2026-06-01", "activated", vec![], vec![]);
        bad.metadata["confidence"] = json!(5.0); // out of range -> invalid
        let out = build_pattern_timeline(vec![seed("p"), bad], &query("p"));
        let (total, _, warnings, ..) = found(out);
        assert_eq!(total, 0);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].warning_code, "invalid_occurrence_record");
    }

    #[test]
    fn suppresses_invalid_occurrence_warning_when_requested() {
        let mut bad = occ("h1", "p", "o1", "2026-06-01", "activated", vec![], vec![]);
        bad.metadata["confidence"] = json!(5.0);
        let mut q = query("p");
        q.include_invalid_warnings = false;
        let (total, _, warnings, ..) = found(build_pattern_timeline(vec![seed("p"), bad], &q));
        assert_eq!(total, 0);
        assert!(warnings.is_empty());
    }

    #[test]
    fn unexpected_record_warns_when_enabled() {
        let mut weird = occ("h1", "p", "o1", "2026-06-01", "activated", vec![], vec![]);
        weird.memory_type = "observation".into(); // carries pattern_id tag, not occurrence/seed
        let (total, _, warnings, ..) =
            found(build_pattern_timeline(vec![seed("p"), weird], &query("p")));
        assert_eq!(total, 0);
        assert_eq!(warnings[0].warning_code, "unexpected_pattern_id_tag_match");
    }

    #[test]
    fn deduplicates_duplicate_occurrence_id_with_warning() {
        let records = vec![
            seed("p"),
            occ("hb", "p", "dup", "2026-06-01", "activated", vec![], vec![]),
            occ("ha", "p", "dup", "2026-06-01", "activated", vec![], vec![]),
        ];
        let (total, _, warnings, ..) = found(build_pattern_timeline(records, &query("p")));
        assert_eq!(total, 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].warning_code, "duplicate_occurrence_id");
    }

    #[test]
    fn preserves_fact_ids_and_interpretation_ids() {
        let records = vec![
            seed("p"),
            occ(
                "h1",
                "p",
                "o1",
                "2026-06-01",
                "activated",
                vec!["fact_a", "fact_b"],
                vec!["interp_z"],
            ),
        ];
        let (_, timeline, ..) = found(build_pattern_timeline(records, &query("p")));
        let o = &timeline[0].occurrences[0];
        assert_eq!(o.fact_ids, vec!["fact_a".to_string(), "fact_b".to_string()]);
        assert_eq!(o.interpretation_ids, vec!["interp_z".to_string()]);
    }

    #[test]
    fn pattern_output_falls_back_when_name_or_slug_missing() {
        let mut s = seed("pattern_savior");
        s.metadata = json!({ "pattern_id": "pattern_savior", "schema_version": SEED_SV });
        let (_, _, _, _, pattern) =
            found(build_pattern_timeline(vec![s], &query("pattern_savior")));
        assert_eq!(pattern.name, "pattern_savior"); // fallback to pattern_id
        assert_eq!(pattern.slug, "savior"); // stripped prefix
        assert_eq!(pattern.status, "seed"); // default
    }

    #[test]
    fn output_contains_no_trend_or_activation_fields() {
        let out = build_pattern_timeline(vec![seed("p")], &query("p"));
        let value = serde_json::to_value(&out).unwrap();
        let obj = value.as_object().unwrap();
        for forbidden in [
            "trend",
            "active",
            "active_since",
            "last_seen",
            "conclusion",
            "psychological_summary",
        ] {
            assert!(!obj.contains_key(forbidden), "output has {forbidden}");
        }
        assert_eq!(obj["ok"], json!(true));
    }
}
