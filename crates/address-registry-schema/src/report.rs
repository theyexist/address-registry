//! The import file and its report: who says what a script is.
//!
//! Nothing here carries a behaviour. A claim names a project and a contract, and
//! says which upstream said so; it never says what the script does.

use crate::claim::{Claim, Source};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize)]
pub struct Imported {
    pub format: u32,
    pub generated_at: String,
    pub sources: Sources,
    pub credentials: BTreeMap<String, Vec<Claim>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Sources {
    pub strica: SourceRef,
    pub eternl: SourceRef,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceRef {
    pub repo: String,
    pub rev: Option<String>,
    pub projects: usize,
}

#[derive(Deserialize, Serialize)]
pub struct ImportReport {
    pub format: u32,
    pub generated_at: String,
    pub sources: Sources,
    pub counts: ImportCounts,
    pub skipped: Vec<Skipped>,
    pub skipped_truncated: usize,
    pub conflicts: Vec<Conflict>,
    pub conflicts_truncated: usize,
    pub observations: Observations,
}

#[derive(Deserialize, Serialize)]
pub struct ImportCounts {
    pub credentials: usize,
    pub claims: usize,
    pub claimed_by_both_sources: usize,
    pub conflicts: usize,
    pub strica_contracts_dropped: usize,
    pub eternl_scripts_dropped: usize,
}

#[derive(Deserialize, Serialize)]
pub struct Skipped {
    pub source: Source,
    pub file: String,
    pub script_hash: Option<String>,
    pub reason: String,
}

#[derive(Deserialize, Serialize)]
pub struct Conflict {
    pub credential: String,
    pub claims: Vec<ConflictClaim>,
}

#[derive(Deserialize, Serialize)]
pub struct ConflictClaim {
    pub source: Source,
    pub project: String,
    pub contract: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct Observations {
    pub eternl_script_index: Option<IndexObservation>,
}

#[derive(Deserialize, Serialize)]
pub struct IndexObservation {
    pub generated_at: Option<String>,
    pub scripts: usize,
    pub credentials: usize,
    pub not_in_project_files: usize,
    pub longer_than_a_credential: usize,
}
