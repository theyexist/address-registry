//! The stake-service table: who runs an ordinary wallet.
//!
//! A payment credential names a CONTRACT. This table names the other thing a
//! consumer keeps finding — a service run from a wallet, identified by its stake
//! credential because a wallet has as many payment addresses as it likes and one
//! stable stake key.
//!
//! It is kept separate from [`crate::Contract`] on purpose. A behaviour is what a
//! script IS, and code decodes datums by it. A stake service is a claim about
//! PEOPLE — an exchange's hot wallet, a minting provider, a staking programme —
//! which changes without any code changing, and a wrong one should be fixable
//! without a release. The two are authored in separate directories and resolved
//! into separate artefacts for exactly that reason.
//!
//! [`StakeLabels`] is the lookup: each service's 29 stake bytes, its kind and its
//! label, in a fixed-width record per entry. It is the same `STKL` layout
//! `shared-crates/address-registry` publishes, so a consumer can read either the
//! published file or this repository's copy with one decoder.

use crate::bech32;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The binary lookup's first four bytes.
pub const MAGIC: &[u8; 4] = b"STKL";

/// Both files' format. Bumped when either layout changes; a reader refuses a
/// format it does not know rather than guessing.
pub const FORMAT: u16 = 1;

/// A stake address's bytes: a header (`0xe1` for a key credential, `0xf1` for a
/// script) and the 28-byte credential.
pub type StakeBytes = [u8; 29];

/// What a service does with what it holds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StakeServiceKind {
    /// Runs mints on a project's behalf and takes a per-mint fee. Its wallet
    /// appears as a destination in EVERY mint transaction it serves, for every
    /// unrelated project — so a consumer that walks a frontier must record it
    /// and refuse to expand it.
    MintingProvider,
    /// An NFT marketplace's escrow. Assets and offers sit at its script
    /// addresses while listed, so it appears as a HOLDER of everything on sale.
    Marketplace,
    /// A token venue — DEX, aggregator, swap desk. Liquidity sits at its
    /// addresses, so it tops any holder list.
    Exchange,
    /// A staking programme run from a wallet, for tokens or NFTs: holders send
    /// it their assets for rewards, so it holds a pooled share of a supply.
    Staking,
}

/// One service run from a wallet, `stakes/<stake address>.toml`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StakeService {
    pub stake: String,
    pub label: String,
    pub kind: StakeServiceKind,
    /// How the attribution was established. Required: an entry is evidence, and
    /// one with no source is a guess with a filename.
    pub source: String,
}

/// A stake credential deliberately NOT a service's, `stake-exclusions/<stake>.toml`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StakeExclusion {
    pub stake: String,
    /// What reaches it, and what it turned out to be.
    pub reason: String,
}

/// `resolved/stake-services.json`: the whole record, for a reader or an audit.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StakeServices {
    pub format: u16,
    pub generated_at: String,
    pub services: Vec<StakeService>,
    pub exclusions: Vec<StakeExclusion>,
}

/// Each service's label and kind by its stake key's bytes: the lookup.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StakeLabels {
    /// Sorted by `stake`, one entry each.
    pub entries: Vec<StakeLabel>,
}

/// One service, as the lookup needs it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StakeLabel {
    /// The 29 raw bytes; see [`StakeBytes`]. Serialised as the bech32 form.
    #[serde(with = "stake_hex")]
    pub stake: StakeBytes,
    pub kind: StakeServiceKind,
    pub label: String,
}

