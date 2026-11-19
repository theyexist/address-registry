//! Fold the two public Cardano contract registries into one generated index,
//! keyed by payment credential.
//!
//!   Strica   https://github.com/StricaHQ/cardano-contracts-registry
//!            projects/*.json -> contracts[] { scriptHash, name, version, audits }
//!   Eternl   https://github.com/Tastenkunst/eternl-cardano-registry
//!            registry/projects/*.json -> scripts[] { scriptHash, name, purpose }
//!
//! This is the **import** layer. It never invents a `kind` or a `shape`: every
//! entry says who claimed it and what they called it. The authored layer
//! (`contracts/<credential>.toml`) overlays the behaviour a script has, and
//! `imported/` is regenerated from the two upstreams at a recorded rev.

mod claim;
mod clock;
mod error;
mod output;
mod upstream;

use crate::claim::Claim;
use crate::error::{Error, Result};
use crate::output::{
    Conflict, ConflictClaim, Counts, Observations, Registry, Report, Skipped, SourceRef, Sources,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const USAGE: &str = "\
usage: address-registry import --strica <dir> --eternl <dir> [--out imported]

Reads local clones of the two upstream registries and writes:
  <out>/registry.json   every claim, keyed by 56-hex payment credential
  <out>/report.json     counts, skipped rows, and conflicting claims

Nothing is fetched and nothing is resolved. A credential two projects claim
is recorded as a conflict for a human, never silently picked.";

const STRICA_REPO: &str = "https://github.com/StricaHQ/cardano-contracts-registry";
const ETERNL_REPO: &str = "https://github.com/Tastenkunst/eternl-cardano-registry";

const SKIPPED_SAMPLE: usize = 100;
const CONFLICT_SAMPLE: usize = 500;

struct Args {
    strica: PathBuf,
    eternl: PathBuf,
    out: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("import: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(args) = parse_args(env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };

    let upstream::Collected {
        claims: strica_claims,
        skipped: strica_skipped,
        projects: strica_projects,
    } = upstream::collect_strica(&args.strica)?;
    let upstream::Collected {
        claims: eternl_claims,
        skipped: eternl_skipped,
        projects: eternl_projects,
    } = upstream::collect_eternl(&args.eternl)?;

    let project_hashes: BTreeSet<String> = eternl_claims
        .iter()
        .map(|(credential, _)| credential.clone())
        .collect();
    let eternl_index = upstream::index_observation(&args.eternl, &project_hashes)?;

    // One claim per (credential, claim) pair: the same statement repeated by
    // the same source is a duplicate, not a second opinion.
    let mut by_credential: BTreeMap<String, Vec<Claim>> = BTreeMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (credential, claim) in strica_claims.into_iter().chain(eternl_claims) {
        let key = format!("{credential}\u{0}{}", serde_json::to_string(&claim)?);
        if seen.insert(key) {
            by_credential.entry(credential).or_default().push(claim);
        }
    }
    for claims in by_credential.values_mut() {
        claims.sort();
    }

    let strica_dropped = strica_skipped.len();
    let eternl_dropped = eternl_skipped.len();
    let skipped: Vec<Skipped> = strica_skipped.into_iter().chain(eternl_skipped).collect();
    let skipped_truncated = skipped.len().saturating_sub(SKIPPED_SAMPLE);
    let skipped: Vec<Skipped> = skipped.into_iter().take(SKIPPED_SAMPLE).collect();

    let (conflicts, from_both) = summarise(&by_credential);
    let conflict_count = conflicts.len();
    let conflicts_truncated = conflict_count.saturating_sub(CONFLICT_SAMPLE);
    let conflicts: Vec<Conflict> = conflicts.into_iter().take(CONFLICT_SAMPLE).collect();

    let credentials = by_credential.len();
    let claims = by_credential.values().map(Vec::len).sum();
    let generated_at = clock::now();
    let sources = Sources {
        strica: SourceRef {
            repo: STRICA_REPO,
            rev: upstream::git_rev(&args.strica),
            projects: strica_projects,
        },
        eternl: SourceRef {
            repo: ETERNL_REPO,
            rev: upstream::git_rev(&args.eternl),
            projects: eternl_projects,
        },
    };

    let registry = Registry {
        format: 1,
        generated_at: generated_at.clone(),
        sources: sources.clone(),
        credentials: by_credential,
    };
    let report = Report {
        format: 1,
        generated_at,
        sources,
        counts: Counts {
            credentials,
            claims,
            claimed_by_both_sources: from_both,
            conflicts: conflict_count,
            strica_contracts_dropped: strica_dropped,
            eternl_scripts_dropped: eternl_dropped,
        },
        skipped,
        skipped_truncated,
        conflicts,
        conflicts_truncated,
        observations: Observations {
            eternl_script_index: eternl_index,
        },
    };

    let registry_path = args.out.join("registry.json");
    let report_path = args.out.join("report.json");
    fs::create_dir_all(&args.out).map_err(|error| Error::at(&args.out, error))?;
    write_json(&registry_path, &registry)?;
    write_json(&report_path, &report)?;

    println!("credentials  {}", report.counts.credentials);
    println!("claims       {}", report.counts.claims);
    println!("both sources {}", report.counts.claimed_by_both_sources);
    println!(
        "conflicts    {}  (see {})",
        report.counts.conflicts,
        report_path.display()
    );
    println!(
        "dropped      {}  not a 56-hex payment credential",
        strica_dropped + eternl_dropped
    );
    match &report.observations.eternl_script_index {
        Some(index) => println!(
            "upstream idx {} credentials, {} not in its project files (not imported)",
            index.credentials, index.not_in_project_files
        ),
        None => println!("upstream idx absent"),
    }
    println!("wrote        {}", registry_path.display());
    Ok(())
}

fn summarise(by_credential: &BTreeMap<String, Vec<Claim>>) -> (Vec<Conflict>, usize) {
    let mut conflicts = Vec::new();
    let mut from_both = 0;
    for (credential, claims) in by_credential {
        let sources: BTreeSet<&str> = claims.iter().map(|claim| claim.source().as_str()).collect();
        if sources.len() > 1 {
            from_both += 1;
        }
        let names: BTreeSet<(&str, &str)> = claims
            .iter()
            .map(|claim| (claim.project(), claim.contract().unwrap_or_default()))
            .collect();
        if names.len() > 1 {
            conflicts.push(Conflict {
                credential: credential.clone(),
                claims: claims
                    .iter()
                    .map(|claim| ConflictClaim {
                        source: claim.source(),
                        project: claim.project().to_string(),
                        contract: claim.contract().map(str::to_string),
                    })
                    .collect(),
            });
        }
    }
    (conflicts, from_both)
}

fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<Option<Args>> {
    let mut rest = argv.into_iter();
    let Some(command) = rest.next() else {
        return Err(Error::new(format!("a subcommand is required\n\n{USAGE}")));
    };
    match command.as_str() {
        "import" => {}
        "--help" | "-h" | "help" => return Ok(None),
        other => {
            return Err(Error::new(format!(
                "unknown subcommand: {other}\n\n{USAGE}"
            )))
        }
    }

    let mut strica = None;
    let mut eternl = None;
    let mut out = PathBuf::from("imported");
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--strica" => strica = Some(PathBuf::from(value(&mut rest, "--strica")?)),
            "--eternl" => eternl = Some(PathBuf::from(value(&mut rest, "--eternl")?)),
            "--out" => out = PathBuf::from(value(&mut rest, "--out")?),
            "--help" | "-h" => return Ok(None),
            other => return Err(Error::new(format!("unknown argument: {other}"))),
        }
    }

    let strica = strica.ok_or_else(|| Error::new(format!("--strica is required\n\n{USAGE}")))?;
    let eternl = eternl.ok_or_else(|| Error::new(format!("--eternl is required\n\n{USAGE}")))?;
    Ok(Some(Args {
        strica,
        eternl,
        out,
    }))
}

fn value(rest: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    rest.next()
        .ok_or_else(|| Error::new(format!("{flag} needs a value")))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text).map_err(|error| Error::at(path, error))
}
