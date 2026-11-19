//! The two generated files, in the shape a consumer reads.
//!
//! Field order is the order it is written in, so a regenerated file diffs as
//! values changing rather than keys moving. Credentials are a `BTreeMap` so the
//! index is sorted by construction.

use crate::claim::{Claim, Source};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct Registry {
    pub format: u32,
    pub generated_at: String,
    pub sources: Sources,
    pub credentials: BTreeMap<String, Vec<Claim>>,
}

#[derive(Clone, Serialize)]
pub struct Sources {
    pub strica: SourceRef,
    pub eternl: SourceRef,
}

#[derive(Clone, Serialize)]
pub struct SourceRef {
    pub repo: &'static str,
    pub rev: Option<String>,
    pub projects: usize,
}

#[derive(Serialize)]
pub struct Report {
    pub format: u32,
    pub generated_at: String,
    pub sources: Sources,
    pub counts: Counts,
    pub skipped: Vec<Skipped>,
    pub skipped_truncated: usize,
    pub conflicts: Vec<Conflict>,
    pub conflicts_truncated: usize,
    pub observations: Observations,
}

#[derive(Serialize)]
pub struct Counts {
    pub credentials: usize,
    pub claims: usize,
    pub claimed_by_both_sources: usize,
    pub conflicts: usize,
    pub strica_contracts_dropped: usize,
    pub eternl_scripts_dropped: usize,
}

#[derive(Serialize)]
pub struct Skipped {
    pub source: Source,
    pub file: String,
    pub script_hash: Option<String>,
    pub reason: String,
}

#[derive(Serialize)]
pub struct Conflict {
    pub credential: String,
    pub claims: Vec<ConflictClaim>,
}

#[derive(Serialize)]
pub struct ConflictClaim {
    pub source: Source,
    pub project: String,
    pub contract: Option<String>,
}

#[derive(Serialize)]
pub struct Observations {
    pub eternl_script_index: Option<IndexObservation>,
}

#[derive(Serialize)]
pub struct IndexObservation {
    pub generated_at: Option<String>,
    pub scripts: usize,
    pub credentials: usize,
    pub not_in_project_files: usize,
    pub longer_than_a_credential: usize,
}
