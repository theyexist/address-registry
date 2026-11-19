//! Decoding a bech32 string to its payload bytes.
//!
//! Cardano addresses are bech32 (BIP-173), never bech32m. This module stops at
//! the bytes: what they mean depends on the header, which is
//! [`crate::address`]'s job. Splitting it there is what stops a reward address
//! being read as though its stake credential were a payment one.

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const GENERATOR: [u32; 5] = [
    0x3b6a_57b2,
    0x2650_8e6d,
    0x1ea1_19fa,
    0x3d42_33dd,
    0x2a14_62b3,
];

/// The payload an address encodes, if it decodes at all.
pub fn payload(address: &str) -> Option<Vec<u8>> {
    let has_upper = address.bytes().any(|byte| byte.is_ascii_uppercase());
    let has_lower = address.bytes().any(|byte| byte.is_ascii_lowercase());
    if has_upper && has_lower {
        return None;
    }
    let lowered = address.to_ascii_lowercase();
    let (hrp, data) = lowered.rsplit_once('1')?;
    if hrp.is_empty() || data.len() < 7 {
        return None;
    }
    let values = to_five_bits(data)?;
    let mut checked = hrp_expand(hrp);
    checked.extend_from_slice(&values);
    if polymod(&checked) != 1 {
        return None;
    }
    from_five_bits(&values[..values.len() - 6])
}

fn polymod(values: &[u8]) -> u32 {
    let mut checksum: u32 = 1;
    for value in values {
        let top = checksum >> 25;
        checksum = ((checksum & 0x1ff_ffff) << 5) ^ u32::from(*value);
        for (index, generator) in GENERATOR.iter().enumerate() {
            if (top >> index) & 1 == 1 {
                checksum ^= generator;
            }
        }
    }
    checksum
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut expanded = Vec::with_capacity(hrp.len() * 2 + 1);
    expanded.extend(hrp.bytes().map(|byte| byte >> 5));
    expanded.push(0);
    expanded.extend(hrp.bytes().map(|byte| byte & 31));
    expanded
}

fn to_five_bits(data: &str) -> Option<Vec<u8>> {
    data.bytes()
        .map(|byte| {
            CHARSET
                .iter()
                .position(|candidate| *candidate == byte)
                .map(|index| index as u8)
        })
        .collect()
}

fn from_five_bits(values: &[u8]) -> Option<Vec<u8>> {
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut bytes = Vec::new();
    for value in values {
        accumulator = (accumulator << 5) | u32::from(*value);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            bytes.push(((accumulator >> bits) & 0xff) as u8);
        }
    }
    // Leftover bits must be fewer than a byte and zero, or the encoding padded.
    if bits >= 5 || accumulator & ((1 << bits) - 1) != 0 {
        return None;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_enterprise_address_decodes_to_29_bytes() {
        let payload =
            payload("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j").expect("decodes");
        assert_eq!(payload.len(), 29);
        // Type 7 (script enterprise), network 1 (mainnet).
        assert_eq!(payload[0], 0x71);
    }

    #[test]
    fn a_base_address_decodes_to_57_bytes() {
        let payload = payload(
            "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w",
        )
        .expect("decodes");
        assert_eq!(payload.len(), 57);
    }

    #[test]
    fn a_corrupted_checksum_is_rejected() {
        assert_eq!(
            payload("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k"),
            None
        );
    }

    #[test]
    fn a_credential_is_not_an_address() {
        assert_eq!(
            payload("c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"),
            None
        );
    }
}