/// Why a stake file is not one, or a table is not a table.
#[derive(Debug, Eq, PartialEq)]
pub enum StakeTableError {
    /// Not a `stake1…` mainnet reward address.
    NotAStake(String),
    /// The file name is not the stake address it declares.
    Filename(String),
    /// A declared value that carries no information.
    Empty(&'static str),
    NotThisFormat,
    UnknownFormat(u16),
    UnknownKind(u8),
    NotSorted(String),
    EndsEarly,
    NotUtf8,
    /// The table decoded, and failed its own checks.
    Check(Vec<String>),
}

impl fmt::Display for StakeTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StakeTableError::NotAStake(value) => {
                write!(f, "`{value}` is not a mainnet `stake1…` address")
            }
            StakeTableError::Filename(value) => {
                write!(
                    f,
                    "the file must be `{value}.toml`, named after its stake address"
                )
            }
            StakeTableError::Empty(field) => write!(f, "`{field}` is empty"),
            StakeTableError::NotThisFormat => f.write_str("not a stake-label table"),
            StakeTableError::UnknownFormat(format) => {
                write!(f, "a stake-label table of format {format}")
            }
            StakeTableError::UnknownKind(code) => {
                write!(f, "a service kind this build does not know: {code}")
            }
            StakeTableError::NotSorted(value) => {
                write!(
                    f,
                    "{value} is out of order, so the table cannot be searched"
                )
            }
            StakeTableError::EndsEarly => f.write_str("the table ends early"),
            StakeTableError::NotUtf8 => f.write_str("a string that is not UTF-8"),
            StakeTableError::Check(problems) => write!(f, "{}", problems.join("; ")),
        }
    }
}

impl std::error::Error for StakeTableError {}

/// A mainnet stake address's bytes, or `None` for anything else: the wrong
/// prefix or length, a bad checksum, or a header that is not a reward address's.
pub fn stake_bytes(stake: &str) -> Option<StakeBytes> {
    let lowered = stake.to_ascii_lowercase();
    if !lowered.starts_with("stake1") {
        return None;
    }
    let payload = bech32::payload(&lowered)?;
    if payload.len() != 29 || !matches!(payload[0], 0xe1 | 0xf1) {
        return None;
    }
    let mut bytes = StakeBytes::default();
    bytes.copy_from_slice(&payload);
    Some(bytes)
}

impl StakeService {
    /// Everything the file must be, given its name.
    pub fn validate(&self, file_stem: &str) -> Result<(), StakeTableError> {
        if stake_bytes(&self.stake).is_none() {
            return Err(StakeTableError::NotAStake(self.stake.clone()));
        }
        if file_stem != self.stake {
            return Err(StakeTableError::Filename(self.stake.clone()));
        }
        for (field, value) in [("label", &self.label), ("source", &self.source)] {
            if value.trim().is_empty() {
                return Err(StakeTableError::Empty(field));
            }
        }
        Ok(())
    }
}

impl StakeExclusion {
    pub fn validate(&self, file_stem: &str) -> Result<(), StakeTableError> {
        if stake_bytes(&self.stake).is_none() {
            return Err(StakeTableError::NotAStake(self.stake.clone()));
        }
        if file_stem != self.stake {
            return Err(StakeTableError::Filename(self.stake.clone()));
        }
        if self.reason.trim().is_empty() {
            return Err(StakeTableError::Empty("reason"));
        }
        Ok(())
    }
}

impl StakeLabels {
    /// The lookup for a set of services, checked and sorted by stake bytes.
    pub fn from_services(services: &[StakeService]) -> Result<Self, StakeTableError> {
        let mut entries = Vec::with_capacity(services.len());
        for service in services {
            let stake = stake_bytes(&service.stake)
                .ok_or_else(|| StakeTableError::NotAStake(service.stake.clone()))?;
            entries.push(StakeLabel {
                stake,
                kind: service.kind,
                label: service.label.clone(),
            });
        }
        entries.sort_by_key(|entry| entry.stake);
        let labels = Self { entries };
        let problems = labels.check();
        if problems.is_empty() {
            Ok(labels)
        } else {
            Err(StakeTableError::Check(problems))
        }
    }

    /// The service a stake key belongs to, if any.
    pub fn get(&self, stake: &str) -> Option<&StakeLabel> {
        self.get_bytes(&stake_bytes(stake)?)
    }

