//! The resolved file: behaviour overlaid on the claims.
//!
//! Two artefacts come out of the same authoring pass, because two consumers ask
//! different questions:
//!
//!   `Behaviour`  what a script does, by credential — small, and all a consumer
//!                holding a handful of credentials needs
//!   `Resolved`   the union: behaviour plus every name anybody gives it, for a
//!                reader that wants both
//!
//! Fetching the union to answer five lookups is what makes it feel unwieldy:
//! the claims are the bulk and are almost never the question.

use crate::claim::Claim;
use crate::contract::{Contract, Kind, Shape};
use crate::report::Sources;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize)]
pub struct Resolved {
    pub format: u32,
    pub generated_at: String,
    pub sources: Sources,
    pub authored: BTreeMap<String, Contract>,
    pub credentials: BTreeMap<String, Vec<Claim>>,
}

impl Resolved {
    /// The behaviour authored for a credential, if anyone has stated one.
    pub fn behaviour(&self, credential: &str) -> Option<&Contract> {
        self.authored.get(credential)
    }

    /// What each upstream calls this credential. Empty when nobody does.
    pub fn claims(&self, credential: &str) -> &[Claim] {
        self.credentials
            .get(credential)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

/// The authored layer alone: what a consumer with a few credentials fetches.
///
/// Costs the curated rows and nothing else, so it does not grow when an upstream
/// adds a project. A credential with no behaviour is `None` here and is not an
/// error — most of the chain is.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Behaviour {
    pub format: u32,
    pub generated_at: String,
    pub contracts: BTreeMap<String, Contract>,
}

impl Behaviour {
    pub fn get(&self, credential: &str) -> Option<&Contract> {
        self.contracts.get(credential)
    }
}

#[derive(Deserialize, Serialize)]
pub struct ResolveReport {
    pub format: u32,
    pub generated_at: String,
    pub counts: ResolveCounts,
    pub authored: Vec<AuthoredRow>,
}

#[derive(Deserialize, Serialize)]
pub struct ResolveCounts {
    pub credentials: usize,
    pub authored: usize,
    pub authored_with_upstream_claim: usize,
    pub authored_only: usize,
    pub derived_from_verified: usize,
    pub derived_from_attested: usize,
}

#[derive(Deserialize, Serialize)]
pub struct AuthoredRow {
    pub credential: String,
    pub kind: Kind,
    pub shape: Shape,
    pub label: String,
    pub project: String,
    pub upstream_claims: usize,
    pub derived_from: DerivedFromStatus,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DerivedFromStatus {
    /// A `derived_from.address` that decoded to this credential.
    Verified,
    /// No address registered; the reason is in the authored file.
    Attested,
}
