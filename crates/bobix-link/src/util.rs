use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("the operating system has no random source");
    b
}

/// SHA-256 over the concatenation of `parts`.
pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// URL-safe base64 without padding, used for every binary value that travels in JSON or in a URI.
pub fn b64(b: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(b)
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).ok()
}

/// Builds the bytes that get signed or MACed: a domain label, then every field prefixed with its length (u32, big
/// endian), so two different field lists can never produce the same bytes.
#[derive(Default)]
pub struct Fields(Vec<u8>);

impl Fields {
    pub fn new(domain: &str) -> Self {
        Fields::default().field(domain.as_bytes())
    }
    pub fn field(mut self, f: &[u8]) -> Self {
        self.0.extend_from_slice(&(f.len() as u32).to_be_bytes());
        self.0.extend_from_slice(f);
        self
    }
    pub fn num(self, n: u64) -> Self {
        self.field(&n.to_be_bytes())
    }
    pub fn done(self) -> Vec<u8> {
        self.0
    }
}
