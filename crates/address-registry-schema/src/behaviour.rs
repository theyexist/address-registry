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
//!   (`u32`), its length (`u16`), and a `kind` and `shape` code. Classification
//!   needs nothing else, so it costs one 36-byte fetch.
//! - the blob holds `label`, `project`, `source` and `derived_from`, each a
//!   LEB128 byte length then UTF-8, read only when a display needs them.
//!
//! All integers are little-endian. Codes are explicit and exhaustive, so
//! reordering an enum never changes a published file, and a code this build does
//! not know is refused rather than guessed.
//!
//! Why an index at all: a consumer holding five credentials should not download
//! the table. It asks for `[HEADER, HEADER + 2)`, then one range covering the
//! index, binary-searches locally, and — only if it wants the strings — asks for
//! one record. The whole file is still one linear object for a reader that wants
//! all of it.

use crate::claim::credential;
use crate::contract::{Contract, DerivedFrom, Kind, Shape};
use crate::resolved::Behaviour;
use std::collections::BTreeMap;
use std::fmt;

pub const MAGIC: &[u8; 4] = b"EXAR";
pub const FORMAT: u16 = 1;
pub const HEADER: usize = 12;
pub const INDEX_RECORD: usize = 36;
pub const CREDENTIAL: usize = 28;

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
        let count =
            u32::try_from(self.contracts.len()).map_err(|_| BehaviourError::TooLong("count"))?;
        let blob_at = HEADER as u64 + u64::from(stamp_len) + u64::from(count) * INDEX_RECORD as u64;
        if blob_at > u64::from(u32::MAX) {
            return Err(BehaviourError::TooLong("table"));
        }

        // The index has to know where each record lands, and the blob is where
        // those offsets point, so the blob is built first and placed after.
        let mut index = Vec::with_capacity(self.contracts.len() * INDEX_RECORD);
        let mut blob = Vec::new();
        for (key, contract) in &self.contracts {
            let credential = credential_bytes(key)?;
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
            let offset = u32::try_from(blob_at + blob.len() as u64)
                .map_err(|_| BehaviourError::TooLong("offset"))?;
            let length =
                u16::try_from(record.len()).map_err(|_| BehaviourError::TooLong("record"))?;
            index.extend_from_slice(&credential);
            index.extend_from_slice(&offset.to_le_bytes());
            index.extend_from_slice(&length.to_le_bytes());
            index.push(kind_code(contract.kind));
            index.push(shape_code(contract.shape));
            blob.extend_from_slice(&record);
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

        let mut contracts = BTreeMap::new();
        let mut previous: Option<&[u8]> = None;
        let mut expected_offset = HEADER + stamp_len + count as usize * INDEX_RECORD;
        for _ in 0..count {
            let key = reader.take(CREDENTIAL)?;
            if let Some(previous) = previous {
                if previous >= key {
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
            let kind = kind_from_code(reader.byte()?)
                .ok_or(BehaviourError::UnknownCode("kind", bytes[reader.at - 1]))?;
            let shape = shape_from_code(reader.byte()?)
                .ok_or(BehaviourError::UnknownCode("shape", bytes[reader.at - 1]))?;

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
            if record.at != record.bytes.len() {
                return Err(BehaviourError::NotTiled);
            }

            contracts.insert(
                hex(key),
                Contract {
                    credential: hex(key),
                    kind,
                    shape,
                    label,
                    project,
                    source,
                    derived_from,
                },
            );
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

/// A kind's code in the binary encoding. EXHAUSTIVE on purpose: a new kind does
/// not compile until it has a code, and a code is never reused.
pub const fn kind_code(kind: Kind) -> u8 {
    match kind {
        Kind::Lock => 0,
        Kind::Staking => 1,
    }
}

pub const fn kind_from_code(code: u8) -> Option<Kind> {
    match code {
        0 => Some(Kind::Lock),
        1 => Some(Kind::Staking),
        _ => None,
    }
}

pub const fn shape_code(shape: Shape) -> u8 {
    match shape {
        Shape::OwnerOnly => 0,
        Shape::DeadlineAndOwner => 1,
    }
}

pub const fn shape_from_code(code: u8) -> Option<Shape> {
    match code {
        0 => Some(Shape::OwnerOnly),
        1 => Some(Shape::DeadlineAndOwner),
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
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| BehaviourError::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
    const STAKING: &str = "b0ebdd68bd00f3a633c5097fe0243d69fb27abacd7f6959bec63c4c4";

    fn behaviour() -> Behaviour {
        let mut contracts = BTreeMap::new();
        contracts.insert(
            STAKING.to_string(),
            Contract {
                credential: STAKING.to_string(),
                kind: Kind::Staking,
                shape: Shape::OwnerOnly,
                label: "TSURU Staking".to_string(),
                project: "TSURU".to_string(),
                source: "datum Constr 0 [Bytes(owner_pkh)]".to_string(),
                derived_from: DerivedFrom::Attested("no address registered".to_string()),
            },
        );
        contracts.insert(
            LOCK.to_string(),
            Contract {
                credential: LOCK.to_string(),
                kind: Kind::Lock,
                shape: Shape::DeadlineAndOwner,
                label: "TSURU Lock".to_string(),
                project: "TSURU".to_string(),
                source: "datum Constr 0 [Int(ms), Bytes(owner_pkh)]".to_string(),
                derived_from: DerivedFrom::Address(
                    "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j".to_string(),
                ),
            },
        );
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
    fn one_lookup_costs_the_index_record() {
        let bytes = behaviour().to_bytes().expect("encodes");
        // The header and two index records fit in 12 + 24 + 72 bytes; the
        // strings are the rest and a classification never needs them.
        assert_eq!(HEADER + 2 * INDEX_RECORD, 84);
        assert!(bytes.len() > 84);
    }

    #[test]
    fn a_wrong_magic_is_refused() {
        assert_eq!(
            Behaviour::from_bytes(b"NOPE0000000000"),
            Err(BehaviourError::NotThisFormat)
        );
    }

    #[test]
    fn a_later_format_is_refused_without_reading_further() {
        let mut bytes = behaviour().to_bytes().expect("encodes");
        bytes[4] = 9;
        assert_eq!(
            Behaviour::from_bytes(&bytes),
            Err(BehaviourError::UnknownFormat(9))
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
    fn an_unsorted_index_is_refused() {
        let mut bytes = behaviour().to_bytes().expect("encodes");
        let stamp_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
        let index = HEADER + stamp_len;
        // Swap the two credentials and nothing else: the offsets still tile the
        // blob, so the only thing wrong with the file is that it is not sorted.
        let (first, rest) = bytes[index..].split_at_mut(INDEX_RECORD);
        let second = &mut rest[..INDEX_RECORD];
        first[..CREDENTIAL].swap_with_slice(&mut second[..CREDENTIAL]);
        assert!(matches!(
            Behaviour::from_bytes(&bytes),
            Err(BehaviourError::NotSorted(_))
        ));
    }

    #[test]
    fn an_unknown_kind_code_is_refused() {
        let mut bytes = behaviour().to_bytes().expect("encodes");
        let stamp_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
        let index = HEADER + stamp_len;
        bytes[index + CREDENTIAL + 6] = 0x7f;
        assert_eq!(
            Behaviour::from_bytes(&bytes),
            Err(BehaviourError::UnknownCode("kind", 0x7f))
        );
    }

    #[test]
    fn a_key_that_is_not_a_credential_is_refused_on_encode() {
        let mut table = behaviour();
        table.contracts.insert(
            "not a credential".to_string(),
            table.contracts[LOCK].clone(),
        );
        assert_eq!(
            table.to_bytes(),
            Err(BehaviourError::NotACredential(
                "not a credential".to_string()
            ))
        );
    }
}
