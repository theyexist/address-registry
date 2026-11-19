//! Reading the two public registries from local clones.
//!
//! Nothing is fetched: an import is reproducible from the revisions recorded in
//! `imported/registry.json`. Strica's `projects/*.json` and Eternl's
//! `registry/projects/*.json` are the sources. Eternl's generated
//! `registry/scripts/script-index.json` is **not** a source — it is measured and
//! reported as drift, never imported (see `index_observation`).

use crate::claim::{credential, Audit, Claim, EternlClaim, Source, StricaClaim};
use crate::error::{Error, Result};
use crate::output::{IndexObservation, Skipped};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Collected {
    pub claims: Vec<(String, Claim)>,
    pub skipped: Vec<Skipped>,
    pub projects: usize,
}

pub fn git_rev(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rev = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!rev.is_empty()).then_some(rev)
}

pub fn collect_strica(root: &Path) -> Result<Collected> {
    let dir = root.join("projects");
    let files = json_files(&dir)?;
    let projects = files.len();
    let mut claims = Vec::new();
    let mut skipped = Vec::new();

    for path in files {
        let doc: StricaProject = read_json(&path)?;
        let file = file_name(&path);
        let project = doc
            .project_name
            .clone()
            .or(doc.label_prefix.clone())
            .unwrap_or_else(|| file.clone());
        let label_prefix = doc.label_prefix.clone().unwrap_or_else(|| project.clone());

        for contract in doc.contracts.unwrap_or_default() {
            let raw = contract.script_hash.clone().unwrap_or_default();
            let Some(credential) = credential(&raw) else {
                skipped.push(Skipped {
                    source: Source::Strica,
                    file: format!("projects/{file}"),
                    script_hash: contract.script_hash,
                    reason: "not a 56-hex payment credential".to_string(),
                });
                continue;
            };
            claims.push((
                credential,
                Claim::Strica(StricaClaim {
                    project: project.clone(),
                    label_prefix: label_prefix.clone(),
                    contract: contract.name,
                    version: contract.version,
                    language: contract.language,
                    language_version: contract.language_version,
                    github: contract.github,
                    description: contract.description,
                    category: doc.category.clone(),
                    website: doc.website.clone(),
                    audits: contract.audit.unwrap_or_default(),
                }),
            ));
        }
    }

    Ok(Collected {
        claims,
        skipped,
        projects,
    })
}

pub fn collect_eternl(root: &Path) -> Result<Collected> {
    let dir = root.join("registry").join("projects");
    let files = json_files(&dir)?;
    let projects = files.len();
    let mut claims = Vec::new();
    let mut skipped = Vec::new();

    for path in files {
        let doc: EternlProject = read_json(&path)?;
        let file = file_name(&path);
        let project = doc.label.clone().unwrap_or_else(|| {
            file.strip_suffix(".json")
                .unwrap_or(file.as_str())
                .to_string()
        });
        let link = doc.link.clone().unwrap_or_default();

        for script in doc.scripts.unwrap_or_default() {
            let raw = script.script_hash.clone().unwrap_or_default();
            let Some(credential) = credential(&raw) else {
                skipped.push(Skipped {
                    source: Source::Eternl,
                    file: format!("registry/projects/{file}"),
                    script_hash: script.script_hash,
                    reason: "not a 56-hex payment credential".to_string(),
                });
                continue;
            };
            claims.push((
                credential,
                Claim::Eternl(EternlClaim {
                    project: project.clone(),
                    contract: script.name,
                    purpose: script.purpose,
                    script_type: script.script_type,
                    plutus_version: script.plutus_version,
                    category: doc.category.clone(),
                    sub_category: doc.sub_category.clone(),
                    website: link.website.clone(),
                    github: link.github.clone(),
                }),
            ));
        }
    }

    Ok(Collected {
        claims,
        skipped,
        projects,
    })
}

/// The upstream's generated index merges into whatever was there before — its
/// generator is explicitly non-destructive — so it accumulates rows that no
/// longer exist in any project file. It is a cache, not a third source: report
/// what it holds and how far it has drifted, import none of it.
pub fn index_observation(
    root: &Path,
    project_hashes: &BTreeSet<String>,
) -> Result<Option<IndexObservation>> {
    let path = root
        .join("registry")
        .join("scripts")
        .join("script-index.json");
    if !path.exists() {
        return Ok(None);
    }
    let doc: EternlIndex = read_json(&path)?;
    let hashes: Vec<&String> = doc
        .scripts
        .as_ref()
        .map(|scripts| scripts.keys().collect())
        .unwrap_or_default();
    let credentials: Vec<&String> = hashes
        .iter()
        .copied()
        .filter(|hash| credential(hash).is_some())
        .collect();
    Ok(Some(IndexObservation {
        generated_at: doc.metadata.and_then(|meta| meta.generated_at),
        scripts: hashes.len(),
        credentials: credentials.len(),
        not_in_project_files: credentials
            .iter()
            .filter(|hash| !project_hashes.contains(**hash))
            .count(),
        longer_than_a_credential: hashes.len() - credentials.len(),
    }))
}

fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| Error::at(dir, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| Error::at(dir, error))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|error| Error::at(path, error))?;
    serde_json::from_str(&text).map_err(|error| Error::at(path, error))
}

#[derive(Deserialize)]
struct StricaProject {
    #[serde(rename = "projectName")]
    project_name: Option<String>,
    #[serde(rename = "labelPrefix")]
    label_prefix: Option<String>,
    category: Option<String>,
    website: Option<String>,
    contracts: Option<Vec<StricaContract>>,
}

#[derive(Deserialize)]
struct StricaContract {
    #[serde(rename = "scriptHash")]
    script_hash: Option<String>,
    name: Option<String>,
    version: Option<i64>,
    language: Option<String>,
    #[serde(rename = "languageVersion")]
    language_version: Option<i64>,
    github: Option<String>,
    description: Option<String>,
    audit: Option<Vec<Audit>>,
}

#[derive(Deserialize)]
struct EternlProject {
    label: Option<String>,
    category: Option<String>,
    #[serde(rename = "subCategory")]
    sub_category: Option<String>,
    link: Option<EternlLink>,
    scripts: Option<Vec<EternlScript>>,
}

#[derive(Clone, Default, Deserialize)]
struct EternlLink {
    website: Option<String>,
    github: Option<String>,
}

#[derive(Deserialize)]
struct EternlScript {
    #[serde(rename = "scriptHash")]
    script_hash: Option<String>,
    name: Option<String>,
    purpose: Option<String>,
    #[serde(rename = "type")]
    script_type: Option<String>,
    #[serde(rename = "plutusVersion")]
    plutus_version: Option<i64>,
}

#[derive(Deserialize)]
struct EternlIndex {
    metadata: Option<EternlIndexMeta>,
    /// Only the keys are read. The values are the same scripts in another
    /// shape, and none of them are imported.
    scripts: Option<BTreeMap<String, IgnoredAny>>,
}

#[derive(Deserialize)]
struct EternlIndexMeta {
    #[serde(rename = "generatedAt")]
    generated_at: Option<String>,
}