    /// The lookup without the bech32 decode, for a consumer already holding the
    /// 29 bytes.
    pub fn get_bytes(&self, stake: &StakeBytes) -> Option<&StakeLabel> {
        self.entries
            .binary_search_by(|entry| entry.stake.cmp(stake))
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Everything a decoded table must be. A problem here is a file that would
    /// answer wrongly rather than loudly, so the reader refuses it.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for entry in &self.entries {
            if !matches!(entry.stake[0], 0xe1 | 0xf1) {
                problems.push(format!(
                    "{:02x}… is not a mainnet reward address header",
                    entry.stake[0]
                ));
            }
            if entry.label.trim().is_empty() {
                problems.push(format!("{} has no label", hex(&entry.stake)));
            }
        }
        for pair in self.entries.windows(2) {
            if pair[0].stake >= pair[1].stake {
                problems.push(format!(
                    "not sorted with one entry each at {}",
                    hex(&pair[1].stake)
                ));
            }
        }
        problems
    }

    /// The binary encoding. `format` is little-endian; `n` is an unsigned
    /// LEB128 varint; `label` is its byte length as `n`, then UTF-8.
    ///
    /// ```text
    /// "STKL"  format:u16
    /// n  entries: stake:29 bytes  kind:u8  label
    /// ```
    ///
    /// `kind` is [`kind_code`]: explicit codes, so reordering the enum never
    /// changes a published file. Nothing may follow the last entry.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        put_len(&mut out, self.entries.len());
        for entry in &self.entries {
            out.extend_from_slice(&entry.stake);
            out.push(kind_code(entry.kind));
            put_len(&mut out, entry.label.len());
            out.extend_from_slice(entry.label.as_bytes());
        }
        out
    }

    /// A lookup from its binary, checked.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StakeTableError> {
        let mut reader = Reader { bytes, at: 0 };
        if reader.take(4)? != MAGIC {
            return Err(StakeTableError::NotThisFormat);
        }
        let format = u16::from_le_bytes([reader.byte()?, reader.byte()?]);
        if format != FORMAT {
            // The layout after the header is the format's own: read no further.
            return Err(StakeTableError::UnknownFormat(format));
        }
        let mut entries = Vec::new();
        for _ in 0..reader.len()? {
            let mut stake = StakeBytes::default();
            stake.copy_from_slice(reader.take(29)?);
            let kind = kind_from_code(reader.byte()?)
                .ok_or(StakeTableError::UnknownKind(reader.bytes[reader.at - 1]))?;
            let length = reader.len()?;
            let label = String::from_utf8(reader.take(length)?.to_vec())
                .map_err(|_| StakeTableError::NotUtf8)?;
            entries.push(StakeLabel { stake, kind, label });
        }
        if reader.at != bytes.len() {
            return Err(StakeTableError::Check(vec![
                "bytes after the last entry".to_string()
            ]));
        }
        let labels = Self { entries };
        let problems = labels.check();
        if problems.is_empty() {
            Ok(labels)
        } else {
            Err(StakeTableError::Check(problems))
        }
    }
}

impl StakeLabel {
    /// The bech32 stake address, `stake1…`.
    pub fn stake_address(&self) -> String {
        // The payload is the header and the credential; re-encoding needs the
        // bech32 machinery in `crate::bech32`, which is decode-only by design.
        // A label that came from a file has its address there, so this is only
        // for a reader that decoded the binary — it re-encodes here rather than
        // growing the schema's surface.
        encode_stake(&self.stake)
    }
}

/// A kind's code in the binary encoding. EXHAUSTIVE on purpose: a new kind does
/// not compile until it has a code, and a code is never reused.
pub const fn kind_code(kind: StakeServiceKind) -> u8 {
    match kind {
        StakeServiceKind::MintingProvider => 0,
        StakeServiceKind::Marketplace => 1,
        StakeServiceKind::Exchange => 2,
        StakeServiceKind::Staking => 3,
    }
}

