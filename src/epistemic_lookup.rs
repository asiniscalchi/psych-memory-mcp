//! Typed read-through lookup of a single epistemic record by its id.
//!
//! The id prefix routes to an expected type + lookup tag; each candidate is
//! validated by deserializing its metadata into a typed record and
//! cross-checking id / schema_version / epistemic_status / numeric ranges. A
//! failing candidate yields a [`RecordValidationReason`] so the invalid-match
//! case can report the most specific error (id mismatch > schema mismatch >
//! generic invalid).

use crate::backend::{MemoryBackend, MemoryRecord};
use crate::errors::{PsychMemoryError, ValidationError};
use crate::model::interpretation::SCHEMA_VERSION as INTERP_SV;
use crate::model::journal_fact::SCHEMA_VERSION as FACT_SV;
use crate::model::pattern_occurrence::SCHEMA_VERSION as OCC_SV;
use crate::model::pattern_seed::SCHEMA_VERSION as SEED_SV;
use crate::model::{
    EpistemicRecord, EpistemicRecordType, GetEpistemicRecordOutput, InterpretationRecord,
    JournalFactRecord, PatternOccurrenceRecord, PatternSeedRecord,
};
use crate::validators::is_yyyy_mm_dd;

/// Why a candidate record failed type validation. Ordered by precedence:
/// `IdMismatch` is the most specific (a record almost matching but pointing at
/// the wrong id is the most dangerous), then `SchemaMismatch`, then the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordValidationReason {
    IdMismatch,
    SchemaMismatch,
    InvalidRecord,
}

fn has_tag(memory: &MemoryRecord, tag: &str) -> bool {
    memory.tags.iter().any(|t| t == tag)
}

