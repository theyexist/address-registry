//! The authored layer's shape: what a script or address is, and where that claim
//! came from.
//!
//! A credential is a **key into a set of statements**, not a single fact. A
//! developer may point one address at more than one job, and a plain key address
//! used in automations holds nothing under a datum at all. So a file declares the
//! credential once and carries one or more `[[use]]` tables, each with its own
//! category, evidence and — only when the thing holds under a script — a shape.
//!
//! The registry lists uses. It never resolves an overload, because only on-chain
//! evidence can: a consumer that finds a datumless UTxO knows which use applies.
//!
//! `derived_from` mirrors `CredentialSource` in `shared-crates/address-registry`:
//! either an address the credential is re-derivable from, or an attestation that
//! no address is registered.

use crate::address::{self, AddressError};
use crate::claim::credential;
use serde::{Deserialize, Serialize};
use std::fmt;

/// What the thing is. The names are the compiled table's, plus two it never had:
/// `lock` for a single-purpose lock and `wallet` for a key address an operator
/// uses.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Exchange,
    Marketplace,
    Launchpad,
    Burn,
    Airdropper,
    Minter,
    Defi,
    Staking,
    Vesting,
    Lock,
    /// A key-controlled address, not a script. It holds nothing under a datum,
    /// and a consumer must not attribute its balance to a holder.
    Wallet,
}

/// How a datum lays out its arguments.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// `Constr 0 [Bytes(owner_pkh)]` — an owner and nothing else.
    OwnerOnly,
    /// `Constr 0 [Int(ms), Bytes(owner_pkh)]` — a timestamp and one owner.
    DeadlineAndOwner,
    /// `Constr 0 [Int(ms), [Bytes(owner_pkh)]]` — a timestamp and a list of
    /// owners. CrowdLock's shape, and deliberately not the singular one.
    DeadlineAndOwners,
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

/// One use of a credential.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub category: Category,
    /// Absent for anything that holds nothing under a datum — a wallet, and a
    /// script whose datum this registry has not been told how to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    /// A real datum observed at this credential, as lowercase hex CBOR.
    ///
    /// It is an observation, not a proof: the registry does not decode it, so it
    /// cannot claim the bytes match `shape`. What it can claim is where they came
    /// from, and a consumer's decoder is what asserts they read as declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum_example: Option<String>,
    pub label: String,
    pub project: String,
    pub source: String,
    pub derived_from: DerivedFrom,
}

/// One credential file: `contracts/<credential>.toml`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractFile {
    pub credential: String,
    #[serde(rename = "use")]
    pub uses: Vec<Contract>,
}

impl ContractFile {
    /// Everything a file must be, given its name. A field added to this shape is
    /// a `format` bump: readers reject keys they do not know rather than
    /// silently skipping them.
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
        if self.uses.is_empty() {
            return Err(Invalid::NoUses);
        }
        for use_of in &self.uses {
            use_of.validate(&self.credential)?;
        }
        Ok(())
    }
}

