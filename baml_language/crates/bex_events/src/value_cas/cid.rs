use std::{error::Error, fmt, str::FromStr};

/// Version of the canonical value-node codec.
pub const NODE_CODEC_VERSION: u16 = 1;
const CID_DOMAIN: &[u8] = b"BAML-VALUE-NODE\0";

/// BLAKE3-256 content identifier for one canonical value DAG node.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cid([u8; 32]);

impl Cid {
    #[must_use]
    pub fn for_node(canonical_node_bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(CID_DOMAIN);
        hasher.update(&NODE_CODEC_VERSION.to_le_bytes());
        hasher.update(canonical_node_bytes);
        Self(*hasher.finalize().as_bytes())
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

impl fmt::Debug for Cid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Cid").field(&self.to_hex()).finish()
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl FromStr for Cid {
    type Err = CidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 {
            return Err(CidParseError::Length(value.len()));
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (decode_hex(pair[0]).ok_or(CidParseError::Character(pair[0]))? << 4)
                | decode_hex(pair[1]).ok_or(CidParseError::Character(pair[1]))?;
        }
        Ok(Self(bytes))
    }
}

fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CidParseError {
    Length(usize),
    Character(u8),
}

impl fmt::Display for CidParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(length) => write!(
                formatter,
                "CID must contain 64 hexadecimal characters, got {length}"
            ),
            Self::Character(character) => write!(
                formatter,
                "CID contains non-hexadecimal byte 0x{character:02x}"
            ),
        }
    }
}

impl Error for CidParseError {}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::Cid;

    #[test]
    fn cid_hex_is_fixed_lowercase_and_round_trips() {
        let cid = Cid::from_bytes([0xab; 32]);
        assert_eq!(cid.to_hex(), "ab".repeat(32));
        assert_eq!(Cid::from_str(&cid.to_hex()), Ok(cid));
        assert_eq!(Cid::from_str(&"AB".repeat(32)), Ok(cid));
    }
}
