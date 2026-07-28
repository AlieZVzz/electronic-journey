use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::{rngs::OsRng, RngCore};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct EncryptionKey([u8; 32]);

impl EncryptionKey {
    pub fn generate() -> Self {
        let mut value = [0_u8; 32];
        OsRng.fill_bytes(&mut value);
        Self(value)
    }

    pub fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }
}

#[derive(Debug)]
pub struct EncryptedPayload {
    pub nonce: [u8; 24],
    /// Ciphertext with the Poly1305 authentication tag appended.
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("the encryption key was rejected")]
    InvalidKey,
    #[error("authenticated encryption failed")]
    EncryptionFailed,
    #[error("authentication failed; no plaintext was returned")]
    AuthenticationFailed,
}

pub struct CryptoService;

impl CryptoService {
    pub fn encrypt(
        key: &EncryptionKey,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<EncryptedPayload, CryptoError> {
        let cipher =
            XChaCha20Poly1305::new_from_slice(&key.0).map_err(|_| CryptoError::InvalidKey)?;
        let mut nonce = [0_u8; 24];
        OsRng.fill_bytes(&mut nonce);

        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: associated_data,
                },
            )
            .map_err(|_| CryptoError::EncryptionFailed)?;

        Ok(EncryptedPayload { nonce, ciphertext })
    }

    pub fn decrypt(
        key: &EncryptionKey,
        payload: &EncryptedPayload,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let cipher =
            XChaCha20Poly1305::new_from_slice(&key.0).map_err(|_| CryptoError::InvalidKey)?;
        cipher
            .decrypt(
                XNonce::from_slice(&payload.nonce),
                Payload {
                    msg: &payload.ciphertext,
                    aad: associated_data,
                },
            )
            .map_err(|_| CryptoError::AuthenticationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciphertext_round_trip_requires_matching_aad() {
        let key = EncryptionKey::generate();
        let encrypted = CryptoService::encrypt(&key, b"private frame", b"capture-id").unwrap();

        assert_eq!(
            CryptoService::decrypt(&key, &encrypted, b"capture-id").unwrap(),
            b"private frame"
        );
        assert!(CryptoService::decrypt(&key, &encrypted, b"other-id").is_err());
    }

    #[test]
    fn tampering_returns_no_plaintext() {
        let key = EncryptionKey::generate();
        let mut encrypted = CryptoService::encrypt(&key, b"private frame", b"capture-id").unwrap();
        encrypted.ciphertext[0] ^= 1;

        assert!(CryptoService::decrypt(&key, &encrypted, b"capture-id").is_err());
    }
}
