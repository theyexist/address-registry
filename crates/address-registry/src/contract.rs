//! Loading `contracts/*.toml` into the schema's type.
//!
//! The shape and its rules live in `address-registry-schema`; this is the part
//! that needs a filesystem. One file per credential, named after it, so the
//! directory listing is the index and a misplaced row is an error rather than a
//! row nobody reads.
//!
//! A file carries one or more `[[use]]` tables. Two files for one credential is
//! still an error: the uses of a credential belong together, where a reader can
//! see the overload.

use crate::error::{Error, Result};
use address_registry_schema::{Contract, ContractFile};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The `*.toml` files in `dir`, sorted. The directory listing is the index, so
/// every reader of the authored layer starts here rather than deciding for itself
/// what counts as a contract file.
pub fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| Error::at(dir, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| Error::at(dir, error))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Parse one file and check it against its own name.
///
/// The directory-wide rule — two files for one credential — needs the directory,
/// so `load` is what enforces it. This is the part a single edited file can be
/// held to on its own.
pub fn read(path: &Path) -> Result<ContractFile> {
    // A contract file is `<credential>.toml`; any other extension would parse and
    // pass its name check while `load` never looked at it.
    if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
        return Err(Error::at(path, "a contract file is `<credential>.toml`"));
    }
    let text = fs::read_to_string(path).map_err(|error| Error::at(path, error))?;
    let file: ContractFile = toml::from_str(&text).map_err(|error| Error::at(path, error))?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    file.validate(stem)
        .map_err(|invalid| Error::at(path, invalid))?;
    Ok(file)
}

pub fn load(dir: &Path) -> Result<BTreeMap<String, Vec<Contract>>> {
    let mut contracts = BTreeMap::new();
    let mut declared_by: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in files(dir)? {
        let file = read(&path)?;
        if let Some(first) = declared_by.get(&file.credential) {
            return Err(Error::at(
                &path,
                format!(
                    "{} is already declared by {}",
                    file.credential,
                    first.display()
                ),
            ));
        }
        declared_by.insert(file.credential.clone(), path);
        contracts.insert(file.credential, file.uses);
    }
    Ok(contracts)
}
