//! Overlay the authored layer onto the imported one.
//!
//! The two layers answer different questions and stay separate in the file:
//! `authored` is behaviour, `credentials` is every name anybody else gives a
//! credential. A consumer reads the behaviour for a credential it already has;
//! nothing here renames an upstream's claim or picks a winner between two.
//!
//! The union matters. A credential with an authored row and no upstream claim is
//! the behavioural gap this registry exists to close, so it appears in
//! `credentials` with an empty claim list rather than being left out.

use crate::claim::Claim;
use crate::clock;
use crate::contract::{self, Contract, DerivedFrom};
use crate::error::{Error, Result};
use crate::output::Sources;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct ImportedRegistry {
    sources: Sources,
    credentials: BTreeMap<String, Vec<Claim>>,
}

#[derive(Serialize)]
struct ResolvedRegistry {
    format: u32,
    generated_at: String,
    sources: Sources,
    authored: BTreeMap<String, Contract>,
    credentials: BTreeMap<String, Vec<Claim>>,
}

#[derive(Serialize)]
struct ResolvedReport {
    format: u32,
    generated_at: String,
    counts: Counts,
    authored: Vec<AuthoredRow>,
}

#[derive(Serialize)]
struct Counts {
    credentials: usize,
    authored: usize,
    authored_with_upstream_claim: usize,
    authored_only: usize,
    derived_from_verified: usize,
    derived_from_attested: usize,
}

#[derive(Serialize)]
struct AuthoredRow {
    credential: String,
    kind: contract::Kind,
    shape: contract::Shape,
    label: String,
    project: String,
    upstream_claims: usize,
    derived_from: DerivedFromStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum DerivedFromStatus {
    /// A `derived_from.address` that decoded to this credential.
    Verified,
    /// No address registered; the reason is in the authored file.
    Attested,
}

pub fn run(imported: &Path, contracts_dir: &Path, out: &Path) -> Result<()> {
    let text = fs::read_to_string(imported).map_err(|error| Error::at(imported, error))?;
    let imported_registry: ImportedRegistry =
        serde_json::from_str(&text).map_err(|error| Error::at(imported, error))?;
    let contracts = contract::load(contracts_dir)?;

    let mut credentials = imported_registry.credentials;
    let mut rows = Vec::new();
    let mut verified = 0;
    let mut attested = 0;
    let mut with_claim = 0;

    for (credential, contract) in &contracts {
        let status = match &contract.derived_from {
            // `Contract::validate` already decoded and compared it.
            DerivedFrom::Address(_) => {
                verified += 1;
                DerivedFromStatus::Verified
            }
            DerivedFrom::Attested(_) => {
                attested += 1;
                DerivedFromStatus::Attested
            }
        };
        let upstream_claims = credentials.get(credential).map_or(0, Vec::len);
        if upstream_claims > 0 {
            with_claim += 1;
        }
        rows.push(AuthoredRow {
            credential: credential.clone(),
            kind: contract.kind,
            shape: contract.shape,
            label: contract.label.clone(),
            project: contract.project.clone(),
            upstream_claims,
            derived_from: status,
        });
    }

    // The union: an authored row with no upstream claim joins the map with an
    // empty list, because `credentials` is every credential the registry knows.
    for credential in contracts.keys() {
        credentials.entry(credential.clone()).or_default();
    }

    let authored = contracts.len();
    let generated_at = clock::now();
    let resolved = ResolvedRegistry {
        format: 1,
        generated_at: generated_at.clone(),
        sources: imported_registry.sources,
        authored: contracts,
        credentials,
    };
    let report = ResolvedReport {
        format: 1,
        generated_at,
        counts: Counts {
            credentials: resolved.credentials.len(),
            authored,
            authored_with_upstream_claim: with_claim,
            authored_only: authored - with_claim,
            derived_from_verified: verified,
            derived_from_attested: attested,
        },
        authored: rows,
    };

    let registry_path = out.join("registry.json");
    let report_path = out.join("report.json");
    fs::create_dir_all(out).map_err(|error| Error::at(out, error))?;
    write_json(&registry_path, &resolved)?;
    write_json(&report_path, &report)?;

    println!("credentials  {}", report.counts.credentials);
    println!("authored     {}", report.counts.authored);
    println!(
        "  with claim {}   only {}",
        report.counts.authored_with_upstream_claim, report.counts.authored_only
    );
    println!(
        "  verified   {}   attested {}",
        report.counts.derived_from_verified, report.counts.derived_from_attested
    );
    println!("wrote        {}", registry_path.display());
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text).map_err(|error| Error::at(path, error))
}