impl Contract {
    pub fn validate(&self, credential: &str) -> Result<(), Invalid> {
        for (field, value) in [
            ("label", &self.label),
            ("project", &self.project),
            ("source", &self.source),
        ] {
            if value.trim().is_empty() {
                return Err(Invalid::Empty(field));
            }
        }
        if self.category == Category::Wallet && self.shape.is_some() {
            return Err(Invalid::WalletWithShape);
        }
        match (&self.shape, &self.datum_example) {
            (None, Some(_)) => return Err(Invalid::ExampleWithoutShape),
            (Some(_), Some(example)) => {
                let is_hex = example.len() % 2 == 0
                    && !example.is_empty()
                    && example.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && example.bytes().all(|byte| !byte.is_ascii_uppercase());
                if !is_hex {
                    return Err(Invalid::ExampleNotHex);
                }
            }
            _ => {}
        }
        match &self.derived_from {
            DerivedFrom::Address(value) => {
                let payment = address::payment(value).map_err(|error| match error {
                    AddressError::NotBech32 => Invalid::AddressNotBech32,
                    other => Invalid::Address(value.clone(), other.to_string()),
                })?;
                if payment.hex() != credential {
                    return Err(Invalid::AddressIsAnotherCredential(payment.hex()));
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

/// Why a file is not one, named so a caller can prefix it with the path.
#[derive(Debug, Eq, PartialEq)]
pub enum Invalid {
    NotACredential(String),
    NotLowercase(String),
    Filename(String),
    NoUses,
    Empty(&'static str),
    WalletWithShape,
    ExampleWithoutShape,
    ExampleNotHex,
    AddressNotBech32,
    /// A bech32 address that is not a payment address, and why.
    Address(String, String),
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
            Invalid::NoUses => f.write_str("a credential file needs at least one [[use]]"),
            Invalid::Empty(field) => write!(f, "`{field}` is empty"),
            Invalid::WalletWithShape => {
                f.write_str("a wallet holds nothing under a datum, so it has no `shape`")
            }
            Invalid::ExampleWithoutShape => {
                f.write_str("`datum_example` needs a `shape` to be an example of")
            }
            Invalid::ExampleNotHex => f.write_str("`datum_example` must be lowercase hex CBOR"),
            Invalid::AddressNotBech32 => {
                write!(f, "derived_from.address is not a bech32 address")
            }
            Invalid::Address(value, reason) => {
                write!(f, "derived_from.address `{value}` {reason}")
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
    const STAKE: &str = "stake1u8jdj77t835q3gl7pmpjxtl6286awmdvn24ljfddnrf0g6gdvtddv";

    fn use_of(derived_from: DerivedFrom) -> Contract {
        Contract {
            category: Category::Lock,
            shape: Some(Shape::DeadlineAndOwner),
            datum_example: None,
            label: "TSURU Lock".to_string(),
            project: "TSURU".to_string(),
            source: "x".to_string(),
            derived_from,
        }
    }

    fn file(uses: Vec<Contract>) -> ContractFile {
        ContractFile {
            credential: CREDENTIAL.to_string(),
            uses,
        }
    }

    #[test]
    fn a_matching_address_validates() {
        let file = file(vec![use_of(DerivedFrom::Address(ADDRESS.to_string()))]);
        assert_eq!(file.validate(CREDENTIAL), Ok(()));
    }

    #[test]
    fn one_credential_may_carry_several_uses() {
        let mut wallet = use_of(DerivedFrom::Attested("no address".to_string()));
        wallet.category = Category::Wallet;
        wallet.shape = None;
        let file = file(vec![
            use_of(DerivedFrom::Attested("no address".to_string())),
            wallet,
        ]);
        assert_eq!(file.validate(CREDENTIAL), Ok(()));
    }

    #[test]
    fn a_wallet_may_not_carry_a_shape() {
        let mut wallet = use_of(DerivedFrom::Attested("no address".to_string()));
        wallet.category = Category::Wallet;
        assert_eq!(wallet.validate(CREDENTIAL), Err(Invalid::WalletWithShape));
    }

    #[test]
    fn an_example_needs_a_shape() {
        let mut contract = use_of(DerivedFrom::Attested("no address".to_string()));
        contract.shape = None;
        contract.datum_example = Some("d87981".to_string());
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::ExampleWithoutShape)
        );
    }

    #[test]
    fn an_example_must_be_lowercase_hex() {
        let mut contract = use_of(DerivedFrom::Attested("no address".to_string()));
        contract.datum_example = Some("D87981".to_string());
        assert_eq!(contract.validate(CREDENTIAL), Err(Invalid::ExampleNotHex));

        contract.datum_example = Some("d8798".to_string());
        assert_eq!(contract.validate(CREDENTIAL), Err(Invalid::ExampleNotHex));

        contract.datum_example = Some("d87981".to_string());
        assert_eq!(contract.validate(CREDENTIAL), Ok(()));
    }

    #[test]
    fn a_file_needs_a_use() {
        assert_eq!(file(Vec::new()).validate(CREDENTIAL), Err(Invalid::NoUses));
    }

    #[test]
    fn an_address_for_another_credential_is_named() {
        let other = "381cd9b620ce9d7c73dd45cfe207971cc79df79251003116094f6b36";
        let file = file(vec![use_of(DerivedFrom::Address(
            "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w"
                .to_string(),
        ))]);
        assert_eq!(
            file.validate(CREDENTIAL),
            Err(Invalid::AddressIsAnotherCredential(other.to_string()))
        );
    }

    #[test]
    fn a_reward_address_is_not_a_derived_from() {
        let file = file(vec![use_of(DerivedFrom::Address(STAKE.to_string()))]);
        assert!(matches!(
            file.validate(CREDENTIAL),
            Err(Invalid::Address(_, _))
        ));
    }

    #[test]
    fn a_corrupted_address_is_not_bech32() {
        let file = file(vec![use_of(DerivedFrom::Address(
            "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k".to_string(),
        ))]);
        assert_eq!(file.validate(CREDENTIAL), Err(Invalid::AddressNotBech32));
    }

    #[test]
    fn the_filename_must_be_the_credential() {
        let file = file(vec![use_of(DerivedFrom::Attested(
            "no address".to_string(),
        ))]);
        assert_eq!(
            file.validate("someone-else"),
            Err(Invalid::Filename(CREDENTIAL.to_string()))
        );
    }

    #[test]
    fn an_empty_attestation_is_rejected() {
        let file = file(vec![use_of(DerivedFrom::Attested("  ".to_string()))]);
        assert_eq!(file.validate(CREDENTIAL), Err(Invalid::EmptyAttestation));
    }
}
