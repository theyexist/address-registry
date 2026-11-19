//! `validate`: is each authored file one the schema accepts, and does each
//! directory still hold one statement per key?
//!
//! `resolve` loads the same directories, so an invalid file fails there too.
//! This command exists for the two things that failure does not give a
//! contributor: the file named, and every failing file in one run rather than
//! only the first one the loader reaches. CI runs it on the files a pull request
//! changed.
//!
//! Nothing here writes, fetches or resolves. A valid file is a statement about
//! the file, not about the registry: an address that decodes is not a contract
//! that behaves as declared.

use crate::contract;
use crate::error::{Error, Result};
use crate::files;
use address_registry_schema::{StakeExclusion, StakeService};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// What a path resolves to, so `./contracts/x.toml`, `contracts/x.toml` and an
/// absolute path are one file rather than two statements about one key. A path
/// that does not resolve is kept as given, so a missing file is reported by the
/// reader instead of disappearing here.
fn identity(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Contract,
    Stake,
    Exclusion,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Contract => "contract file",
            Kind::Stake => "stake file",
            Kind::Exclusion => "stake exclusion",
        }
    }
}

/// The key a file declares, which is what a second file must not repeat. The
/// error already names the path, so a caller prints it as it is.
fn key(kind: Kind, path: &Path) -> Result<String> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
        return Err(Error::at(
            path,
            format!("a {} is `<key>.toml`", kind.name()),
        ));
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    match kind {
        Kind::Contract => Ok(contract::read(path)?.credential),
        Kind::Stake => {
            let text = fs::read_to_string(path).map_err(|error| Error::at(path, error))?;
            let service: StakeService =
                toml::from_str(&text).map_err(|error| Error::at(path, error))?;
            service
                .validate(stem)
                .map_err(|invalid| Error::at(path, invalid))?;
            Ok(service.stake)
        }
        Kind::Exclusion => {
            let text = fs::read_to_string(path).map_err(|error| Error::at(path, error))?;
            let exclusion: StakeExclusion =
                toml::from_str(&text).map_err(|error| Error::at(path, error))?;
            exclusion
                .validate(stem)
                .map_err(|invalid| Error::at(path, invalid))?;
            Ok(exclusion.stake)
        }
    }
}

/// One outcome per file. A duplicate marks both: only one of them is usually in
/// the change, and naming the other is what makes the pair fixable.
fn outcomes(
    files: Vec<(PathBuf, PathBuf)>,
    targets: &BTreeSet<PathBuf>,
    kind: Kind,
) -> Vec<(PathBuf, bool, Option<String>)> {
    let mut out: Vec<(PathBuf, bool, Option<String>)> = Vec::new();
    let mut declared: BTreeMap<String, usize> = BTreeMap::new();
    for (id, path) in files {
        let target = targets.contains(&id);
        match key(kind, &path) {
            Ok(declared_key) => match declared.get(&declared_key).copied() {
                Some(first) => {
                    let other = out[first].0.display().to_string();
                    let this = path.display().to_string();
                    out[first].2 = Some(format!(
                        "{other}: {declared_key} is also declared by {this}"
                    ));
                    out.push((
                        path,
                        target,
                        Some(format!(
                            "{this}: {declared_key} is also declared by {other}"
                        )),
                    ));
                }
                None => {
                    declared.insert(declared_key, out.len());
                    out.push((path, target, None));
                }
            },
            Err(error) => out.push((path, target, Some(error.to_string()))),
        }
    }
    out
}

/// Validate `named`, or every authored file when nothing is named.
///
/// Every directory is read either way, because "one statement per key" is a
/// property of the directory and because a file not yet committed is in the
/// named list but not in the listing.
pub fn run(
    contracts: &Path,
    stakes_dir: &Path,
    exclusions_dir: &Path,
    named: &[PathBuf],
) -> Result<()> {
    let sets = [
        (contracts, Kind::Contract),
        (stakes_dir, Kind::Stake),
        (exclusions_dir, Kind::Exclusion),
    ];

    // identity -> (the path to read and print, which directory it came from).
    let mut listed: BTreeMap<PathBuf, (PathBuf, Kind)> = BTreeMap::new();
    for (directory, kind) in sets {
        for path in files::toml(directory)? {
            listed.insert(identity(&path), (path, kind));
        }
    }

    let mut targets: BTreeSet<PathBuf> = BTreeSet::new();
    for path in named {
        let id = identity(path);
        if !listed.contains_key(&id) {
            // A file that has not been moved into place yet: it belongs to the
            // directory it is already sitting in.
            let parent = path.parent().map(identity).unwrap_or_default();
            let Some((_, kind)) = sets
                .iter()
                .find(|(directory, _)| identity(directory) == parent)
            else {
                return Err(Error::new(format!(
                    "{} is not in {}, {} or {}",
                    path.display(),
                    contracts.display(),
                    stakes_dir.display(),
                    exclusions_dir.display()
                )));
            };
            listed.insert(id.clone(), (path.clone(), *kind));
        }
        targets.insert(id);
    }

    let all: BTreeSet<PathBuf> = listed.keys().cloned().collect();
    let targets = if named.is_empty() { &all } else { &targets };

    let mut checked = 0;
    let mut invalid = 0;
    for kind in [Kind::Contract, Kind::Stake, Kind::Exclusion] {
        let set: Vec<(PathBuf, PathBuf)> = listed
            .iter()
            .filter(|(_, (_, found))| *found == kind)
            .map(|(id, (path, _))| (id.clone(), path.clone()))
            .collect();
        for (path, target, error) in outcomes(set, targets, kind) {
            if !target {
                continue;
            }
            checked += 1;
            match error {
                None => println!("ok    {}", path.display()),
                Some(error) => {
                    invalid += 1;
                    println!("FAIL  {error}");
                }
            }
        }
    }

    if invalid > 0 {
        return Err(Error::new(format!(
            "{invalid} of {checked} authored files invalid"
        )));
    }
    println!("authored files valid: {checked}");
    Ok(())
}
