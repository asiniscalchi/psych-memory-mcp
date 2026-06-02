//! Domain and backend-neutral data models.

pub mod backend_request;
pub mod interpretation;
pub mod journal_fact;
pub mod pattern_occurrence;
pub mod pattern_seed;
pub mod pattern_timeline;

pub use backend_request::{BackendStoreResult, StoreMemoryRequest};
pub use interpretation::{
    InterpretationStatus, InterpretationType, StoreInterpretationInput, StoreInterpretationOutput,
    ValidatedInterpretation,
};
pub use journal_fact::{
    FactType, JournalEntryRef, StoreJournalFactInput, StoreJournalFactOutput, ValidatedJournalFact,
};
pub use pattern_occurrence::{
    OccurrencePhase, RecordPatternOccurrenceInput, RecordPatternOccurrenceOutput,
    ValidatedPatternOccurrence,
};
pub use pattern_seed::{CreatePatternSeedInput, CreatePatternSeedOutput, ValidatedPatternSeed};
pub use pattern_timeline::{
    QueryPatternTimelineInput, QueryPatternTimelineOutput, ValidatedPatternTimelineQuery,
};
