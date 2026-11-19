//! The resolved file: behaviour overlaid on the claims.
//!
//! Two artefacts come out of the same authoring pass, because two consumers ask
//! different questions:
//!
//!   `Behaviour`  what a credential is, and every use of it — small, and all a
//!                consumer holding a handful of credentials needs
//!   `Resolved`   the union: behaviour plus every name anybody gives it, for a
//!                reader that wants both
//!
//! Both map a credential to a **list** of uses. One address can do more than one
//! job, so "what is this?" has more than one answer, and the registry does not
//! pick. A consumer resolves the overload with the evidence it holds — a
//! datumless UTxO is the wallet use; a datum matching `deadline_and_owner` is the
//! lock use.

use crate::claim::Claim;
use crate::contract::{Category, Contract, Shape};
use crate::report::Sources;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Resolved {
    pub format: u32,
    pub generated_at: String,
    pub sources: Sources,
    pub authored: BTreeMap<String, Vec<Contract>>,
    pub credentials: BTreeMap<String, Vec<Claim>>,
}

impl Resolved {
    /// Every use authored for a credential. Empty when nobody has stated one.
    pub fn behaviour(&self, credential: &str) -> &[Contract] {
        self.authored
            .get(credential)
            .map(Vec::as_slice)
            .unwrap_or_default()
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
/// adds a project. An unknown credential is an empty slice, not an error — most
/// of the chain is unknown, and a credential with two uses has two entries.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Behaviour {
    pub format: u32,
    pub generated_at: String,
    pub contracts: BTreeMap<String, Vec<Contract>>,
}

impl Behaviour {
    pub fn get(&self, credential: &str) -> &[Contract] {
        self.contracts
            .get(credential)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The uses of the credential an address pays to.
    ///
    /// A reward address is refused rather than read: its credential sits where
    /// a payment one would and is a different thing.
    pub fn get_for_address(
        &self,
        address: &str,
    ) -> Result<&[Contract], crate::address::AddressError> {
        let payment = crate::address::payment(address)?;
        Ok(self.get(&payment.hex()))
    }
}

#[derive(Deserialize, Serialize)]
pub struct ResolveReport {
    pub format: u32,
    pub generated_at: String,
    pub counts: ResolveCounts,
    pub authored: Vec<AuthoredRow>,
}

/// Counts are mixed by design and the names say which unit each is: a credential
/// may carry several uses, so "authored" and "authored_credentials" are not the
/// same number.
#[derive(Deserialize, Serialize)]
pub struct ResolveCounts {
    pub credentials: usize,
    /// Uses, one row per `[[use]]`.
    pub authored: usize,
    /// Distinct credentials with at least one use.
    pub authored_credentials: usize,
    pub authored_with_upstream_claim: usize,
    pub authored_only: usize,
    pub derived_from_verified: usize,
    pub derived_from_attested: usize,
}

#[derive(Deserialize, Serialize)]
pub struct AuthoredRow {
    pub credential: String,
    pub category: Category,
    pub shape: Option<Shape>,
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
