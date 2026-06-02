//! Domain and backend-neutral data models.

pub mod backend_request;
pub mod journal_fact;

pub use backend_request::{BackendStoreResult, StoreMemoryRequest};
pub use journal_fact::{
    FactType, JournalEntryRef, StoreJournalFactInput, StoreJournalFactOutput, ValidatedJournalFact,
};
