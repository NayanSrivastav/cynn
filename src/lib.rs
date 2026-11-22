//! `git-lineage` library core
//!
//! Provides the engine for provenance traversal, noise filtering,
//! and archaeological context assembly.

pub mod core;
pub mod enrich;
pub mod git;
pub mod output;

/// Top-level result type for library operations
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
