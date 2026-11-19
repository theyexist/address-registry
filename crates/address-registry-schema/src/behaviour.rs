//! The compact form of the authored layer, built for one lookup at a time.
//!
//! JSON is the readable artefact; this is the one a consumer can address. The
//! difference is the index: fixed-width records sorted by credential, so a
//! reader computes an offset instead of parsing a document.
//!
//! ```text
//! "EXAR"  format:u16  count:u32  stamp_len:u16  stamp   count × 36  blob
//! ```
//!
//! - the header is [`HEADER`] bytes, then `generated_at` as `stamp` (UTF-8,
//!   `stamp_len` bytes). A client reads the first [`HEADER`] bytes to learn
//!   `count` and `stamp_len`; the index begins at `HEADER + stamp_len` and is
//!   `count × INDEX_RECORD` bytes.
//! - an index record is the 28-byte credential, the record's absolute offset
//!   (`u32`), its length (`u16`), and a `category` and `shape` code.
//!   Classification needs nothing else, so it costs one 36-byte fetch.
//! - the blob holds `label`, `project`, `source`, `derived_from` and an optional
//!   `datum_example`, each a LEB128 byte length then UTF-8, read only when a
//!   display needs them.
//!
//! **A credential may appear more than once.** One address can do more than one
//! job. Records are sorted by credential with duplicates adjacent, so a lookup is
//! a binary search for the first record and a scan of the run — still
//! `O(log n + k)` and still one range for the index.
//!
//! All integers are little-endian. Codes are explicit and exhaustive, so
//! reordering an enum never changes a published file, and a code this build does
//! not know is refused rather than guessed.

use crate::claim::credential;
use crate::contract::{Category, Contract, DerivedFrom, Shape};
use crate::resolved::Behaviour;
use std::collections::BTreeMap;
use std::fmt;

pub const MAGIC: &[u8; 4] = b"EXAR";
pub const FORMAT: u16 = 2;
pub const HEADER: usize = 12;
pub const INDEX_RECORD: usize = 36;
pub const CREDENTIAL: usize = 28;

/// No shape, for the records where nothing holds under a datum.
pub const NO_SHAPE: u8 = 0xff;

#[derive(Debug, Eq, PartialEq)]
pub enum BehaviourError {
    NotThisFormat,
    UnknownFormat(u16),
    NotACredential(String),
    NotSorted(String),
    UnknownCode(&'static str, u8),
    TooLong(&'static str),
    EndsEarly,
    NotUtf8,
    NotTiled,
}

impl fmt::Display for BehaviourError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BehaviourError::NotThisFormat => f.write_str("not a behaviour table"),
            BehaviourError::UnknownFormat(format) => {
                write!(f, "a behaviour table of format {format}")
            }
            BehaviourError::NotACredential(value) => write!(f, "`{value}` is not a credential"),
            BehaviourError::NotSorted(value) => {
                write!(
                    f,
                    "{value} is out of order, so the index cannot be searched"
                )
            }
            BehaviourError::UnknownCode(what, code) => {
                write!(f, "a {what} code this build does not know: {code}")
            }
            BehaviourError::TooLong(field) => write!(f, "`{field}` is too long for its slot"),
            BehaviourError::EndsEarly => f.write_str("the table ends early"),
            BehaviourError::NotUtf8 => f.write_str("a string that is not UTF-8"),
            BehaviourError::NotTiled => f.write_str("the records do not tile the blob"),
        }
    }
}

impl std::error::Error for BehaviourError {}

