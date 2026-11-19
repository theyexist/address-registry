//! Just enough bech32 to check a credential against the address it came from.
//!
//! `shared-crates/address-registry` carried a test that decoded `derived_from`
//! and compared the payment credential, because a mistyped hex character names
//! an address that exists and is not the one meant. Doing the same check when a
//! row is authored moves that failure to the person who can fix it.
//!
//! Cardano addresses are bech32 (BIP-173), never bech32m. For every Shelley
//! address type the payment credential is payload bytes 1..29: the header byte
//! says what the rest is.

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const GENERATOR: [u32; 5] = [
    0x3b6a_57b2,
    0x2650_8e6d,
    0x1ea1_19fa,
    0x3d42_33dd,
    0x2a14_62b3,
];

/// The 56-hex payment credential an address pays to, if it decodes.
pub fn payment_credential(address: &str) -> Option<String> {
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
    let payload = from_five_bits(&values[..values.len() - 6])?;
    let credential = payload.get(1..29)?;
    Some(hex(credential))
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_script_enterprise_address() {
        // The TOOL lock, paired with its credential in the Koios fixture the
        // holder walk runs against.
        assert_eq!(
            payment_credential("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j")
                .as_deref(),
            Some("c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f")
        );
    }

    #[test]
    fn decodes_a_base_address_past_its_stake_part() {
        // CrowdLock, from the same fixture set: header, payment script, stake key.
        assert_eq!(
            payment_credential(
                "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkdhym9auk0rgpz3lurkryvhl55046ak6ex4tlyj6mxxj735syr937w"
            )
            .as_deref(),
            Some("381cd9b620ce9d7c73dd45cfe207971cc79df79251003116094f6b36")
        );
    }

    #[test]
    fn a_corrupted_checksum_is_rejected() {
        assert_eq!(
            payment_credential("addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6k"),
            None
        );
    }

    #[test]
    fn a_credential_is_not_an_address() {
        assert_eq!(
            payment_credential("c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"),
            None
        );
    }
}
