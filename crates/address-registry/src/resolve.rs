//! Overlay the authored layer onto the imported one, and write the artefacts.
//!
//! The shapes are `address-registry-schema`'s; this module is the part that
//! reads and writes them. The two layers stay separate in the file: `authored`
//! is behaviour, `credentials` is every name anybody else gives a credential.
//! Nothing here renames an upstream's claim or picks a winner between two.
//!
//! A credential may carry several uses, and the resolver writes them all. It does
//! not choose between them: only on-chain evidence can, at the consumer.

use crate::contract;
use crate::error::{Error, Result};
use crate::stakes;
use crate::write_json;
use address_registry_schema::{
    AuthoredRow, Behaviour, DerivedFrom, DerivedFromStatus, Imported, ResolveCounts, ResolveReport,
    Resolved, StakeLabels, StakeServices,
};
use std::fs;
use std::path::Path;

pub fn run(
    imported_path: &Path,
    contracts_dir: &Path,
    stakes_dir: &Path,
    stake_exclusions_dir: &Path,
    out: &Path,
) -> Result<()> {
    let text =
        fs::read_to_string(imported_path).map_err(|error| Error::at(imported_path, error))?;
    let imported: Imported =
        serde_json::from_str(&text).map_err(|error| Error::at(imported_path, error))?;
    let contracts = contract::load(contracts_dir)?;
    let services = stakes::load_services(stakes_dir)?;
    let exclusions = stakes::load_exclusions(stake_exclusions_dir)?;
    stakes::check_disjoint(&services, &exclusions)?;

    let mut credentials = imported.credentials;
    let mut rows = Vec::new();
    let mut verified = 0;
    let mut attested = 0;
    let mut with_claim = 0;

    for (credential, uses) in &contracts {
        let upstream_claims = credentials.get(credential).map_or(0, Vec::len);
        if upstream_claims > 0 {
            with_claim += 1;
        }
        for use_of in uses {
            let status = match &use_of.derived_from {
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
            rows.push(AuthoredRow {
                credential: credential.clone(),
                category: use_of.category,
                shape: use_of.shape,
                label: use_of.label.clone(),
                project: use_of.project.clone(),
                upstream_claims,
                derived_from: status,
            });
        }
    }

    // The union: an authored credential with no upstream claim joins the map
    // with an empty list, because `credentials` is every credential the registry
    // knows.
    for credential in contracts.keys() {
        credentials.entry(credential.clone()).or_default();
    }

    let authored: usize = contracts.values().map(Vec::len).sum();
    let authored_credentials = contracts.len();
    let service_count = services.len();
    let exclusion_count = exclusions.len();
    let generated_at = crate::clock::now();
    // The two artefacts are written from the same pass so they cannot disagree.
    // `behaviour.bin` is the targeted read: the curated rows only.
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
        generated_at: generated_at.clone(),
        counts: ResolveCounts {
            credentials: resolved.credentials.len(),
            authored,
            authored_credentials,
            authored_with_upstream_claim: with_claim,
            authored_only: authored_credentials - with_claim,
            derived_from_verified: verified,
            derived_from_attested: attested,
            stake_services: service_count,
            stake_exclusions: exclusion_count,
        },
        authored: rows,
    };
    // The stake table is a separate artefact because it is a separate question —
    // WHO runs a wallet, not WHAT a script is — and its answer changes on its own
    // schedule. The binary is byte-compatible with the `STKL` table shared-crates
    // already publishes, so a consumer switches source without a new decoder.
    let stake_services = StakeServices {
        format: address_registry_schema::stake::FORMAT,
        generated_at: generated_at.clone(),
        services,
        exclusions,
    };
    let stake_labels = StakeLabels::from_services(&stake_services.services)
        .map_err(|error| Error::new(error.to_string()))?;

    let registry_path = out.join("registry.json");
    let behaviour_path = out.join("behaviour.json");
    let behaviour_bin_path = out.join("behaviour.bin");
    let stakes_path = out.join("stake-services.json");
    let stakes_bin_path = out.join("stake-services.bin");
    let report_path = out.join("report.json");
    fs::create_dir_all(out).map_err(|error| Error::at(out, error))?;
    write_json(&registry_path, &resolved)?;
    write_json(&behaviour_path, &behaviour)?;
    let behaviour_bytes = behaviour
        .to_bytes()
        .map_err(|error| Error::new(error.to_string()))?;
    fs::write(&behaviour_bin_path, behaviour_bytes)
        .map_err(|error| Error::at(&behaviour_bin_path, error))?;
    write_json(&stakes_path, &stake_services)?;
    fs::write(&stakes_bin_path, stake_labels.to_bytes())
        .map_err(|error| Error::at(&stakes_bin_path, error))?;
    write_json(&report_path, &report)?;

    println!("credentials  {}", report.counts.credentials);
    println!(
        "authored     {} uses on {} credentials",
        report.counts.authored, report.counts.authored_credentials
    );
    println!(
        "  with claim {}   only {}",
        report.counts.authored_with_upstream_claim, report.counts.authored_only
    );
    println!(
        "  verified   {}   attested {}",
        report.counts.derived_from_verified, report.counts.derived_from_attested
    );
    println!(
        "stakes       {} services, {} exclusions",
        report.counts.stake_services, report.counts.stake_exclusions
    );
    println!("wrote        {}", registry_path.display());
    println!("             {}", behaviour_path.display());
    println!("             {}", behaviour_bin_path.display());
    println!("             {}", stakes_path.display());
    println!("             {}", stakes_bin_path.display());
    Ok(())
}
