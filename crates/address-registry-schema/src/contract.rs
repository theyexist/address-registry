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

/// What a marketplace contract is for. Typed rather than left in the label,
/// because a consumer selects behaviour by it: `fold-probe` reads offers and must
/// not read a sale escrow as one, and the two are otherwise the same category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketplacePurpose {
    Offer,
    Sale,
    Fee,
}

/// How a datum lays out its arguments.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// `Constr 0 [Bytes(owner_pkh)]` — an owner and nothing else.
    OwnerOnly,
    /// `Constr 0 [Int(ms), Bytes(owner_pkh)]` — a timestamp and one owner.
    /// Whether that timestamp is a deadline or a stake date is the category's
    /// business: `$PERP` staking records when a position was staked.
    TimestampAndOwner,
    /// `Constr 0 [Int(ms), [Bytes(owner_pkh)]]` — a timestamp and a list of
    /// owners. CrowdLock's shape, and deliberately not the singular one.
    TimestampAndOwners,
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
    /// For a marketplace, what the contract is for. Absent for every other
    /// category, and refused there: a purpose is a marketplace's fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<MarketplacePurpose>,
    /// For a marketplace, WHICH venue, as a machine key (`jpg.store`, `wayup`).
    /// The label is prose and may say anything; this is what a consumer matches
    /// to pick a decoder, and it is refused outside a marketplace use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
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
    /// What the thing is called. Absent for a category that carries no name of
    /// its own — a burn sink is identified by the category and its evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Who operates it. Absent when the source does not say, which is most of
    /// the compiled table — naming one would be a guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub source: String,
    pub derived_from: DerivedFrom,
    /// Further addresses this use is registered at — one script under another
    /// delegation form — beyond the one `derived_from` records. Empty for the
    /// common case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_at: Vec<String>,
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
        if self.source.trim().is_empty() {
            return Err(Invalid::Empty("source"));
        }
        for (field, value) in [("label", &self.label), ("project", &self.project)] {
            if value.as_ref().is_some_and(|value| value.trim().is_empty()) {
                return Err(Invalid::Empty(field));
            }
        }
        if self.purpose.is_some() && self.category != Category::Marketplace {
            return Err(Invalid::PurposeWithoutMarketplace);
        }
        if let Some(market) = &self.market {
            if self.category != Category::Marketplace {
                return Err(Invalid::MarketWithoutMarketplace);
            }
            // A key, not prose: lowercase, and nothing a URL or a shell would
            // have to quote.
            let is_key = !market.is_empty()
                && market.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'-')
                });
            if !is_key {
                return Err(Invalid::MarketNotAKey(market.clone()));
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
                let mut seen =
                    vec![crate::bech32::payload(value).ok_or(Invalid::AddressNotBech32)?];
                for form in &self.also_at {
                    let payment = address::payment(form).map_err(|error| match error {
                        AddressError::NotBech32 => Invalid::AddressNotBech32,
                        other => Invalid::Address(form.clone(), other.to_string()),
                    })?;
                    if payment.hex() != credential {
                        return Err(Invalid::AddressIsAnotherCredential(payment.hex()));
                    }
                    // Duplicates are compared as BYTES: every form here shares the
                    // credential, so the credential cannot tell two forms apart.
                    let payload = crate::bech32::payload(form).ok_or(Invalid::AddressNotBech32)?;
                    if seen.contains(&payload) {
                        return Err(Invalid::AddressRepeated(form.clone()));
                    }
                    seen.push(payload);
                }
            }
            DerivedFrom::Attested(reason) => {
                if !self.also_at.is_empty() {
                    return Err(Invalid::AlsoWithoutAddress);
                }
                if reason.trim().is_empty() {
                    return Err(Invalid::EmptyAttestation);
                }
            }
        }
        Ok(())
    }

    /// Every address this use is registered at: the claim's own, then any other
    /// delegation form. Empty for an attested use.
    pub fn addresses(&self) -> impl Iterator<Item = &str> {
        let primary = match &self.derived_from {
            DerivedFrom::Address(address) => Some(address.as_str()),
            DerivedFrom::Attested(_) => None,
        };
        primary
            .into_iter()
            .chain(self.also_at.iter().map(String::as_str))
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
    PurposeWithoutMarketplace,
    MarketWithoutMarketplace,
    MarketNotAKey(String),
    WalletWithShape,
    ExampleWithoutShape,
    ExampleNotHex,
    AddressNotBech32,
    /// A bech32 address that is not a payment address, and why.
    Address(String, String),
    AddressIsAnotherCredential(String),
    AddressRepeated(String),
    AlsoWithoutAddress,
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
            Invalid::PurposeWithoutMarketplace => {
                f.write_str("`purpose` belongs to a `marketplace` use and nothing else")
            }
            Invalid::MarketWithoutMarketplace => {
                f.write_str("`market` belongs to a `marketplace` use and nothing else")
            }
            Invalid::MarketNotAKey(value) => write!(
                f,
                "`{value}` is not a market key: lowercase letters, digits, `.` and `-`"
            ),
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
            Invalid::AddressRepeated(value) => {
                write!(f, "derived_from lists `{value}` twice")
            }
            Invalid::AlsoWithoutAddress => {
                f.write_str("`also_at` needs a `derived_from.address` to be another form of")
            }
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
            purpose: None,
            market: None,
            shape: Some(Shape::TimestampAndOwner),
            datum_example: None,
            label: Some("TSURU Lock".to_string()),
            project: Some("TSURU".to_string()),
            source: "x".to_string(),
            derived_from,
            also_at: Vec::new(),
        }
    }

    fn at(address: &str) -> DerivedFrom {
        DerivedFrom::Address(address.to_string())
    }

    fn file(uses: Vec<Contract>) -> ContractFile {
        ContractFile {
            credential: CREDENTIAL.to_string(),
            uses,
        }
    }

    #[test]
    fn a_matching_address_validates() {
        let file = file(vec![use_of(at(ADDRESS))]);
        assert_eq!(file.validate(CREDENTIAL), Ok(()));
    }

    #[test]
    fn another_delegation_form_of_the_same_script_validates() {
        // jpg.store's V1 escrow: one script, an addr1z sale form and an addr1x
        // offer form, both deriving the same payment credential.
        const JPG: &str = "9068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad";
        const SALE: &str = "addr1zxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tvpw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspks905plm";
        const OFFER: &str = "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd";
        let mut contract = use_of(at(SALE));
        contract.category = Category::Marketplace;
        contract.purpose = Some(MarketplacePurpose::Offer);
        contract.also_at = vec![OFFER.to_string()];
        assert_eq!(contract.validate(JPG), Ok(()));
        assert_eq!(contract.addresses().count(), 2);
    }

    #[test]
    fn an_also_at_address_must_derive_the_same_credential() {
        const OTHER: &str = "381cd9b620ce9d7c73dd45cfe207971cc79df79251003116094f6b36";
        let mut contract = use_of(at(ADDRESS));
        contract.also_at = vec![
            "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w"
                .to_string(),
        ];
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::AddressIsAnotherCredential(OTHER.to_string()))
        );

        contract.also_at = vec![ADDRESS.to_string()];
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::AddressRepeated(ADDRESS.to_string()))
        );

        let mut attested = use_of(DerivedFrom::Attested("no address".to_string()));
        attested.also_at = vec![ADDRESS.to_string()];
        assert_eq!(
            attested.validate(CREDENTIAL),
            Err(Invalid::AlsoWithoutAddress)
        );
    }

    #[test]
    fn a_use_names_every_address_it_is_registered_at() {
        let contract = use_of(at(ADDRESS));
        assert_eq!(contract.addresses().collect::<Vec<_>>(), vec![ADDRESS]);

        let attested = use_of(DerivedFrom::Attested("no address".to_string()));
        assert_eq!(attested.addresses().count(), 0);
    }

    #[test]
    fn a_purpose_belongs_to_a_marketplace_and_nowhere_else() {
        let mut contract = use_of(DerivedFrom::Attested("no address".to_string()));
        contract.purpose = Some(MarketplacePurpose::Offer);
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::PurposeWithoutMarketplace)
        );

        contract.category = Category::Marketplace;
        assert_eq!(contract.validate(CREDENTIAL), Ok(()));
    }

    #[test]
    fn a_market_key_is_machine_shaped_and_marketplace_only() {
        let mut contract = use_of(DerivedFrom::Attested("no address".to_string()));
        contract.market = Some("jpg.store".to_string());
        assert_eq!(
            contract.validate(CREDENTIAL),
            Err(Invalid::MarketWithoutMarketplace)
        );

        contract.category = Category::Marketplace;
        assert_eq!(contract.validate(CREDENTIAL), Ok(()));

        for bad in ["JPG.store", "jpg store", "", "way_up"] {
            contract.market = Some(bad.to_string());
            assert_eq!(
                contract.validate(CREDENTIAL),
                Err(Invalid::MarketNotAKey(bad.to_string())),
                "{bad:?} should not be a key"
            );
        }
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
        let file = file(vec![use_of(at(
            "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w",
        ))]);
        assert_eq!(
            file.validate(CREDENTIAL),
            Err(Invalid::AddressIsAnotherCredential(other.to_string()))
        );
    }

    #[test]
    fn a_reward_address_is_not_a_derived_from() {
        let file = file(vec![use_of(at(STAKE))]);
        assert!(matches!(
            file.validate(CREDENTIAL),
            Err(Invalid::Address(_, _))
        ));
    }

    #[test]
    fn a_corrupted_address_is_not_bech32() {
        let file = file(vec![use_of(at(
            "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k",
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
