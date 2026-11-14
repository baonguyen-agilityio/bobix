//! The keys of a device. A desktop has two (`auth`, `box`), a phone has three (`auth`, `box`, `sign`).
//! The `sign` private key lives in the phone's secure hardware and never reaches this crate: here there is only the
//! public half (to verify) and a software signer for tests and `fake-phone`.

use crate::util::{hex, random, sha256};
use crypto_box::{PublicKey, SecretKey};
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use p256::ecdsa::{signature, signature::Verifier as _, Signature, VerifyingKey as P256Verifying};
use zeroize::Zeroizing;

/// The device id: the first 16 bytes of SHA-256 of the `auth` public key, in hex.
pub fn device_id(auth_public: &[u8; 32]) -> String {
    hex(&sha256(&[auth_public])[..16])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicKeys {
    pub auth: [u8; 32],
    pub boxk: [u8; 32],
}

impl PublicKeys {
    pub fn device_id(&self) -> String {
        device_id(&self.auth)
    }
}

/// `auth` (Ed25519, proves the device to the relay, never needs biometrics) and `box` (X25519, seals and opens packets).
pub struct Identity {
    auth: SigningKey,
    boxk: SecretKey,
}

impl Identity {
    pub fn generate() -> Self {
        Self::from_secrets(random::<32>(), random::<32>())
    }

    /// `auth_seed` and `box_secret` are the 32-byte secrets the caller keeps in its keychain.
    pub fn from_secrets(auth_seed: [u8; 32], box_secret: [u8; 32]) -> Self {
        Identity { auth: SigningKey::from_bytes(&auth_seed), boxk: SecretKey::from(box_secret) }
    }

    pub fn auth_seed(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.auth.to_bytes())
    }

    pub fn box_secret(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.boxk.to_bytes())
    }

    pub fn public(&self) -> PublicKeys {
        PublicKeys { auth: self.auth.verifying_key().to_bytes(), boxk: *self.boxk.public_key().as_bytes() }
    }

    pub fn device_id(&self) -> String {
        self.public().device_id()
    }

    pub fn sign_auth(&self, msg: &[u8]) -> [u8; 64] {
        self.auth.sign(msg).to_bytes()
    }
}

/// True when `sig` is a valid Ed25519 signature of `msg` by `auth_public`.
pub fn verify_auth(auth_public: &[u8; 32], msg: &[u8], sig: &[u8]) -> bool {
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_bytes(auth_public), <[u8; 64]>::try_from(sig)) else {
        return false;
    };
    key.verify_strict(msg, &ed25519_dalek::Signature::from_bytes(&sig)).is_ok()
}

pub fn box_public(bytes: [u8; 32]) -> PublicKey {
    PublicKey::from(bytes)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SignError {
    /// The key is not a SEC1 point on P-256, or the signature is not DER.
    Encoding,
    Invalid,
}

/// The public half of a phone's `sign` key (ECDSA P-256), as a compressed SEC1 point.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SignPublic(P256Verifying);

impl SignPublic {
    pub fn from_sec1(bytes: &[u8]) -> Result<Self, SignError> {
        P256Verifying::from_sec1_bytes(bytes).map(SignPublic).map_err(|_| SignError::Encoding)
    }

    pub fn to_sec1(&self) -> [u8; 33] {
        self.0.to_sec1_point(true).as_bytes().try_into().expect("a compressed P-256 point is 33 bytes")
    }

    /// `der` is an ECDSA signature over SHA-256(`msg`) in DER. A high-S signature verifies too: the secure hardware
    /// does not normalise S.
    pub fn verify(&self, msg: &[u8], der: &[u8]) -> Result<(), SignError> {
        let sig = Signature::from_der(der).map_err(|_| SignError::Encoding)?;
        let sig = sig.normalize_s();
        self.0.verify(msg, &sig).map_err(|_| SignError::Invalid)
    }
}

/// A software P-256 key for tests and `fake-phone`. A real phone signs in its secure hardware; nothing in the
/// desktop or the relay may use this.
pub mod soft {
    use super::*;
    use p256::ecdsa::SigningKey as P256Signing;

    pub struct SoftSigner(P256Signing);

    impl SoftSigner {
        pub fn generate() -> Self {
            loop {
                if let Ok(k) = P256Signing::from_slice(&random::<32>()) {
                    return SoftSigner(k);
                }
            }
        }
        pub fn from_secret(secret: &[u8; 32]) -> Option<Self> {
            P256Signing::from_slice(secret).ok().map(SoftSigner)
        }
        pub fn public(&self) -> SignPublic {
            SignPublic(*self.0.verifying_key())
        }
        /// DER signature over SHA-256(`msg`).
        pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
            let sig: Signature = signature::Signer::sign(&self.0, msg);
            sig.to_der().as_bytes().to_vec()
        }
    }
}
