//! The MCP server surface.
//!
//! Only explicit epistemic tools are exposed — there is no generic
//! `store_memory` / `save_memory` / `remember`. Story 1 adds the first tool,
//! `store_journal_fact`.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};

use crate::backend::MemoryBackend;
use crate::epistemic_lookup::get_epistemic_record;
use crate::errors::{PsychMemoryError, ValidationError};
use crate::evidence::{
    resolve_linked_interpretations, resolve_pattern_seed_by_pattern_id, resolve_supporting_facts,
};
use crate::fact_id::generate_fact_id;
use crate::interpretation_id::generate_interpretation_id;
use crate::mapping::{
    map_create_pattern_seed_to_backend_request, map_record_pattern_occurrence_to_backend_request,
    map_store_interpretation_to_backend_request, map_store_journal_fact_to_backend_request,
};
use crate::model::{
    CreatePatternSeedInput, CreatePatternSeedOutput, GetEpistemicRecordInput,
    GetEpistemicRecordOutput, QueryPatternTimelineInput, QueryPatternTimelineOutput,
    RecordPatternOccurrenceInput, RecordPatternOccurrenceOutput, StoreInterpretationInput,
    StoreInterpretationOutput, StoreJournalFactInput, StoreJournalFactOutput,
};
use crate::occurrence_id::generate_occurrence_id;
use crate::pattern_id::generate_pattern_id;
use crate::pattern_validation::validate_create_pattern_seed;
use crate::resolution::TypedLookup;
use crate::timeline::build_pattern_timeline;
use crate::validators::{
    validate_query_pattern_timeline, validate_record_pattern_occurrence,
    validate_store_interpretation, validate_store_journal_fact,
};

#[derive(Clone)]
pub struct PsychMemoryServer {
    backend: Arc<dyn MemoryBackend>,
    tool_router: ToolRouter<Self>,
}

impl PsychMemoryServer {
    pub fn new(backend: Arc<dyn MemoryBackend>) -> Self {
        Self {
            backend,
            tool_router: Self::tool_router(),
        }
    }

    /// The `store_journal_fact` flow, independent of the MCP envelope.
    ///
    /// Returns a structured outcome: validation failures become
    /// [`StoreJournalFactOutput::Rejected`] (a tool-level error), while a
    /// backend failure is propagated as `Err` (an infrastructure problem).
    pub async fn store_journal_fact_flow(
        &self,
        input: StoreJournalFactInput,
    ) -> Result<StoreJournalFactOutput, PsychMemoryError> {
        let validated = match validate_store_journal_fact(&input) {
            Ok(v) => v,
            Err(err) => {
                return Ok(StoreJournalFactOutput::Rejected {
                    error_code: err.error_code().to_string(),
                    message: err.to_string(),
                });
            }
        };

        let fact_id = generate_fact_id(&validated);
        let request = map_store_journal_fact_to_backend_request(&validated, &fact_id);
        let stored = self.backend.store_memory(request).await?;

        Ok(StoreJournalFactOutput::Stored {
            fact_id,
            backend_memory_id: Some(stored.backend_memory_id),
            status: "stored".to_string(),
        })
    }

    /// The `store_interpretation` flow, independent of the MCP envelope.
    ///
    /// Both shape validation and evidence-resolution failures are domain
    /// validation errors and become [`StoreInterpretationOutput::Rejected`];
    /// only backend/transport failures propagate as `Err`.
    pub async fn store_interpretation_flow(
        &self,
        input: StoreInterpretationInput,
    ) -> Result<StoreInterpretationOutput, PsychMemoryError> {
        let validated = match validate_store_interpretation(&input) {
            Ok(v) => v,
            Err(err) => return Ok(rejected_interpretation(&err)),
        };

        // Resolution returns a ValidationError (unknown/ambiguous/invalid fact)
        // as a structured rejection; a transport failure propagates as Err.
        if let Err(err) =
            resolve_supporting_facts(self.backend.as_ref(), &validated.supported_by_fact_ids).await
        {
            return match err {
                PsychMemoryError::Validation(v) => Ok(rejected_interpretation(&v)),
                other => Err(other),
            };
        }

        let interpretation_id = generate_interpretation_id(&validated);
        let request = map_store_interpretation_to_backend_request(&validated, &interpretation_id);
        let stored = self.backend.store_memory(request).await?;

        Ok(StoreInterpretationOutput::Stored {
            interpretation_id,
            backend_memory_id: Some(stored.backend_memory_id),
            status: "stored".to_string(),
        })
    }

