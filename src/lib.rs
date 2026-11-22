//! `git-lineage` library core
//!
//! High-performance codebase archaeology CLI & agent forensic primitive.

pub mod core;
pub mod enrich;
pub mod git;
pub mod output;

pub use core::{GenesisAnalyzer, LineageDossier, Target};
pub use git::GitRepo;
