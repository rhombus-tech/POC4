use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct Address {
    pub bytes: Vec<u8>,
}

impl Address {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        if s.starts_with("0x") {
            let bytes = hex::decode(&s[2..]).ok()?;
            Some(Self { bytes })
        } else {
            None
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.bytes))
    }
}

impl Hash for Address {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl PartialEq for Address {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for Address {}

impl From<[u8; 33]> for Address {
    fn from(bytes: [u8; 33]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }
}