    /// The `create_pattern_seed` flow, independent of the MCP envelope.
    ///
    /// Idempotent by `pattern_id`: an existing valid seed returns
    /// `AlreadyExists` without storing; ambiguous or all-invalid matches become
    /// structured rejections. Only backend failures propagate as `Err`.
    pub async fn create_pattern_seed_flow(
        &self,
        input: CreatePatternSeedInput,
    ) -> Result<CreatePatternSeedOutput, PsychMemoryError> {
        let validated = match validate_create_pattern_seed(&input) {
            Ok(v) => v,
            Err(err) => return Ok(rejected_pattern(&err)),
        };

        let pattern_id = generate_pattern_id(&validated);

        match resolve_pattern_seed_by_pattern_id(self.backend.as_ref(), &pattern_id).await? {
            TypedLookup::NotFound => {}
            TypedLookup::Found(existing) => {
                return Ok(CreatePatternSeedOutput::AlreadyExists {
                    pattern_id,
                    backend_memory_id: Some(existing.content_hash),
                });
            }
            TypedLookup::Ambiguous(_) => {
                return Ok(rejected_pattern(&ValidationError::AmbiguousPatternSeed));
            }
            TypedLookup::InvalidMatch(_) => {
                return Ok(rejected_pattern(&ValidationError::InvalidPatternSeedMatch));
            }
        }

        let request = map_create_pattern_seed_to_backend_request(&validated, &pattern_id);
        let stored = self.backend.store_memory(request).await?;

        Ok(CreatePatternSeedOutput::Stored {
            pattern_id,
            backend_memory_id: Some(stored.backend_memory_id),
        })
    }

    /// The `record_pattern_occurrence` flow, independent of the MCP envelope.
    ///
    /// Resolves the pattern seed, every supporting fact, and any linked
    /// interpretation before storing. Validation/resolution failures become
    /// structured rejections; backend failures propagate as `Err`. The
    /// PatternSeed is only read, never mutated.
    pub async fn record_pattern_occurrence_flow(
        &self,
        input: RecordPatternOccurrenceInput,
    ) -> Result<RecordPatternOccurrenceOutput, PsychMemoryError> {
        let validated = match validate_record_pattern_occurrence(&input) {
            Ok(v) => v,
            Err(err) => return Ok(rejected_occurrence(&err)),
        };

        match resolve_pattern_seed_by_pattern_id(self.backend.as_ref(), &validated.pattern_id)
            .await?
        {
            TypedLookup::Found(_) => {}
            TypedLookup::NotFound => {
                return Ok(rejected_occurrence(&ValidationError::UnknownPatternSeed));
            }
            TypedLookup::Ambiguous(_) => {
                return Ok(rejected_occurrence(&ValidationError::AmbiguousPatternSeed));
            }
            TypedLookup::InvalidMatch(_) => {
                return Ok(rejected_occurrence(
                    &ValidationError::InvalidPatternSeedMatch,
                ));
            }
        }

        if let Some(rejection) = resolve_or_reject(
            resolve_supporting_facts(self.backend.as_ref(), &validated.fact_ids).await,
        )? {
            return Ok(rejection);
        }
        if let Some(rejection) = resolve_or_reject(
            resolve_linked_interpretations(self.backend.as_ref(), &validated.interpretation_ids)
                .await,
        )? {
            return Ok(rejection);
        }

        let occurrence_id = generate_occurrence_id(&validated);
        let request = map_record_pattern_occurrence_to_backend_request(&validated, &occurrence_id);
        let stored = self.backend.store_memory(request).await?;

        Ok(RecordPatternOccurrenceOutput::Stored {
            occurrence_id,
            backend_memory_id: Some(stored.backend_memory_id),
            status: "stored".to_string(),
        })
    }