impl Behaviour {
    pub fn to_bytes(&self) -> Result<Vec<u8>, BehaviourError> {
        let stamp = self.generated_at.as_bytes();
        let stamp_len =
            u16::try_from(stamp.len()).map_err(|_| BehaviourError::TooLong("generated_at"))?;
        let count: usize = self.contracts.values().map(Vec::len).sum();
        let count = u32::try_from(count).map_err(|_| BehaviourError::TooLong("count"))?;
        let blob_at = HEADER as u64 + u64::from(stamp_len) + u64::from(count) * INDEX_RECORD as u64;
        if blob_at > u64::from(u32::MAX) {
            return Err(BehaviourError::TooLong("table"));
        }

        // The index has to know where each record lands, and the blob is where
        // those offsets point, so the blob is built first and placed after.
        let mut index = Vec::with_capacity(count as usize * INDEX_RECORD);
        let mut blob = Vec::new();
        for (key, uses) in &self.contracts {
            let credential = credential_bytes(key)?;
            for contract in uses {
                let mut record = Vec::new();
                put_str(&mut record, &contract.label);
                put_str(&mut record, &contract.project);
                put_str(&mut record, &contract.source);
                match &contract.derived_from {
                    DerivedFrom::Attested(reason) => {
                        record.push(0);
                        put_str(&mut record, reason);
                    }
                    DerivedFrom::Address(address) => {
                        record.push(1);
                        put_str(&mut record, address);
                    }
                }
                // Length zero means no example; validation refuses an empty one,
                // so the two cannot be confused.
                match &contract.datum_example {
                    Some(example) => {
                        put_len(&mut record, example.len());
                        record.extend_from_slice(example.as_bytes());
                    }
                    None => put_len(&mut record, 0),
                }

                let offset = u32::try_from(blob_at + blob.len() as u64)
                    .map_err(|_| BehaviourError::TooLong("offset"))?;
                let length =
                    u16::try_from(record.len()).map_err(|_| BehaviourError::TooLong("record"))?;
                index.extend_from_slice(&credential);
                index.extend_from_slice(&offset.to_le_bytes());
                index.extend_from_slice(&length.to_le_bytes());
                index.push(category_code(contract.category));
                index.push(shape_code(contract.shape));
                blob.extend_from_slice(&record);
            }
        }

        let mut out = Vec::with_capacity(blob_at as usize + blob.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&stamp_len.to_le_bytes());
        out.extend_from_slice(stamp);
        out.extend_from_slice(&index);
        out.extend_from_slice(&blob);
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BehaviourError> {
        let mut reader = Reader { bytes, at: 0 };
        if reader.take(4)? != MAGIC {
            return Err(BehaviourError::NotThisFormat);
        }
        let format = u16::from_le_bytes([reader.byte()?, reader.byte()?]);
        if format != FORMAT {
            // The layout after the header is the format's own: read no further.
            return Err(BehaviourError::UnknownFormat(format));
        }
        let count = u32::from_le_bytes([
            reader.byte()?,
            reader.byte()?,
            reader.byte()?,
            reader.byte()?,
        ]);
        let stamp_len = usize::from(u16::from_le_bytes([reader.byte()?, reader.byte()?]));
        let generated_at = String::from_utf8(reader.take(stamp_len)?.to_vec())
            .map_err(|_| BehaviourError::NotUtf8)?;

        let mut contracts: BTreeMap<String, Vec<Contract>> = BTreeMap::new();
        let mut previous: Option<&[u8]> = None;
        let mut expected_offset = HEADER + stamp_len + count as usize * INDEX_RECORD;
        for _ in 0..count {
            let key = reader.take(CREDENTIAL)?;
            // Duplicates are legal — one address, several uses — so equal keys
            // are allowed and only a *descending* one is corruption.
            if let Some(previous) = previous {
                if previous > key {
                    return Err(BehaviourError::NotSorted(hex(key)));
                }
            }
            previous = Some(key);
            let offset = u32::from_le_bytes([
                reader.byte()?,
                reader.byte()?,
                reader.byte()?,
                reader.byte()?,
            ]) as usize;
            let length = usize::from(u16::from_le_bytes([reader.byte()?, reader.byte()?]));
            let category_byte = reader.byte()?;
            let category = category_from_code(category_byte)
                .ok_or(BehaviourError::UnknownCode("category", category_byte))?;
            let shape_byte = reader.byte()?;
            let shape = shape_from_code(shape_byte)
                .ok_or(BehaviourError::UnknownCode("shape", shape_byte))?;

            // Records tile the blob: a gap means the index disagrees with the
            // bytes, and every later offset is suspect.
            if offset != expected_offset {
                return Err(BehaviourError::NotTiled);
            }
            expected_offset += length;

            let mut record = Reader {
                bytes: bytes
                    .get(offset..offset + length)
                    .ok_or(BehaviourError::EndsEarly)?,
                at: 0,
            };
            let label = record.string()?;
            let project = record.string()?;
            let source = record.string()?;
            let derived_from = match record.byte()? {
                0 => DerivedFrom::Attested(record.string()?),
                1 => DerivedFrom::Address(record.string()?),
                other => return Err(BehaviourError::UnknownCode("derived_from", other)),
            };
            let example_len = record.len()?;
            let datum_example = if example_len == 0 {
                None
            } else {
                Some(record.string_of(example_len)?)
            };
            if record.at != record.bytes.len() {
                return Err(BehaviourError::NotTiled);
            }

            contracts.entry(hex(key)).or_default().push(Contract {
                category,
                shape,
                datum_example,
                label,
                project,
                source,
                derived_from,
            });
        }
        if expected_offset != bytes.len() {
            return Err(BehaviourError::NotTiled);
        }
        Ok(Self {
            format: u32::from(FORMAT),
            generated_at,
            contracts,
        })
    }
}

/// A category's code in the binary encoding. EXHAUSTIVE on purpose: a new
/// category does not compile until it has a code, and a code is never reused.
pub const fn category_code(category: Category) -> u8 {
    match category {
        Category::Exchange => 0,
        Category::Marketplace => 1,
        Category::Launchpad => 2,
        Category::Burn => 3,
        Category::Airdropper => 4,
        Category::Minter => 5,
        Category::Defi => 6,
        Category::Staking => 7,
        Category::Vesting => 8,
        Category::Lock => 9,
        Category::Wallet => 10,
    }
}

pub const fn category_from_code(code: u8) -> Option<Category> {
    match code {
        0 => Some(Category::Exchange),
        1 => Some(Category::Marketplace),
        2 => Some(Category::Launchpad),
        3 => Some(Category::Burn),
        4 => Some(Category::Airdropper),
        5 => Some(Category::Minter),
        6 => Some(Category::Defi),
        7 => Some(Category::Staking),
        8 => Some(Category::Vesting),
        9 => Some(Category::Lock),
        10 => Some(Category::Wallet),
        _ => None,
    }
}

pub const fn shape_code(shape: Option<Shape>) -> u8 {
    match shape {
        None => NO_SHAPE,
        Some(Shape::OwnerOnly) => 0,
        Some(Shape::DeadlineAndOwner) => 1,
        Some(Shape::DeadlineAndOwners) => 2,
    }
}

pub const fn shape_from_code(code: u8) -> Option<Option<Shape>> {
    match code {
        NO_SHAPE => Some(None),
        0 => Some(Some(Shape::OwnerOnly)),
        1 => Some(Some(Shape::DeadlineAndOwner)),
        2 => Some(Some(Shape::DeadlineAndOwners)),
        _ => None,
    }
}

fn credential_bytes(value: &str) -> Result<[u8; CREDENTIAL], BehaviourError> {
    if credential(value).as_deref() != Some(value) {
        return Err(BehaviourError::NotACredential(value.to_string()));
    }
    let mut out = [0u8; CREDENTIAL];
    let digits = value.as_bytes();
    for (index, byte) in out.iter_mut().enumerate() {
        let high =
            nibble(digits[index * 2]).ok_or(BehaviourError::NotACredential(value.to_string()))?;
        let low = nibble(digits[index * 2 + 1])
            .ok_or(BehaviourError::NotACredential(value.to_string()))?;
        *byte = (high << 4) | low;
    }
    Ok(out)
}

fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    put_len(out, value.len());
    out.extend_from_slice(value.as_bytes());
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

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BehaviourError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(BehaviourError::EndsEarly)?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, BehaviourError> {
        Ok(self.take(1)?[0])
    }

