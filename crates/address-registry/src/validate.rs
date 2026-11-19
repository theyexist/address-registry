//! `validate`: is each contract file one the schema accepts, and does the
//! directory still hold one file per credential?
//!
//! `resolve` loads the same directory, so an invalid file fails there too. This
//! command exists for the two things that failure does not give a contributor:
//! the file named, and every failing file in one run rather than only the first
//! one the loader reaches. CI runs it on the files a pull request changed.
//!
//! Nothing here writes, fetches or resolves. A valid file is a statement about
//! the file, not about the registry: an address that decodes is not a contract
//! that behaves as declared.

use crate::contract;
use crate::error::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// What a path resolves to, so `./contracts/x.toml`, `contracts/x.toml` and an
/// absolute path are one file rather than two declarations of one credential. A
/// path that does not resolve is kept as given, so a missing file is reported by
/// `read` instead of disappearing here.
fn identity(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Validate `named`, or every file in `dir` when nothing is named.
///
/// The whole directory is read either way, because "two files declare one
/// credential" is a property of the directory and because a file not yet
/// committed is in the named list but not in the listing.
pub fn run(dir: &Path, named: &[PathBuf]) -> Result<()> {
    // identity -> the path to read and print. The directory is the index; a
    // named file that is not in it yet — a new contract, before it moves — is
    // added to it.
    let mut files: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    for path in contract::files(dir)? {
        files.insert(identity(&path), path);
    }
    for path in named {
        files.entry(identity(path)).or_insert_with(|| path.clone());
    }

    let targets: BTreeSet<PathBuf> = if named.is_empty() {
        files.keys().cloned().collect()
    } else {
        named.iter().map(|path| identity(path)).collect()
    };

    // One outcome per file. A duplicate marks both: only one of them is usually
    // in the change, and naming the other is what makes the pair fixable.
    let mut outcomes: Vec<(PathBuf, bool, Option<String>)> = Vec::new();
    let mut declared_by: BTreeMap<String, usize> = BTreeMap::new();
    for (id, path) in files {
        let target = targets.contains(&id);
        match contract::read(&path) {
            Ok(file) => match declared_by.get(&file.credential).copied() {
                Some(first) => {
                    let other = outcomes[first].0.display().to_string();
                    let this = path.display().to_string();
                    outcomes[first].2 = Some(format!(
                        "{other}: {} is also declared by {this}",
                        file.credential
                    ));
                    outcomes.push((
                        path,
                        target,
                        Some(format!(
                            "{this}: {} is also declared by {other}",
                            file.credential
                        )),
                    ));
                }
                None => {
                    declared_by.insert(file.credential, outcomes.len());
                    outcomes.push((path, target, None));
                }
            },
            Err(error) => outcomes.push((path, target, Some(error.to_string()))),
        }
    }

    let mut checked = 0;
    let mut invalid = 0;
    for (path, target, error) in &outcomes {
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

    if invalid > 0 {
        return Err(Error::new(format!(
            "{invalid} of {checked} contract files invalid"
        )));
    }
    println!("{checked} contract files valid");
    Ok(())
}