fn in_unit_range(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

/// Route an id to its expected `(record_type, lookup_tag)`.
pub fn route_epistemic_id(id: &str) -> Result<(EpistemicRecordType, String), ValidationError> {
    if id.trim().is_empty() {
        return Err(ValidationError::MissingEpistemicId);
    }
    if id.starts_with("fact_") {
        Ok((EpistemicRecordType::JournalFact, format!("fact_id:{id}")))
    } else if id.starts_with("interp_") {
        Ok((
            EpistemicRecordType::Interpretation,
            format!("interpretation_id:{id}"),
        ))
    } else if id.starts_with("pattern_") {
        Ok((EpistemicRecordType::PatternSeed, format!("pattern_id:{id}")))
    } else if id.starts_with("occ_") {
        Ok((
            EpistemicRecordType::PatternOccurrence,
            format!("occurrence_id:{id}"),
        ))
    } else {
        Err(ValidationError::UnsupportedEpistemicId)
    }
}

fn validate_journal_fact(
    memory: &MemoryRecord,
    id: &str,
) -> Result<EpistemicRecord, RecordValidationReason> {
    use RecordValidationReason::*;
    if memory.memory_type != "fact"
        || !has_tag(memory, "epistemic:fact")
        || !has_tag(memory, "source:froid")
        || !has_tag(memory, &format!("fact_id:{id}"))
    {
        return Err(InvalidRecord);
    }
    let rec: JournalFactRecord =
        serde_json::from_value(memory.metadata.clone()).map_err(|_| InvalidRecord)?;
    if rec.fact_id != id {
        return Err(IdMismatch);
    }
    if rec.schema_version != FACT_SV {
        return Err(SchemaMismatch);
    }
    if rec.epistemic_status != "journal_reported" {
        return Err(InvalidRecord);
    }
    Ok(EpistemicRecord::JournalFact(rec))
}

fn validate_interpretation(
    memory: &MemoryRecord,
    id: &str,
) -> Result<EpistemicRecord, RecordValidationReason> {
    use RecordValidationReason::*;
    if memory.memory_type != "interpretation"
        || !has_tag(memory, "epistemic:interpretation")
        || !has_tag(memory, "epistemic_status:hypothesis")
        || !has_tag(memory, &format!("interpretation_id:{id}"))
    {
        return Err(InvalidRecord);
    }
    let rec: InterpretationRecord =
        serde_json::from_value(memory.metadata.clone()).map_err(|_| InvalidRecord)?;
    if rec.interpretation_id != id {
        return Err(IdMismatch);
    }
    if rec.schema_version != INTERP_SV {
        return Err(SchemaMismatch);
    }
    if rec.epistemic_status != "hypothesis" || !in_unit_range(rec.confidence) {
        return Err(InvalidRecord);
    }
    Ok(EpistemicRecord::Interpretation(rec))
}

fn validate_pattern_seed(
    memory: &MemoryRecord,
    id: &str,
) -> Result<EpistemicRecord, RecordValidationReason> {
    use RecordValidationReason::*;
    if memory.memory_type != "pattern_seed"
        || !has_tag(memory, "epistemic:pattern_seed")
        || !has_tag(memory, "status:seed")
        || !has_tag(memory, &format!("pattern_id:{id}"))
    {
        return Err(InvalidRecord);
    }
    let mut rec: PatternSeedRecord =
        serde_json::from_value(memory.metadata.clone()).map_err(|_| InvalidRecord)?;
    if rec.pattern_id != id {
        return Err(IdMismatch);
    }
    if rec.schema_version != SEED_SV {
        return Err(SchemaMismatch);
    }
    if rec.status != "seed" || rec.epistemic_status != "observation_category" {
        return Err(InvalidRecord);
    }
    // Sparse-metadata fallbacks (AC18).
    if rec.name.is_empty() {
        rec.name = id.to_string();
    }
    if rec.slug.is_empty() {
        rec.slug = id.strip_prefix("pattern_").unwrap_or(id).to_string();
    }
    Ok(EpistemicRecord::PatternSeed(rec))
}

fn validate_pattern_occurrence(
    memory: &MemoryRecord,
    id: &str,
) -> Result<EpistemicRecord, RecordValidationReason> {
    use RecordValidationReason::*;
    if memory.memory_type != "pattern_occurrence"
        || !has_tag(memory, "epistemic:pattern_occurrence")
        || !has_tag(memory, "epistemic_status:evidence_linked_occurrence")
        || !has_tag(memory, &format!("occurrence_id:{id}"))
    {
        return Err(InvalidRecord);
    }
    // Deserialization also validates the `phase` enum.
    let rec: PatternOccurrenceRecord =
        serde_json::from_value(memory.metadata.clone()).map_err(|_| InvalidRecord)?;
    if rec.occurrence_id != id {
        return Err(IdMismatch);
    }
    if rec.schema_version != OCC_SV {
        return Err(SchemaMismatch);
    }
    if rec.epistemic_status != "evidence_linked_occurrence"
        || !is_yyyy_mm_dd(&rec.occurrence_date)
        || !in_unit_range(rec.confidence)
        || rec.intensity.is_some_and(|i| !in_unit_range(i))
    {
        return Err(InvalidRecord);
    }
    Ok(EpistemicRecord::PatternOccurrence(rec))
}

fn validate_record(
    record_type: EpistemicRecordType,
    memory: &MemoryRecord,
    id: &str,
) -> Result<EpistemicRecord, RecordValidationReason> {
    match record_type {
        EpistemicRecordType::JournalFact => validate_journal_fact(memory, id),
        EpistemicRecordType::Interpretation => validate_interpretation(memory, id),
        EpistemicRecordType::PatternSeed => validate_pattern_seed(memory, id),
        EpistemicRecordType::PatternOccurrence => validate_pattern_occurrence(memory, id),
    }
}

fn rejected(err: ValidationError) -> GetEpistemicRecordOutput {
    GetEpistemicRecordOutput::Rejected {
        error_code: err.error_code().to_string(),
        message: err.to_string(),
    }
}

/// Resolve a single typed epistemic record by id. Validation/resolution
/// failures become a structured `Rejected`; only transport failures are `Err`.
pub async fn get_epistemic_record(
    backend: &dyn MemoryBackend,
    id: &str,
) -> Result<GetEpistemicRecordOutput, PsychMemoryError> {
    let (record_type, tag) = match route_epistemic_id(id) {
        Ok(route) => route,
        Err(err) => return Ok(rejected(err)),
    };

    let records = backend.find_memories_by_tag(&tag).await?;
    if records.is_empty() {
        return Ok(rejected(ValidationError::UnknownEpistemicRecord));
    }

    let mut valid: Vec<(String, EpistemicRecord)> = Vec::new();
    let mut reasons: Vec<RecordValidationReason> = Vec::new();
    for memory in &records {
        match validate_record(record_type, memory, id) {
            Ok(record) => valid.push((memory.content_hash.clone(), record)),
            Err(reason) => reasons.push(reason),
        }
    }

    match valid.len() {
        1 => {
            let (backend_memory_id, record) = valid.into_iter().next().unwrap();
            Ok(GetEpistemicRecordOutput::Found {
                record_type,
                id: id.to_string(),
                backend_memory_id: Some(backend_memory_id),
                record,
            })
        }
        0 => {
            // Most specific reason wins: id mismatch > schema mismatch > invalid.
            let err = if reasons.contains(&RecordValidationReason::IdMismatch) {
                ValidationError::EpistemicIdMismatch
            } else if reasons.contains(&RecordValidationReason::SchemaMismatch) {
                ValidationError::EpistemicSchemaMismatch
            } else {
                ValidationError::InvalidEpistemicRecord
            };
            Ok(rejected(err))
        }
        _ => Ok(rejected(ValidationError::AmbiguousEpistemicRecord)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::FakeMemoryBackend;
    use crate::model::StoreMemoryRequest;
    use serde_json::{json, Value};

    fn fact(id: &str, content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.into(),
            memory_type: "fact".into(),
            tags: vec![
                "epistemic:fact".into(),
                "source:froid".into(),
                format!("fact_id:{id}"),
            ],
            metadata: json!({
                "fact_id": id, "source_excerpt": "x", "normalized_statement": "y",
                "fact_type": "self_report", "epistemic_status": "journal_reported",
                "journal_entry_ref": {"entry_id": "e", "entry_date": "2026-06-01"},
                "event_date": Value::Null, "schema_version": FACT_SV,
            }),
        }
    }

    fn interp(id: &str, content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.into(),
            memory_type: "interpretation".into(),
            tags: vec![
                "epistemic:interpretation".into(),
                "epistemic_status:hypothesis".into(),
                format!("interpretation_id:{id}"),
            ],
            metadata: json!({
                "interpretation_id": id, "hypothesis": "h", "interpretation_type": "psychological_hypothesis",
                "epistemic_status": "hypothesis", "status": "candidate",
                "supported_by_fact_ids": ["fact_a"], "contradicted_by_fact_ids": [],
                "confidence": 0.3, "falsification_question": "q?", "review_due": Value::Null,
                "schema_version": INTERP_SV,
            }),
        }
    }

    fn seed(id: &str, content: &str, meta: Value) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.into(),
            memory_type: "pattern_seed".into(),
            tags: vec![
                "epistemic:pattern_seed".into(),
                "status:seed".into(),
                format!("pattern_id:{id}"),
            ],
            metadata: meta,
        }
    }

    fn seed_meta(id: &str) -> Value {
        json!({
            "pattern_id": id, "name": "Savior", "slug": "savior", "description": "d",
            "markers": ["m"], "counter_markers": ["c"], "aliases": [],
            "epistemic_status": "observation_category", "status": "seed", "schema_version": SEED_SV,
        })
    }

    fn occ(id: &str, content: &str, meta: Value) -> StoreMemoryRequest {
        StoreMemoryRequest {
            content: content.into(),
            memory_type: "pattern_occurrence".into(),
            tags: vec![
                "epistemic:pattern_occurrence".into(),
                "epistemic_status:evidence_linked_occurrence".into(),
                format!("occurrence_id:{id}"),
            ],
            metadata: meta,
        }
    }

    fn occ_meta(id: &str) -> Value {
        json!({
            "occurrence_id": id, "pattern_id": "pattern_savior", "fact_ids": ["fact_a"],
            "interpretation_ids": ["interp_a"], "occurrence_date": "2026-06-01",
            "phase": "recognized_before_action", "summary": "s", "confidence": 0.5, "intensity": 0.4,
            "epistemic_status": "evidence_linked_occurrence", "schema_version": OCC_SV,
        })
    }

    async fn get(reqs: Vec<StoreMemoryRequest>, id: &str) -> GetEpistemicRecordOutput {
        let backend = FakeMemoryBackend::new();
        for r in reqs {
            backend.store_memory(r).await.unwrap();
        }
        get_epistemic_record(&backend, id).await.unwrap()
    }

    fn ok_type(out: &GetEpistemicRecordOutput) -> &'static str {
        match out {
            GetEpistemicRecordOutput::Found { record_type, .. } => record_type.as_str(),
            GetEpistemicRecordOutput::Rejected { error_code, .. } => {
                panic!("rejected: {error_code}")
            }
        }
    }

    fn err(out: &GetEpistemicRecordOutput) -> &str {
        match out {
            GetEpistemicRecordOutput::Rejected { error_code, .. } => error_code,
            GetEpistemicRecordOutput::Found { .. } => panic!("expected rejection"),
        }
    }

    // --- routing ---
    #[test]
    fn rejects_empty_id() {
        assert_eq!(
            route_epistemic_id("  ").unwrap_err().error_code(),
            "missing_epistemic_id"
        );
    }
    #[test]
    fn rejects_unsupported_id_prefix() {
        assert_eq!(
            route_epistemic_id("foo_123").unwrap_err().error_code(),
            "unsupported_epistemic_id"
        );
    }
    #[test]
    fn routes_each_prefix() {
        assert_eq!(route_epistemic_id("fact_x").unwrap().1, "fact_id:fact_x");
        assert_eq!(
            route_epistemic_id("interp_x").unwrap().1,
            "interpretation_id:interp_x"
        );
        assert_eq!(
            route_epistemic_id("pattern_x").unwrap().1,
            "pattern_id:pattern_x"
        );
        assert_eq!(
            route_epistemic_id("occ_x").unwrap().1,
            "occurrence_id:occ_x"
        );
    }

    // --- happy paths ---
    #[tokio::test]
    async fn retrieves_each_type() {
        assert_eq!(
            ok_type(&get(vec![fact("fact_a", "c")], "fact_a").await),
            "journal_fact"
        );
        assert_eq!(
            ok_type(&get(vec![interp("interp_a", "c")], "interp_a").await),
            "interpretation"
        );
        assert_eq!(
            ok_type(
                &get(
                    vec![seed("pattern_a", "c", seed_meta("pattern_a"))],
                    "pattern_a"
                )
                .await
            ),
            "pattern_seed"
        );
        assert_eq!(
            ok_type(&get(vec![occ("occ_a", "c", occ_meta("occ_a"))], "occ_a").await),
            "pattern_occurrence"
        );
    }

    // --- resolver ---
    #[tokio::test]
    async fn unknown_id_returns_unknown() {
        assert_eq!(
            err(&get(vec![], "fact_missing").await),
            "unknown_epistemic_record"
        );
    }
    #[tokio::test]
    async fn ambiguous_id_returns_ambiguous() {
        let out = get(vec![fact("fact_a", "one"), fact("fact_a", "two")], "fact_a").await;
        assert_eq!(err(&out), "ambiguous_epistemic_record");
    }
    #[tokio::test]
    async fn all_invalid_returns_invalid() {
        let mut bad = fact("fact_a", "c");
        bad.memory_type = "observation".into(); // wrong type
        assert_eq!(
            err(&get(vec![bad], "fact_a").await),
            "invalid_epistemic_record"
        );
    }
    #[tokio::test]
    async fn mixed_valid_and_invalid_returns_found() {
        let mut bad = fact("fact_a", "corrupt");
        bad.metadata["schema_version"] = json!("wrong");
        let out = get(vec![fact("fact_a", "valid"), bad], "fact_a").await;
        assert_eq!(ok_type(&out), "journal_fact");
    }

    // --- precedence ---
    #[tokio::test]
    async fn id_mismatch_takes_precedence_over_schema_mismatch() {
        let mut id_bad = fact("fact_a", "one");
        id_bad.metadata["fact_id"] = json!("fact_other"); // id mismatch
        let mut schema_bad = fact("fact_a", "two");
        schema_bad.metadata["schema_version"] = json!("wrong"); // schema mismatch
        let out = get(vec![id_bad, schema_bad], "fact_a").await;
        assert_eq!(err(&out), "epistemic_id_mismatch");
    }
    #[tokio::test]
    async fn schema_mismatch_takes_precedence_over_invalid_record() {
        let mut schema_bad = fact("fact_a", "one");
        schema_bad.metadata["schema_version"] = json!("wrong");
        let mut invalid = fact("fact_a", "two");
        invalid.metadata["epistemic_status"] = json!("nonsense"); // generic invalid
        let out = get(vec![schema_bad, invalid], "fact_a").await;
        assert_eq!(err(&out), "epistemic_schema_mismatch");
    }
    #[tokio::test]
    async fn id_mismatch_alone() {
        let mut bad = fact("fact_a", "c");
        bad.metadata["fact_id"] = json!("fact_other");
        assert_eq!(
            err(&get(vec![bad], "fact_a").await),
            "epistemic_id_mismatch"
        );
    }
    #[tokio::test]
    async fn schema_mismatch_alone() {
        let mut bad = fact("fact_a", "c");
        bad.metadata["schema_version"] = json!("wrong");
        assert_eq!(
            err(&get(vec![bad], "fact_a").await),
            "epistemic_schema_mismatch"
        );
    }

    // --- serde / sparse / optional ---
    #[tokio::test]
    async fn pattern_seed_sparse_name_slug_fallback() {
        let meta = json!({
            "pattern_id": "pattern_savior", "description": "d",
            "epistemic_status": "observation_category", "status": "seed", "schema_version": SEED_SV,
        });
        let out = get(vec![seed("pattern_savior", "c", meta)], "pattern_savior").await;
        match out {
            GetEpistemicRecordOutput::Found { record, .. } => {
                let v = serde_json::to_value(&record).unwrap();
                assert_eq!(v["name"], "pattern_savior");
                assert_eq!(v["slug"], "savior");
                assert_eq!(v["aliases"], json!([]));
            }
            other => panic!("expected Found: {other:?}"),
        }
    }
    #[tokio::test]
    async fn occurrence_defaults_optional_fields() {
        let meta = json!({
            "occurrence_id": "occ_a", "pattern_id": "p", "fact_ids": ["fact_a"],
            "occurrence_date": "2026-06-01", "phase": "activated", "summary": "s", "confidence": 0.5,
            "epistemic_status": "evidence_linked_occurrence", "schema_version": OCC_SV,
        });
        let out = get(vec![occ("occ_a", "c", meta)], "occ_a").await;
        match out {
            GetEpistemicRecordOutput::Found { record, .. } => {
                let v = serde_json::to_value(&record).unwrap();
                assert_eq!(v["interpretation_ids"], json!([]));
                assert_eq!(v["intensity"], Value::Null);
            }
            other => panic!("expected Found: {other:?}"),
        }
    }
    #[tokio::test]
    async fn rejects_malformed_metadata() {
        let mut bad = fact("fact_a", "c");
        bad.metadata = json!({ "fact_id": "fact_a" }); // missing required fields
        assert_eq!(
            err(&get(vec![bad], "fact_a").await),
            "invalid_epistemic_record"
        );
    }
    #[tokio::test]
    async fn rejects_invalid_phase_enum() {
        let mut meta = occ_meta("occ_a");
        meta["phase"] = json!("bogus_phase");
        assert_eq!(
            err(&get(vec![occ("occ_a", "c", meta)], "occ_a").await),
            "invalid_epistemic_record"
        );
    }
    #[tokio::test]
    async fn rejects_invalid_interpretation_type_enum() {
        let mut bad = interp("interp_a", "c");
        bad.metadata["interpretation_type"] = json!("not_a_type");
        assert_eq!(
            err(&get(vec![bad], "interp_a").await),
            "invalid_epistemic_record"
        );
    }
    #[tokio::test]
    async fn rejects_occurrence_out_of_range_confidence_and_intensity() {
        let mut c = occ_meta("occ_a");
        c["confidence"] = json!(5.0);
        assert_eq!(
            err(&get(vec![occ("occ_a", "c", c)], "occ_a").await),
            "invalid_epistemic_record"
        );
        let mut i = occ_meta("occ_b");
        i["intensity"] = json!(9.0);
        assert_eq!(
            err(&get(vec![occ("occ_b", "c", i)], "occ_b").await),
            "invalid_epistemic_record"
        );
    }
    #[tokio::test]
    async fn rejects_missing_required_tag() {
        let mut bad = fact("fact_a", "c");
        bad.tags.retain(|t| t != "source:froid"); // missing required tag
        assert_eq!(
            err(&get(vec![bad], "fact_a").await),
            "invalid_epistemic_record"
        );
    }
}
