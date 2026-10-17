//! Crate `syno-verbal` (Lapis 5 - Kognisi Verbal)
//!
//! Mengurus `VerbalEngine` (pemrosesan bahasa alami, Hierarchical Trie traversal, Auto-Regressive generation),
//! `TreeReasoning` (penalaran berbasis pohon memori dan konfirmasi deduktif),
//! serta `BulkDatasetLoader` & `ConversationDataLoader` (ingesti dataset efisien O(1) streaming).

pub mod engine;
pub mod loader;
pub mod reasoning;

pub use engine::{
    VerbalEngine, VerbalInferenceResult, VerbalSession, VerbalStep, VerbalStopReason,
    VerbalTrainingStats,
};
pub use loader::{
    BulkDatasetLoader, ConversationDataLoader, ConversationEntry, DataLoaderConfig, LoaderStats,
    TextDataLoader,
};
pub use reasoning::{
    AssociationPath, ReasoningEngine, ReasoningTrace, ResponseCategory, ResponseMemoryNode,
    TreeReasoningEngine,
};
