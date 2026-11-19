//! A machine-readable registry of Cardano scripts, keyed by payment credential.
//!
//! Two layers, kept separate on purpose:
//!
//!   import   Strica + Eternl -> imported/registry.json   who says what a script is
//!   resolve  imported + contracts/*.toml -> resolved/    what it does, on top
//!
//! The imported layer never assigns a `kind` or a datum `shape`; nobody upstream
//! carries them. The authored layer is where a behaviour is stated, and every
//! row there names its source.
//!
//! The types live in `address-registry-schema`, which is what a consumer depends
//! on. This binary is the part that reads and writes the files.

mod clock;
mod contract;
mod error;
mod resolve;
mod upstream;
mod validate;

use crate::error::{Error, Result};
use address_registry_schema::{
    Claim, Conflict, ConflictClaim, ImportCounts, ImportReport, Imported, Observations, Skipped,
    SourceRef, Sources,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const USAGE: &str = "\
usage: address-registry <command>

  import   --strica <dir> --eternl <dir> [--out imported]
           fold the two public registries into one index of claims

  resolve  [--imported imported/registry.json] [--contracts contracts] [--out resolved]
           overlay the authored contracts onto the imported claims

  validate [--contracts contracts] [<file>...]
           check contract files against the schema, and that no two declare one
           credential; with no files, check every file in the directory

Nothing is fetched and nothing is resolved behind your back: a credential two
projects claim is recorded as a conflict for a human, never silently picked.";

const STRICA_REPO: &str = "https://github.com/StricaHQ/cardano-contracts-registry";
const ETERNL_REPO: &str = "https://github.com/Tastenkunst/eternl-cardano-registry";

const SKIPPED_SAMPLE: usize = 100;
const CONFLICT_SAMPLE: usize = 500;

enum Command {
    Import {
        strica: PathBuf,
        eternl: PathBuf,
        out: PathBuf,
    },
    Resolve {
        imported: PathBuf,
        contracts: PathBuf,
        out: PathBuf,
    },
    Validate {
        contracts: PathBuf,
        files: Vec<PathBuf>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("address-registry: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(command) = parse_args(env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    match command {
        Command::Import {
            strica,
            eternl,
            out,
        } => import(&strica, &eternl, &out),
        Command::Resolve {
            imported,
            contracts,
            out,
        } => resolve::run(&imported, &contracts, &out),
        Command::Validate { contracts, files } => validate::run(&contracts, &files),
    }
}

fn import(strica_dir: &Path, eternl_dir: &Path, out: &Path) -> Result<()> {
    let upstream::Collected {
        claims: strica_claims,
        skipped: strica_skipped,
        projects: strica_projects,
    } = upstream::collect_strica(strica_dir)?;
    let upstream::Collected {
        claims: eternl_claims,
        skipped: eternl_skipped,
        projects: eternl_projects,
    } = upstream::collect_eternl(eternl_dir)?;

    let project_hashes: BTreeSet<String> = eternl_claims
        .iter()
        .map(|(credential, _)| credential.clone())
        .collect();
    let eternl_index = upstream::index_observation(eternl_dir, &project_hashes)?;

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
            repo: STRICA_REPO.to_string(),
            rev: upstream::git_rev(strica_dir),
            projects: strica_projects,
        },
        eternl: SourceRef {
            repo: ETERNL_REPO.to_string(),
            rev: upstream::git_rev(eternl_dir),
            projects: eternl_projects,
        },
    };

    let imported = Imported {
        format: address_registry_schema::IMPORT_FORMAT,
        generated_at: generated_at.clone(),
        sources: sources.clone(),
        credentials: by_credential,
    };
    let report = ImportReport {
        format: address_registry_schema::IMPORT_FORMAT,
        generated_at,
        sources,
        counts: ImportCounts {
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

    let registry_path = out.join("registry.json");
    let report_path = out.join("report.json");
    fs::create_dir_all(out).map_err(|error| Error::at(out, error))?;
    write_json(&registry_path, &imported)?;
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

fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<Option<Command>> {
    let mut rest = argv.into_iter();
    let Some(command) = rest.next() else {
        return Err(Error::new(format!("a command is required\n\n{USAGE}")));
    };
    match command.as_str() {
        "import" => parse_import(&mut rest),
        "resolve" => parse_resolve(&mut rest),
        "validate" => parse_validate(&mut rest),
        "--help" | "-h" | "help" => Ok(None),
        other => Err(Error::new(format!("unknown command: {other}\n\n{USAGE}"))),
    }
}

fn parse_import(rest: &mut impl Iterator<Item = String>) -> Result<Option<Command>> {
    let mut strica = None;
    let mut eternl = None;
    let mut out = PathBuf::from("imported");
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--strica" => strica = Some(PathBuf::from(value(rest, "--strica")?)),
            "--eternl" => eternl = Some(PathBuf::from(value(rest, "--eternl")?)),
            "--out" => out = PathBuf::from(value(rest, "--out")?),
            "--help" | "-h" => return Ok(None),
            other => return Err(Error::new(format!("unknown argument: {other}"))),
        }
    }
    let strica = strica.ok_or_else(|| Error::new(format!("--strica is required\n\n{USAGE}")))?;
    let eternl = eternl.ok_or_else(|| Error::new(format!("--eternl is required\n\n{USAGE}")))?;
    Ok(Some(Command::Import {
        strica,
        eternl,
        out,
    }))
}

fn parse_resolve(rest: &mut impl Iterator<Item = String>) -> Result<Option<Command>> {
    let mut imported = PathBuf::from("imported/registry.json");
    let mut contracts = PathBuf::from("contracts");
    let mut out = PathBuf::from("resolved");
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--imported" => imported = PathBuf::from(value(rest, "--imported")?),
            "--contracts" => contracts = PathBuf::from(value(rest, "--contracts")?),
            "--out" => out = PathBuf::from(value(rest, "--out")?),
            "--help" | "-h" => return Ok(None),
            other => return Err(Error::new(format!("unknown argument: {other}"))),
        }
    }
    Ok(Some(Command::Resolve {
        imported,
        contracts,
        out,
    }))
}

fn parse_validate(rest: &mut impl Iterator<Item = String>) -> Result<Option<Command>> {
    let mut contracts = PathBuf::from("contracts");
    let mut files = Vec::new();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--contracts" => contracts = PathBuf::from(value(rest, "--contracts")?),
            "--help" | "-h" => return Ok(None),
            other if other.starts_with('-') => {
                return Err(Error::new(format!("unknown argument: {other}")))
            }
            path => files.push(PathBuf::from(path)),
        }
    }
    Ok(Some(Command::Validate { contracts, files }))
}

fn value(rest: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    rest.next()
        .ok_or_else(|| Error::new(format!("{flag} needs a value")))
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text).map_err(|error| Error::at(path, error))
}
