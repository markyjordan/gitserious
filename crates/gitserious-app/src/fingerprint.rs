use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// A lowercase, SHA-256 semantic fingerprint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl Display for Fingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("sha256:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Fingerprint {
    type Err = FingerprintError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(FingerprintError::InvalidPrefix);
        };
        if hex.len() != 64 {
            return Err(FingerprintError::InvalidLength(hex.len()));
        }

        let mut bytes = [0_u8; 32];
        for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(FingerprintError::InvalidCharacter(index * 2))?;
            let low =
                decode_hex(pair[1]).ok_or(FingerprintError::InvalidCharacter(index * 2 + 1))?;
            bytes[index] = high << 4 | low;
        }
        Ok(Self(bytes))
    }
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// A malformed SHA-256 fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FingerprintError {
    /// The fingerprint does not begin with `sha256:`.
    InvalidPrefix,
    /// The hexadecimal payload is not exactly 64 characters.
    InvalidLength(usize),
    /// The payload contains a non-lowercase-hex character at this byte index.
    InvalidCharacter(usize),
}

impl Display for FingerprintError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrefix => formatter.write_str("fingerprint must begin with sha256:"),
            Self::InvalidLength(length) => write!(
                formatter,
                "fingerprint must contain exactly 64 hexadecimal characters, found {length}"
            ),
            Self::InvalidCharacter(index) => write!(
                formatter,
                "fingerprint contains invalid lowercase hexadecimal at byte index {index}"
            ),
        }
    }
}

impl Error for FingerprintError {}
