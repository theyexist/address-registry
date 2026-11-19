//! The authored layer's shape: what a script is, and where that claim came from.
//!
//! `derived_from` mirrors `CredentialSource` in `shared-crates/address-registry`
//! deliberately: either an address the credential is re-derivable from, or an
//! attestation that no address is registered. The first is decoded and compared
//! here; the second is what the resolve report counts, so an unverifiable row
//! never looks like a checked one.

use crate::bech32;
use crate::claim::credential;
use serde::{Deserialize, Serialize};
use std::fmt;

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
    /// No address registered, so the credential cannot be re-derived here. The
    /// string says why, and stays in the report so the gap is visible.
    Attested(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl Contract {
    /// Everything a contract must be, given the name of the file it came from.
    /// A field added to this shape is a `format` bump: readers reject keys they
    /// do not know rather than silently skipping them.
    pub fn validate(&self, file_stem: &str) -> Result<(), Invalid> {
        let Some(lowered) = credential(&self.credential) else {
            return Err(Invalid::NotACredential(self.credential.clone()));
        };
        if lowered != self.credential {
            return Err(Invalid::NotLowercase(self.credential.clone()));
        }
        if file_stem != self.credential {
            return Err(Invalid::Filename(self.credential.clone()));
        }
        for (field, value) in [
            ("label", &self.label),
            ("project", &self.project),
            ("source", &self.source),
        ] {
            if value.trim().is_empty() {
                return Err(Invalid::Empty(field));
            }
        }
        match &self.derived_from {
            DerivedFrom::Address(address) => {
                let decoded =
                    bech32::payment_credential(address).ok_or(Invalid::AddressNotBech32)?;
                if decoded != self.credential {
                    return Err(Invalid::AddressIsAnotherCredential(decoded));
                }
            }
            DerivedFrom::Attested(reason) => {
                if reason.trim().is_empty() {
                    return Err(Invalid::EmptyAttestation);
                }
            }
        }
        Ok(())
    }
}

/// Why a contract is not one, named so a caller can prefix it with the file.
#[derive(Debug, Eq, PartialEq)]
pub enum Invalid {
    NotACredential(String),
    NotLowercase(String),
    Filename(String),
    Empty(&'static str),
    AddressNotBech32,
    AddressIsAnotherCredential(String),
    EmptyAttestation,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Invalid::NotACredential(value) => write!(f, "`{value}` is not 56 hex digits"),
            Invalid::NotLowercase(value) => write!(f, "`{value}` must be written lowercase"),
            Invalid::Filename(value) => {
                write!(
                    f,
                    "the file must be `{value}.toml`, named after its credential"
                )
            }
            Invalid::Empty(field) => write!(f, "`{field}` is empty"),
            Invalid::AddressNotBech32 => {
                write!(f, "derived_from.address is not a bech32 address")
            }
            Invalid::AddressIsAnotherCredential(found) => write!(
                f,
                "derived_from.address decodes to {found}, which is not this credential"
            ),
            Invalid::EmptyAttestation => {
                write!(
                    f,
                    "derived_from.attested needs a reason: it is the visible gap"
                )
            }
        }
    }
}

impl std::error::Error for Invalid {}

#[cfg(test)]
mod tests {
    use super::*;

    const CREDENTIAL: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
    const ADDRESS: &str = "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j";

    fn contract(derived_from: DerivedFrom) -> Contract {
        Contract {
            credential: CREDENTIAL.to_string(),
            kind: Kind::Lock,
            shape: Shape::OwnerOnly,
            label: "TSURU Lock".to_string(),
            project: "TSURU".to_string(),
            source: "x".to_string(),
            derived_from,
        }
    }

    #[test]
    fn a_matching_address_validates() {
        let contract = contract(DerivedFrom::Address(ADDRESS.to_string()));
        assert_eq!(contract.validate(CREDENTIAL), Ok(()));
    }

    #[test]
    fn an_address_for_another_credential_is_named() {
        let other = "381cd9b620ce9d7c73dd45cfe207971cc79df79251003116094f6b36";
        let contract = contract(DerivedFrom::Address(
            "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w"
                .to_string(),
        ));
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::AddressIsAnotherCredential(other.to_string()))
        );
    }

    #[test]
    fn a_corrupted_address_is_not_bech32() {
        let contract = contract(DerivedFrom::Address(
            "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k".to_string(),
        ));
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::AddressNotBech32)
        );
    }

    #[test]
    fn the_filename_must_be_the_credential() {
        let contract = contract(DerivedFrom::Attested("no address".to_string()));
        assert_eq!(
            contract.validate("someone-else"),
            Err(Invalid::Filename(CREDENTIAL.to_string()))
        );
    }

    #[test]
    fn an_uppercase_credential_is_rejected() {
        let mut contract = contract(DerivedFrom::Attested("no address".to_string()));
        contract.credential = CREDENTIAL.to_uppercase();
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::NotLowercase(CREDENTIAL.to_uppercase()))
        );
    }

    #[test]
    fn an_empty_attestation_is_rejected() {
        let contract = contract(DerivedFrom::Attested("  ".to_string()));
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::EmptyAttestation)
        );
    }
}
