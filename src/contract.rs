//! The authored layer: what a script is, and how its datum names its owner.
//!
//! One file per credential, `contracts/<credential>.toml`. Nobody upstream
//! carries a `kind` or a datum `shape`, so this is the only place a behaviour is
//! stated — and every row has to say where the statement came from. A row
//! without a `source` would be a fabrication with a filename.
//!
//! `derived_from` mirrors `CredentialSource` in `shared-crates/address-registry`
//! deliberately: either an address the credential is re-derivable from, or an
//! attestation that no address is registered. The first is decoded and compared
//! here; the second stays visible in the resolved report, so an unverifiable
//! entry never looks like a checked one.

use crate::bech32;
use crate::claim::credential;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Lock,
    Staking,
}

/// How the datum lays out its arguments. The shape says where the owner is; the
/// `kind` says what the other field means, which is why they are separate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// `Constr 0 [Bytes(owner_pkh)]` — an owner and nothing else.
    OwnerOnly,
    /// `Constr 0 [Int(ms), Bytes(owner_pkh)]` — a timestamp and an owner.
    DeadlineAndOwner,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DerivedFrom {
    /// A bech32 address whose payment credential is this one.
    Address(String),
    /// No address registered, so the credential cannot be re-derived here.
    /// The string says why, and stays in the report so the gap is visible.
    Attested(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub credential: String,
    pub kind: Kind,
    pub shape: Shape,
    pub label: String,
    pub project: String,
    pub source: String,
    pub derived_from: DerivedFrom,
}

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
        contract.validate(&path)?;
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

impl Contract {
    fn validate(&self, path: &Path) -> Result<()> {
        let Some(lowered) = credential(&self.credential) else {
            return Err(Error::at(
                path,
                format!("`{}` is not 56 hex digits", self.credential),
            ));
        };
        if lowered != self.credential {
            return Err(Error::at(
                path,
                format!("`{}` must be written lowercase", self.credential),
            ));
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        if stem != Some(self.credential.as_str()) {
            return Err(Error::at(
                path,
                format!(
                    "the file must be `{}.toml`, named after its credential",
                    self.credential
                ),
            ));
        }
        for (field, value) in [
            ("label", &self.label),
            ("project", &self.project),
            ("source", &self.source),
        ] {
            if value.trim().is_empty() {
                return Err(Error::at(path, format!("`{field}` is empty")));
            }
        }
        match &self.derived_from {
            DerivedFrom::Address(address) => {
                let decoded = bech32::payment_credential(address).ok_or_else(|| {
                    Error::at(
                        path,
                        format!("derived_from.address `{address}` is not a bech32 address"),
                    )
                })?;
                if decoded != self.credential {
                    return Err(Error::at(
                        path,
                        format!(
                            "derived_from.address decodes to {decoded}, which is not this credential"
                        ),
                    ));
                }
            }
            DerivedFrom::Attested(reason) => {
                if reason.trim().is_empty() {
                    return Err(Error::at(
                        path,
                        "derived_from.attested needs a reason: it is the visible gap".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREDENTIAL: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
    const ADDRESS: &str = "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j";

    fn parse(body: &str) -> std::result::Result<Contract, toml::de::Error> {
        toml::from_str(body)
    }

    #[test]
    fn the_authored_shape_parses() {
        let contract = parse(&format!(
            r#"
credential = "{CREDENTIAL}"
kind = "lock"
shape = "deadline_and_owner"
label = "TSURU Lock"
project = "TSURU"
source = "datum Constr 0 [Int(ms), Bytes(owner_pkh)]"

[derived_from]
address = "{ADDRESS}"
"#
        ))
        .expect("the authored shape parses");
        assert_eq!(contract.kind, Kind::Lock);
        assert_eq!(contract.shape, Shape::DeadlineAndOwner);
        assert_eq!(
            contract.derived_from,
            DerivedFrom::Address(ADDRESS.to_string())
        );
    }

    #[test]
    fn an_attested_row_parses() {
        let contract = parse(&format!(
            r#"
credential = "{CREDENTIAL}"
kind = "staking"
shape = "owner_only"
label = "TSURU Staking"
project = "TSURU"
source = "datum Constr 0 [Bytes(owner_pkh)]"

[derived_from]
attested = "no address registered"
"#
        ))
        .expect("the attested shape parses");
        assert_eq!(contract.kind, Kind::Staking);
        assert_eq!(
            contract.derived_from,
            DerivedFrom::Attested("no address registered".to_string())
        );
    }

    #[test]
    fn an_unknown_field_is_an_error_not_a_silent_drop() {
        let error = parse(&format!(
            r#"
credential = "{CREDENTIAL}"
kind = "lock"
shape = "owner_only"
label = "x"
project = "x"
source = "x"
knd = "typo"

[derived_from]
attested = "x"
"#
        ))
        .expect_err("a typo must not parse");
        assert!(error.to_string().contains("knd"), "{error}");
    }

    #[test]
    fn a_derived_from_address_must_decode_to_the_credential() {
        let path = Path::new(CREDENTIAL).with_extension("toml");
        let contract = Contract {
            credential: CREDENTIAL.to_string(),
            kind: Kind::Lock,
            shape: Shape::DeadlineAndOwner,
            label: "TSURU Lock".to_string(),
            project: "TSURU".to_string(),
            source: "x".to_string(),
            derived_from: DerivedFrom::Address(
                "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k".to_string(),
            ),
        };
        let error = contract
            .validate(&path)
            .expect_err("a corrupted address must not validate");
        assert!(error.to_string().contains("bech32"), "{error}");
    }

    #[test]
    fn the_filename_must_be_the_credential() {
        let contract = Contract {
            credential: CREDENTIAL.to_string(),
            kind: Kind::Lock,
            shape: Shape::OwnerOnly,
            label: "x".to_string(),
            project: "x".to_string(),
            source: "x".to_string(),
            derived_from: DerivedFrom::Attested("x".to_string()),
        };
        let error = contract
            .validate(Path::new("someone-else.toml"))
            .expect_err("a misnamed file must not validate");
        assert!(
            error.to_string().contains("named after its credential"),
            "{error}"
        );
    }

    #[test]
    fn an_uppercase_credential_is_rejected() {
        let contract = Contract {
            credential: CREDENTIAL.to_uppercase(),
            kind: Kind::Lock,
            shape: Shape::OwnerOnly,
            label: "x".to_string(),
            project: "x".to_string(),
            source: "x".to_string(),
            derived_from: DerivedFrom::Attested("x".to_string()),
        };
        let error = contract
            .validate(&PathBuf::from(format!("{CREDENTIAL}.toml")))
            .expect_err("uppercase must not validate");
        assert!(error.to_string().contains("lowercase"), "{error}");
    }
}
