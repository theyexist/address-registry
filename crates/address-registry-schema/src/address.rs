//! Reading a Cardano address for the one field this registry is keyed by.
//!
//! A registry keyed by payment credential is asked about addresses, and the
//! conversion is not a substring. The header byte says what the rest of the
//! payload is: types 0–7 put a **payment** credential at bytes 1–29, while types
//! 14–15 (a `stake1…` reward address) put a **stake** credential in the same
//! place. Reading one as the other is how a lookup silently answers about the
//! wrong thing, so it is refused by name instead.
//!
//! Byron addresses are base58 and have no Shelley payment credential; they fail
//! to decode here rather than being guessed at.

use crate::bech32;
use std::fmt;

pub const CREDENTIAL: usize = 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Network {
    Testnet,
    Mainnet,
    /// A network id this build does not know. Cardano defines 0 and 1; a
    /// registry should be able to say it saw something else rather than assume.
    Other(u8),
}

impl Network {
    pub const fn from_id(id: u8) -> Self {
        match id {
            0 => Network::Testnet,
            1 => Network::Mainnet,
            other => Network::Other(other),
        }
    }
}

/// A payment credential and the network its address was read from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Payment {
    pub credential: [u8; CREDENTIAL],
    pub network: Network,
}

impl Payment {
    /// The credential as the 56 lowercase hex this registry is keyed by.
    pub fn hex(self) -> String {
        hex(&self.credential)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddressError {
    /// Not bech32, or the checksum failed. Byron addresses land here.
    NotBech32,
    /// A reward address: 14 for a key stake credential, 15 for a script one.
    StakeCredential(u8),
    /// A header type this build does not know. A new address type needs a
    /// release that knows what it means, not a guess.
    UnknownType(u8),
    /// Fewer bytes than a payment credential needs.
    TooShort,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressError::NotBech32 => f.write_str("not a bech32 Shelley address"),
            AddressError::StakeCredential(kind) => write!(
                f,
                "a reward address (type {kind}) carries a stake credential, not a payment one"
            ),
            AddressError::UnknownType(kind) => {
                write!(f, "an address type this build does not know: {kind}")
            }
            AddressError::TooShort => f.write_str("shorter than a payment credential"),
        }
    }
}

impl std::error::Error for AddressError {}

/// The payment credential an address pays to.
pub fn payment(address: &str) -> Result<Payment, AddressError> {
    let payload = bech32::payload(address).ok_or(AddressError::NotBech32)?;
    let header = payload.first().copied().ok_or(AddressError::TooShort)?;
    let credential = payload
        .get(1..1 + CREDENTIAL)
        .ok_or(AddressError::TooShort)?;
    let network = Network::from_id(header & 0x0f);
    let kind = header >> 4;
    match kind {
        // base (0–3), pointer (4–5), enterprise (6–7): payment credential first.
        0..=7 => Ok(Payment {
            credential: credential.try_into().expect("sliced 28"),
            network,
        }),
        14 | 15 => Err(AddressError::StakeCredential(kind)),
        other => Err(AddressError::UnknownType(other)),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_script_enterprise_address() {
        // The TOOL lock, paired with its credential in the Koios fixture the
        // holder walk runs against.
        let payment =
            payment("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j").expect("reads");
        assert_eq!(
            payment.hex(),
            "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"
        );
        assert_eq!(payment.network, Network::Mainnet);
    }

    #[test]
    fn reads_a_base_address_past_its_stake_part() {
        let payment = payment(
            "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w",
        )
        .expect("reads");
        assert_eq!(
            payment.hex(),
            "381cd9b620ce9d7c73dd45cfe207971cc79df79251003116094f6b36"
        );
    }

    #[test]
    fn a_reward_address_is_refused_not_read_as_a_payment_credential() {
        assert_eq!(
            payment("stake1u8jdj77t835q3gl7pmpjxtl6286awmdvn24ljfddnrf0g6gdvtddv"),
            Err(AddressError::StakeCredential(14))
        );
    }

    #[test]
    fn a_credential_is_not_an_address() {
        assert_eq!(
            payment("c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"),
            Err(AddressError::NotBech32)
        );
    }

    #[test]
    fn a_corrupted_address_is_not_bech32() {
        assert_eq!(
            payment("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k"),
            Err(AddressError::NotBech32)
        );
    }

    #[test]
    fn a_network_id_this_build_does_not_know_is_named() {
        assert_eq!(Network::from_id(0), Network::Testnet);
        assert_eq!(Network::from_id(1), Network::Mainnet);
        assert_eq!(Network::from_id(7), Network::Other(7));
    }
}