    /// The `query_pattern_timeline` flow, independent of the MCP envelope.
    ///
    /// Read-only: validate the query, do one `pattern_id` tag lookup, and build
    /// a descriptive timeline. Never writes. Only a backend/transport failure
    /// propagates as `Err`.
    pub async fn query_pattern_timeline_flow(
        &self,
        input: QueryPatternTimelineInput,
    ) -> Result<QueryPatternTimelineOutput, PsychMemoryError> {
        let query = match validate_query_pattern_timeline(&input) {
            Ok(q) => q,
            Err(err) => {
                return Ok(QueryPatternTimelineOutput::Rejected {
                    error_code: err.error_code().to_string(),
                    message: err.to_string(),
                });
            }
        };

        // One lookup: resolves the seed and finds occurrence candidates from the
        // same snapshot. find_memories_by_tag exhausts pagination.
        let tag = format!("pattern_id:{}", query.pattern_id);
        let records = self.backend.find_memories_by_tag(&tag).await?;

        Ok(build_pattern_timeline(records, &query))
    }

    /// The `get_epistemic_record` flow, independent of the MCP envelope.
    /// Read-only typed read-through by epistemic id.
    pub async fn get_epistemic_record_flow(
        &self,
        input: GetEpistemicRecordInput,
    ) -> Result<GetEpistemicRecordOutput, PsychMemoryError> {
        get_epistemic_record(self.backend.as_ref(), &input.id).await
    }
}

/// Turn a list-resolver result into either a structured rejection (domain
/// validation error) or `None` (success), propagating transport errors as `Err`.
fn resolve_or_reject<T>(
    result: Result<T, PsychMemoryError>,
) -> Result<Option<RecordPatternOccurrenceOutput>, PsychMemoryError> {
    match result {
        Ok(_) => Ok(None),
        Err(PsychMemoryError::Validation(v)) => Ok(Some(rejected_occurrence(&v))),
        Err(other) => Err(other),
    }
}

fn rejected_occurrence(err: &ValidationError) -> RecordPatternOccurrenceOutput {
    RecordPatternOccurrenceOutput::Rejected {
        error_code: err.error_code().to_string(),
        message: err.to_string(),
    }
}

fn rejected_interpretation(err: &ValidationError) -> StoreInterpretationOutput {
    StoreInterpretationOutput::Rejected {
        error_code: err.error_code().to_string(),
        message: err.to_string(),
    }
}

fn rejected_pattern(err: &ValidationError) -> CreatePatternSeedOutput {
    CreatePatternSeedOutput::Rejected {
        error_code: err.error_code().to_string(),
        message: err.to_string(),
    }
}