    fn len(&mut self) -> Result<usize, BehaviourError> {
        let mut n: usize = 0;
        for shift in (0..28).step_by(7) {
            let byte = self.byte()?;
            n |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(BehaviourError::TooLong("length"))
    }

    fn string(&mut self) -> Result<String, BehaviourError> {
        let len = self.len()?;
        self.string_of(len)
    }

    fn string_of(&mut self, len: usize) -> Result<String, BehaviourError> {
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| BehaviourError::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
    const STAKING: &str = "b0ebdd68bd00f3a633c5097fe0243d69fb27abacd7f6959bec63c4c4";

    fn lock_use() -> Contract {
        Contract {
            category: Category::Lock,
            shape: Some(Shape::DeadlineAndOwner),
            datum_example: None,
            label: "TSURU Lock".to_string(),
            project: "TSURU".to_string(),
            source: "datum Constr 0 [Int(ms), Bytes(owner_pkh)]".to_string(),
            derived_from: DerivedFrom::Address(
                "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j".to_string(),
            ),
        }
    }

    fn staking_use() -> Contract {
        Contract {
            category: Category::Staking,
            shape: Some(Shape::OwnerOnly),
            datum_example: Some("d87981581c".to_string()),
            label: "TSURU Staking".to_string(),
            project: "TSURU".to_string(),
            source: "datum Constr 0 [Bytes(owner_pkh)]".to_string(),
            derived_from: DerivedFrom::Attested("no address registered".to_string()),
        }
    }

    fn behaviour() -> Behaviour {
        let mut contracts = BTreeMap::new();
        contracts.insert(STAKING.to_string(), vec![staking_use()]);
        contracts.insert(LOCK.to_string(), vec![lock_use()]);
        Behaviour {
            format: u32::from(FORMAT),
            generated_at: "2026-11-19T01:42:17.808Z".to_string(),
            contracts,
        }
    }

    #[test]
    fn a_table_survives_bytes_and_back() {
        let original = behaviour();
        let bytes = original.to_bytes().expect("encodes");
        let decoded = Behaviour::from_bytes(&bytes).expect("decodes");
        assert_eq!(decoded.generated_at, original.generated_at);
        assert_eq!(decoded.contracts, original.contracts);
    }

    #[test]
    fn one_credential_may_hold_two_uses() {
        let mut original = behaviour();
        let mut wallet = lock_use();
        wallet.category = Category::Wallet;
        wallet.shape = None;
        wallet.label = "TSURU operator".to_string();
        original
            .contracts
            .insert(LOCK.to_string(), vec![lock_use(), wallet]);

        let bytes = original.to_bytes().expect("encodes");
        let decoded = Behaviour::from_bytes(&bytes).expect("decodes");
        let uses = decoded.get(LOCK);
        assert_eq!(uses.len(), 2);
        assert_eq!(
            uses.first().map(|use_of| use_of.category),
            Some(Category::Lock)
        );
        assert_eq!(
            uses.get(1).map(|use_of| use_of.category),
            Some(Category::Wallet)
        );
        assert_eq!(uses.get(1).and_then(|use_of| use_of.shape), None);
    }

    #[test]
    fn the_index_is_fixed_width_and_sorted() {
        let bytes = behaviour().to_bytes().expect("encodes");
        assert_eq!(&bytes[..4], MAGIC);
        let count = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
        assert_eq!(count, 2);
        let stamp_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
        let index = HEADER + stamp_len;
        let first = &bytes[index..index + CREDENTIAL];
        let second = &bytes[index + INDEX_RECORD..index + INDEX_RECORD + CREDENTIAL];
        assert!(first < second, "the index is not sorted");
    }

    #[test]
    fn a_wrong_magic_is_refused() {
        assert_eq!(
            Behaviour::from_bytes(b"NOPE0000000000"),
            Err(BehaviourError::NotThisFormat)
        );
    }

    #[test]
    fn an_older_format_is_refused_without_reading_further() {
        let mut bytes = behaviour().to_bytes().expect("encodes");
        bytes[4] = 1;
        assert_eq!(
            Behaviour::from_bytes(&bytes),
            Err(BehaviourError::UnknownFormat(1))
        );
    }

    #[test]
    fn a_truncated_table_is_refused() {
        let bytes = behaviour().to_bytes().expect("encodes");
        assert_eq!(
            Behaviour::from_bytes(&bytes[..bytes.len() - 4]),
            Err(BehaviourError::EndsEarly)
        );
    }

    #[test]
    fn a_descending_index_is_refused() {
        let mut bytes = behaviour().to_bytes().expect("encodes");
        let stamp_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
        let index = HEADER + stamp_len;
        // Swap the two credentials and nothing else: the offsets still tile the
        // blob, so the only thing wrong with the file is that it descends.
        let (first, rest) = bytes[index..].split_at_mut(INDEX_RECORD);
        let second = &mut rest[..INDEX_RECORD];
        first[..CREDENTIAL].swap_with_slice(&mut second[..CREDENTIAL]);
        assert!(matches!(
            Behaviour::from_bytes(&bytes),
            Err(BehaviourError::NotSorted(_))
        ));
    }

    #[test]
    fn an_unknown_category_code_is_refused() {
        let mut bytes = behaviour().to_bytes().expect("encodes");
        let stamp_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
        let index = HEADER + stamp_len;
        bytes[index + CREDENTIAL + 6] = 0x7f;
        assert_eq!(
            Behaviour::from_bytes(&bytes),
            Err(BehaviourError::UnknownCode("category", 0x7f))
        );
    }

    #[test]
    fn a_key_that_is_not_a_credential_is_refused_on_encode() {
        let mut table = behaviour();
        table
            .contracts
            .insert("not a credential".to_string(), vec![lock_use()]);
        assert_eq!(
            table.to_bytes(),
            Err(BehaviourError::NotACredential(
                "not a credential".to_string()
            ))
        );
    }

    #[test]
    fn no_shape_round_trips_as_no_shape() {
        let mut original = behaviour();
        let mut wallet = lock_use();
        wallet.category = Category::Wallet;
        wallet.shape = None;
        original.contracts.insert(LOCK.to_string(), vec![wallet]);
        let decoded =
            Behaviour::from_bytes(&original.to_bytes().expect("encodes")).expect("decodes");
        assert_eq!(
            decoded.get(LOCK).first().and_then(|use_of| use_of.shape),
            None
        );
    }

    #[test]
    fn every_category_has_its_own_code() {
        let all = [
            Category::Exchange,
            Category::Marketplace,
            Category::Launchpad,
            Category::Burn,
            Category::Airdropper,
            Category::Minter,
            Category::Defi,
            Category::Staking,
            Category::Vesting,
            Category::Lock,
            Category::Wallet,
        ];
        for (index, category) in all.iter().enumerate() {
            assert_eq!(category_code(*category), index as u8);
            assert_eq!(category_from_code(index as u8), Some(*category));
        }
        assert_eq!(category_from_code(11), None);
    }
}
