//! `cynn` library core
//!
//! High-performance codebase archaeology CLI & agent forensic primitive.
//! Old English *cynn* (/kɪn/) — lineage, descent, ancestral origin.

pub mod cli;
pub mod core;
pub mod enrich;
pub mod git;
pub mod output;

pub use cli::run_cli;
pub use core::{GenesisAnalyzer, LineageDossier, Target};
pub use git::GitRepo;