/// The kind a code names, or `None` for one this build does not know. A reader
/// then refuses the whole file and keeps its last good one.
pub const fn kind_from_code(code: u8) -> Option<StakeServiceKind> {
    match code {
        0 => Some(StakeServiceKind::MintingProvider),
        1 => Some(StakeServiceKind::Marketplace),
        2 => Some(StakeServiceKind::Exchange),
        3 => Some(StakeServiceKind::Staking),
        _ => None,
    }
}

fn put_len(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let low = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], StakeTableError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(StakeTableError::EndsEarly)?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, StakeTableError> {
        Ok(self.take(1)?[0])
    }

    fn len(&mut self) -> Result<usize, StakeTableError> {
        let mut n: usize = 0;
        // Four groups is 28 bits: far past any table, and short of overflow.
        for shift in (0..28).step_by(7) {
            let b = self.byte()?;
            n |= ((b & 0x7f) as usize) << shift;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(StakeTableError::Check(
            vec!["a length too long".to_string()],
        ))
    }
}

/// `StakeLabel::stake` as hex in JSON: the bech32 form is the human one, but it
/// is derived from these bytes and a reader should see the bytes it looks up.
mod stake_hex {
    use super::StakeBytes;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(stake: &StakeBytes, serializer: S) -> Result<S::Ok, S::Error> {
        let hex: String = stake.iter().map(|b| format!("{b:02x}")).collect();
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<StakeBytes, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = hex_decode(&text).ok_or_else(|| serde::de::Error::custom("not hex"))?;
        <[u8; 29]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("a stake key is 29 bytes"))
    }

    fn hex_decode(text: &str) -> Option<Vec<u8>> {
        if !text.len().is_multiple_of(2) {
            return None;
        }
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
            .collect()
    }
}

