//! Overlay the authored layer onto the imported one, and write the artefact.
//!
//! The shapes are `address-registry-schema`'s; this module is the part that
//! reads and writes them. The two layers stay separate in the file: `authored`
//! is behaviour, `credentials` is every name anybody else gives a credential.
//! Nothing here renames an upstream's claim or picks a winner between two.

use crate::contract;
use crate::error::{Error, Result};
use crate::write_json;
use address_registry_schema::{
    AuthoredRow, Behaviour, DerivedFrom, DerivedFromStatus, Imported, ResolveCounts, ResolveReport,
    Resolved,
};
use std::fs;
use std::path::Path;

pub fn run(imported_path: &Path, contracts_dir: &Path, out: &Path) -> Result<()> {
    let text =
        fs::read_to_string(imported_path).map_err(|error| Error::at(imported_path, error))?;
    let imported: Imported =
        serde_json::from_str(&text).map_err(|error| Error::at(imported_path, error))?;
    let contracts = contract::load(contracts_dir)?;

    let mut credentials = imported.credentials;
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
    let generated_at = crate::clock::now();
    // The two artefacts are written from the same pass so they cannot disagree.
    // `behaviour.json` is the targeted read: the curated rows only.
    let behaviour = Behaviour {
        format: address_registry_schema::FORMAT,
        generated_at: generated_at.clone(),
        contracts: contracts.clone(),
    };
    let resolved = Resolved {
        format: address_registry_schema::FORMAT,
        generated_at: generated_at.clone(),
        sources: imported.sources,
        authored: contracts,
        credentials,
    };
    let report = ResolveReport {
        format: address_registry_schema::FORMAT,
        generated_at,
        counts: ResolveCounts {
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
    let behaviour_path = out.join("behaviour.json");
    let behaviour_bin_path = out.join("behaviour.bin");
    let report_path = out.join("report.json");
    fs::create_dir_all(out).map_err(|error| Error::at(out, error))?;
    write_json(&registry_path, &resolved)?;
    write_json(&behaviour_path, &behaviour)?;
    let behaviour_bytes = behaviour
        .to_bytes()
        .map_err(|error| Error::new(error.to_string()))?;
    fs::write(&behaviour_bin_path, behaviour_bytes)
        .map_err(|error| Error::at(&behaviour_bin_path, error))?;
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
    println!("             {}", behaviour_path.display());
    println!("             {}", behaviour_bin_path.display());
    Ok(())
}
