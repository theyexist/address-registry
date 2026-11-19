//! Loading `contracts/*.toml` into the schema's type.
//!
//! The shape and its rules live in `address-registry-schema`; this is the part
//! that needs a filesystem. One file per credential, named after it, so the
//! directory listing is the index and a misplaced row is an error rather than a
//! row nobody reads.

use crate::error::{Error, Result};
use address_registry_schema::Contract;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn load(dir: &Path) -> Result<BTreeMap<String, Contract>> {
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

    let mut contracts = BTreeMap::new();
    let mut declared_by: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(|error| Error::at(&path, error))?;
        let contract: Contract = toml::from_str(&text).map_err(|error| Error::at(&path, error))?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        contract
            .validate(stem)
            .map_err(|invalid| Error::at(&path, invalid))?;
        if let Some(first) = declared_by.get(&contract.credential) {
            return Err(Error::at(
                &path,
                format!(
                    "{} is already declared by {}",
                    contract.credential,
                    first.display()
                ),
            ));
        }
        declared_by.insert(contract.credential.clone(), path);
        contracts.insert(contract.credential.clone(), contract);
    }
    Ok(contracts)
}