#[tool_router(vis = "pub")]
impl PsychMemoryServer {
    #[tool(
        description = "Store a journal-derived fact anchored to its raw Froid source excerpt. \
                       Provide the verbatim source_excerpt, a normalized_statement, a fact_type, \
                       and a journal_entry_ref. All facts are recorded as journal-reported; the \
                       system never claims to have observed reality."
    )]
    async fn store_journal_fact(
        &self,
        Parameters(input): Parameters<StoreJournalFactInput>,
    ) -> Result<CallToolResult, ErrorData> {
        // A backend failure is infrastructure, not a tool-input problem, so it
        // surfaces as a protocol-level error. Validation failures, by contrast,
        // come back as Rejected and become a structured tool error (is_error).
        let outcome = self
            .store_journal_fact_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend store failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            StoreJournalFactOutput::Stored { .. } => Ok(CallToolResult::success(vec![content])),
            StoreJournalFactOutput::Rejected { .. } => Ok(CallToolResult::error(vec![content])),
        }
    }

    #[tool(
        description = "Store a psychological interpretation (a hypothesis) that must be grounded \
                       in at least one existing journal fact. Provide the hypothesis, an \
                       interpretation_type, supported_by_fact_ids (each must resolve to a stored \
                       fact), a confidence in 0.0..=1.0, and a falsification_question. \
                       Interpretations are hypotheses, never facts."
    )]
    async fn store_interpretation(
        &self,
        Parameters(input): Parameters<StoreInterpretationInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let outcome = self
            .store_interpretation_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend store failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            StoreInterpretationOutput::Stored { .. } => Ok(CallToolResult::success(vec![content])),
            StoreInterpretationOutput::Rejected { .. } => Ok(CallToolResult::error(vec![content])),
        }
    }

    #[tool(
        description = "Create a named pattern seed: an observation category for a recurring \
                       dynamic (e.g. 'hunger as discharge'). Provide name, a slug \
                       (^[a-z][a-z0-9_]*$), description, markers, and counter_markers. A seed is \
                       only a category to observe — it does NOT claim the pattern is active or \
                       that the user has it. Idempotent by slug."
    )]
    async fn create_pattern_seed(
        &self,
        Parameters(input): Parameters<CreatePatternSeedInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let outcome = self
            .create_pattern_seed_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend store failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            // Stored and AlreadyExists are both successful (ok:true) outcomes.
            CreatePatternSeedOutput::Stored { .. }
            | CreatePatternSeedOutput::AlreadyExists { .. } => {
                Ok(CallToolResult::success(vec![content]))
            }
            CreatePatternSeedOutput::Rejected { .. } => Ok(CallToolResult::error(vec![content])),
        }
    }

    #[tool(
        description = "Record a concrete dated occurrence of a pattern seed in one episode. \
                       Requires an existing pattern_id and at least one existing fact_id; \
                       interpretation_ids are optional. Provide occurrence_date (YYYY-MM-DD), a \
                       phase (activated / recognized_before_action / recognized_after_action / \
                       inhibited / not_activated / transformed), a summary describing THIS episode \
                       only (not a global identity claim), a confidence, and an optional intensity. \
                       This does not activate the pattern."
    )]
    async fn record_pattern_occurrence(
        &self,
        Parameters(input): Parameters<RecordPatternOccurrenceInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let outcome = self
            .record_pattern_occurrence_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend store failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            RecordPatternOccurrenceOutput::Stored { .. } => {
                Ok(CallToolResult::success(vec![content]))
            }
            RecordPatternOccurrenceOutput::Rejected { .. } => {
                Ok(CallToolResult::error(vec![content]))
            }
        }
    }

    #[tool(
        description = "Retrieve the dated occurrences recorded for a pattern seed, grouped by \
                       date. Read-only and descriptive: returns occurrences, per-phase counts, \
                       and warnings about corrupt records. Optionally filter by date_from/date_to \
                       (YYYY-MM-DD) and phases. It does NOT compute trend, decide whether the \
                       pattern is active, or produce any psychological conclusion."
    )]
    async fn query_pattern_timeline(
        &self,
        Parameters(input): Parameters<QueryPatternTimelineInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let outcome = self
            .query_pattern_timeline_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend query failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            QueryPatternTimelineOutput::Found { .. } => Ok(CallToolResult::success(vec![content])),
            QueryPatternTimelineOutput::Rejected { .. } => Ok(CallToolResult::error(vec![content])),
        }
    }

    #[tool(
        description = "Retrieve a single epistemic record by its stable id (prefix routes the \
                       type: fact_ -> journal fact, interp_ -> interpretation, pattern_ -> \
                       pattern seed, occ_ -> pattern occurrence). Read-only and typed: validates \
                       the stored record against its schema and returns it. Use this to follow ids \
                       from a timeline. It does not interpret, conclude, or assess activation."
    )]
    async fn get_epistemic_record(
        &self,
        Parameters(input): Parameters<GetEpistemicRecordInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let outcome = self
            .get_epistemic_record_flow(input)
            .await
            .map_err(|e| ErrorData::internal_error(format!("backend query failed: {e}"), None))?;

        let content = Content::json(&outcome)?;
        match outcome {
            GetEpistemicRecordOutput::Found { .. } => Ok(CallToolResult::success(vec![content])),
            GetEpistemicRecordOutput::Rejected { .. } => Ok(CallToolResult::error(vec![content])),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PsychMemoryServer {
    fn get_info(&self) -> ServerInfo {
        // ServerInfo is #[non_exhaustive]; build from default and set fields.
        let mut info = ServerInfo::default();
        // env! expands in this crate, so it names psych-memory-mcp (not rmcp).
        let mut implementation = Implementation::default();
        implementation.name = env!("CARGO_PKG_NAME").to_string();
        implementation.version = env!("CARGO_PKG_VERSION").to_string();
        info.server_info = implementation;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Psychological memory wrapper. Only explicit epistemic tools are \
             exposed; there is no generic memory-writing tool."
                .to_string(),
        );
        info
    }
}
