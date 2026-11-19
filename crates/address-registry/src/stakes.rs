//! Loading the stake-service layer into the schema's types.
//!
//! Two directories, one file per stake address: `stakes/` for a service and
//! `stake-exclusions/` for a key deliberately not one. Naming the file after the
//! key is what makes the listing the index — a misplaced row is an error rather
//! than a row nobody reads.

use crate::error::{Error, Result};
use crate::files;
use address_registry_schema::{StakeExclusion, StakeService};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn stem(path: &Path) -> &str {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
}

pub fn load_services(dir: &Path) -> Result<Vec<StakeService>> {
    let mut services = Vec::new();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in files::toml(dir)? {
        let text = fs::read_to_string(&path).map_err(|error| Error::at(&path, error))?;
        let service: StakeService =
            toml::from_str(&text).map_err(|error| Error::at(&path, error))?;
        service
            .validate(stem(&path))
            .map_err(|invalid| Error::at(&path, invalid))?;
        if let Some(first) = seen.get(&service.stake) {
            return Err(Error::at(
                &path,
                format!(
                    "{} is already declared by {}",
                    service.stake,
                    first.display()
                ),
            ));
        }
        seen.insert(service.stake.clone(), path);
        services.push(service);
    }
    services.sort_by(|a, b| a.stake.cmp(&b.stake));
    Ok(services)
}

pub fn load_exclusions(dir: &Path) -> Result<Vec<StakeExclusion>> {
    let mut exclusions = Vec::new();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in files::toml(dir)? {
        let text = fs::read_to_string(&path).map_err(|error| Error::at(&path, error))?;
        let exclusion: StakeExclusion =
            toml::from_str(&text).map_err(|error| Error::at(&path, error))?;
        exclusion
            .validate(stem(&path))
            .map_err(|invalid| Error::at(&path, invalid))?;
        if let Some(first) = seen.get(&exclusion.stake) {
            return Err(Error::at(
                &path,
                format!(
                    "{} is already declared by {}",
                    exclusion.stake,
                    first.display()
                ),
            ));
        }
        seen.insert(exclusion.stake.clone(), path);
        exclusions.push(exclusion);
    }
    exclusions.sort_by(|a, b| a.stake.cmp(&b.stake));
    Ok(exclusions)
}

/// A stake cannot be both a service and an excluded key; the pair would name
/// nothing and deny it at once.
pub fn check_disjoint(services: &[StakeService], exclusions: &[StakeExclusion]) -> Result<()> {
    let named: BTreeMap<&str, ()> = services.iter().map(|s| (s.stake.as_str(), ())).collect();
    for exclusion in exclusions {
        if named.contains_key(exclusion.stake.as_str()) {
            return Err(Error::new(format!(
                "{} is both a service and an exclusion",
                exclusion.stake
            )));
        }
    }
    Ok(())
}