/// The bech32 form of a mainnet reward address.
///
/// The schema crate decodes bech32 and generally does not encode it; a stake
/// label is the one place a consumer needs the address back from the bytes, and
/// keeping it here is cheaper than a dependency on an address library.
fn encode_stake(stake: &StakeBytes) -> String {
    const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    const GENERATOR: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let mut five = Vec::new();
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    for byte in stake {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            five.push(((accumulator >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        five.push(((accumulator << (5 - bits)) & 31) as u8);
    }
    let hrp = b"stake";
    let mut values = Vec::with_capacity(hrp.len() * 2 + 1 + five.len() + 6);
    values.extend(hrp.iter().map(|b| b >> 5));
    values.push(0);
    values.extend(hrp.iter().map(|b| b & 31));
    values.extend_from_slice(&five);
    // Six zero groups, so the checksum is computed over the payload only.
    values.extend_from_slice(&[0; 6]);
    let mut checksum: u32 = 1;
    for value in &values {
        let top = checksum >> 25;
        checksum = ((checksum & 0x1ff_ffff) << 5) ^ u32::from(*value);
        for (index, generator) in GENERATOR.iter().enumerate() {
            if (top >> index) & 1 == 1 {
                checksum ^= generator;
            }
        }
    }
    checksum ^= 1;
    let mut out = String::from("stake1");
    for value in five {
        out.push(CHARSET[value as usize] as char);
    }
    for index in 0..6 {
        let value = ((checksum >> (5 * (5 - index))) & 31) as usize;
        out.push(CHARSET[value] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANVIL: &str = "stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr";
    const JPG: &str = "stake17ykfvl6t62y5fvryvtsnch3lt406dcpls4n4d9pcekpnumg6v83tq";

    fn service(stake: &str, label: &str, kind: StakeServiceKind) -> StakeService {
        StakeService {
            stake: stake.to_string(),
            label: label.to_string(),
            kind,
            source: "test".to_string(),
        }
    }

    #[test]
    fn a_stake_address_decodes_to_its_header_and_credential() {
        let bytes = stake_bytes(ANVIL).expect("a mainnet stake address");
        assert_eq!(bytes.len(), 29);
        assert_eq!(bytes[0], 0xe1, "a key stake credential");
        assert_eq!(bytes[1..].len(), 28);
    }

    #[test]
    fn a_payment_address_is_not_a_stake_address() {
        assert_eq!(
            stake_bytes("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j"),
            None
        );
        assert_eq!(
            stake_bytes("c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"),
            None
        );
    }

    #[test]
    fn a_stake_address_round_trips_through_its_bech32_form() {
        let bytes = stake_bytes(ANVIL).expect("decodes");
        let label = StakeLabel {
            stake: bytes,
            kind: StakeServiceKind::MintingProvider,
            label: "Anvil".to_string(),
        };
        assert_eq!(label.stake_address(), ANVIL);
    }

    #[test]
    fn a_table_survives_bytes_and_back() {
        let labels = StakeLabels::from_services(&[
            service(ANVIL, "Anvil", StakeServiceKind::MintingProvider),
            service(JPG, "JPG.store", StakeServiceKind::Marketplace),
        ])
        .expect("a valid set");
        let decoded = StakeLabels::from_bytes(&labels.to_bytes()).expect("decodes");
        assert_eq!(decoded, labels);
        assert_eq!(
            decoded.get(ANVIL).map(|entry| entry.label.as_str()),
            Some("Anvil")
        );
        assert_eq!(
            decoded.get(JPG).map(|entry| entry.kind),
            Some(StakeServiceKind::Marketplace)
        );
        assert_eq!(
            decoded.get("stake1u8jdj77t835q3gl7pmpjxtl6286awmdvn24ljfddnrf0g6gdvtddv"),
            None
        );
    }

    #[test]
    fn a_table_with_a_repeated_stake_is_refused() {
        let error = StakeLabels::from_services(&[
            service(ANVIL, "Anvil", StakeServiceKind::MintingProvider),
            service(ANVIL, "Anvil again", StakeServiceKind::Exchange),
        ])
        .expect_err("one entry each");
        assert!(matches!(error, StakeTableError::Check(_)));
    }

    #[test]
    fn a_wrong_magic_is_refused() {
        let mut bytes = StakeLabels::from_services(&[service(
            ANVIL,
            "Anvil",
            StakeServiceKind::MintingProvider,
        )])
        .expect("valid")
        .to_bytes();
        bytes[0] = b'X';
        assert_eq!(
            StakeLabels::from_bytes(&bytes),
            Err(StakeTableError::NotThisFormat)
        );
    }

    #[test]
    fn an_unknown_kind_code_is_refused() {
        let mut bytes = StakeLabels::from_services(&[service(
            ANVIL,
            "Anvil",
            StakeServiceKind::MintingProvider,
        )])
        .expect("valid")
        .to_bytes();
        let at = bytes.len() - "Anvil".len() - 2;
        bytes[at] = 9;
        assert_eq!(
            StakeLabels::from_bytes(&bytes),
            Err(StakeTableError::UnknownKind(9))
        );
    }

    #[test]
    fn a_service_file_is_named_after_its_stake() {
        let service = service(ANVIL, "Anvil", StakeServiceKind::MintingProvider);
        assert_eq!(service.validate(ANVIL), Ok(()));
        assert_eq!(
            service.validate("someone-else"),
            Err(StakeTableError::Filename(ANVIL.to_string()))
        );
    }

    #[test]
    fn an_empty_source_is_refused() {
        let mut service = service(ANVIL, "Anvil", StakeServiceKind::MintingProvider);
        service.source = " ".to_string();
        assert_eq!(
            service.validate(ANVIL),
            Err(StakeTableError::Empty("source"))
        );
    }

    #[test]
    fn an_exclusion_needs_a_reason() {
        let exclusion = StakeExclusion {
            stake: ANVIL.to_string(),
            reason: String::new(),
        };
        assert_eq!(
            exclusion.validate(ANVIL),
            Err(StakeTableError::Empty("reason"))
        );
    }
}
