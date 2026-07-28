use std::path::PathBuf;

use chrono::{DateTime, Local};
use image::{imageops::FilterType, DynamicImage, ImageBuffer, ImageEncoder, Rgba};
#[cfg(target_os = "macos")]
use rand::{rngs::OsRng, RngCore};
#[cfg(target_os = "macos")]
use security_framework::passwords::{get_generic_password, set_generic_password};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    capture::CapturedImage,
    commands::CaptureSettings,
    crypto::{CryptoService, EncryptionKey},
    vault,
};

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.electronicjourney.app";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "local-vault-master-key-v1";
const FILE_MAGIC: &[u8; 8] = b"EJOURNEY";
const FILE_VERSION: u8 = 1;
const KEY_VERSION: u32 = 1;
const HEADER_LENGTH: usize = FILE_MAGIC.len() + 1 + 4 + 16;
const NONCE_LENGTH: usize = 24;
const MINIMUM_CONTAINER_LENGTH: usize = HEADER_LENGTH + NONCE_LENGTH + 16;
const MAXIMUM_CONTAINER_LENGTH: u64 = 128 * 1024 * 1024;

#[derive(Debug)]
pub struct StoredCapture {
    pub cipher_size: u64,
}

#[derive(Debug, Error)]
pub enum CapturePipelineError {
    #[error("captured pixels were invalid")]
    InvalidPixels,
    #[error("image encoding failed")]
    ImageEncoding,
    #[error("local encryption key is unavailable")]
    KeyUnavailable,
    #[error("image encryption failed")]
    Encryption,
    #[error("application data directory is unavailable")]
    DataDirectory,
    #[error("encrypted capture could not be written")]
    WriteFailed,
    #[error("encrypted capture could not be read")]
    ReadFailed,
    #[error("encrypted capture container is invalid")]
    InvalidContainer,
    #[error("encrypted capture authentication failed")]
    AuthenticationFailed,
}

pub async fn persist_capture(
    app: &AppHandle,
    captured: CapturedImage,
    settings: &CaptureSettings,
) -> Result<StoredCapture, CapturePipelineError> {
    let encoded = encode_webp(captured, settings.max_width)?;
    let key = load_or_create_key()?;
    let capture_id = Uuid::new_v4();
    let container = encrypt_container(&key, capture_id, &encoded)?;

    let captures_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?
        .join("vault")
        .join("captures");
    let destination = captures_dir.join(format!("{capture_id}.ejourney"));
    vault::write_atomic(&destination, &container)
        .await
        .map_err(|_| CapturePipelineError::WriteFailed)?;

    // Calculate the digest now so the complete ciphertext has been read and
    // validated by the same code path that will later populate SQLite.
    let _cipher_sha256 = hex::encode(Sha256::digest(&container));

    Ok(StoredCapture {
        cipher_size: container.len() as u64,
    })
}

fn encrypt_container(
    key: &EncryptionKey,
    capture_id: Uuid,
    encoded: &[u8],
) -> Result<Vec<u8>, CapturePipelineError> {
    let mut associated_data = Vec::with_capacity(FILE_MAGIC.len() + 1 + 4 + 16);
    associated_data.extend_from_slice(FILE_MAGIC);
    associated_data.push(FILE_VERSION);
    associated_data.extend_from_slice(&KEY_VERSION.to_le_bytes());
    associated_data.extend_from_slice(capture_id.as_bytes());
    let encrypted = CryptoService::encrypt(&key, &encoded, &associated_data)
        .map_err(|_| CapturePipelineError::Encryption)?;

    let mut container = Vec::with_capacity(
        associated_data.len() + encrypted.nonce.len() + encrypted.ciphertext.len(),
    );
    container.extend_from_slice(&associated_data);
    container.extend_from_slice(&encrypted.nonce);
    container.extend_from_slice(&encrypted.ciphertext);
    Ok(container)
}

pub fn decrypt_saved_capture(
    app: &AppHandle,
    capture_id: Uuid,
) -> Result<Vec<u8>, CapturePipelineError> {
    let captures_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?
        .join("vault")
        .join("captures");
    let path = captures_dir.join(format!("{capture_id}.ejourney"));
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| CapturePipelineError::ReadFailed)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAXIMUM_CONTAINER_LENGTH
    {
        return Err(CapturePipelineError::InvalidContainer);
    }

    let container = std::fs::read(path).map_err(|_| CapturePipelineError::ReadFailed)?;
    let key = load_existing_key()?;
    let plaintext = decrypt_container(&key, capture_id, &container)?;
    if !matches!(
        image::guess_format(&plaintext),
        Ok(image::ImageFormat::WebP)
    ) || image::load_from_memory_with_format(&plaintext, image::ImageFormat::WebP).is_err()
    {
        let mut plaintext = plaintext;
        plaintext.zeroize();
        return Err(CapturePipelineError::InvalidContainer);
    }

    Ok(plaintext)
}

fn decrypt_container(
    key: &EncryptionKey,
    expected_capture_id: Uuid,
    container: &[u8],
) -> Result<Vec<u8>, CapturePipelineError> {
    if container.len() < MINIMUM_CONTAINER_LENGTH
        || &container[..FILE_MAGIC.len()] != FILE_MAGIC
        || container[FILE_MAGIC.len()] != FILE_VERSION
    {
        return Err(CapturePipelineError::InvalidContainer);
    }

    let key_version_start = FILE_MAGIC.len() + 1;
    let key_version = u32::from_le_bytes(
        container[key_version_start..key_version_start + 4]
            .try_into()
            .map_err(|_| CapturePipelineError::InvalidContainer)?,
    );
    if key_version != KEY_VERSION {
        return Err(CapturePipelineError::InvalidContainer);
    }

    let capture_id_start = key_version_start + 4;
    let stored_capture_id = Uuid::from_slice(&container[capture_id_start..capture_id_start + 16])
        .map_err(|_| CapturePipelineError::InvalidContainer)?;
    if stored_capture_id != expected_capture_id {
        return Err(CapturePipelineError::InvalidContainer);
    }

    let mut nonce = [0_u8; NONCE_LENGTH];
    nonce.copy_from_slice(&container[HEADER_LENGTH..HEADER_LENGTH + NONCE_LENGTH]);
    let payload = crate::crypto::EncryptedPayload {
        nonce,
        ciphertext: container[HEADER_LENGTH + NONCE_LENGTH..].to_vec(),
    };
    CryptoService::decrypt(key, &payload, &container[..HEADER_LENGTH])
        .map_err(|_| CapturePipelineError::AuthenticationFailed)
}

pub fn capture_inventory(app: &AppHandle) -> Result<(u32, u64), CapturePipelineError> {
    let captures_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?
        .join("vault")
        .join("captures");
    let entries = match std::fs::read_dir(captures_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(_) => return Err(CapturePipelineError::WriteFailed),
    };

    let mut count = 0_u32;
    let mut bytes = 0_u64;
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("ejourney") {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            bytes = bytes.saturating_add(metadata.len());
            if metadata
                .modified()
                .ok()
                .map(DateTime::<Local>::from)
                .is_some_and(|modified| modified.date_naive() == Local::now().date_naive())
            {
                count = count.saturating_add(1);
            }
        }
    }
    Ok((count, bytes))
}

fn encode_webp(captured: CapturedImage, max_width: u32) -> Result<Vec<u8>, CapturePipelineError> {
    let image =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(captured.width, captured.height, captured.rgba)
            .ok_or(CapturePipelineError::InvalidPixels)?;
    let mut image = DynamicImage::ImageRgba8(image);
    if image.width() > max_width {
        let height = ((image.height() as u64 * max_width as u64) / image.width() as u64) as u32;
        image = image.resize_exact(max_width, height.max(1), FilterType::Lanczos3);
    }

    let rgba = image.to_rgba8();
    let mut encoded = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| CapturePipelineError::ImageEncoding)?;
    Ok(encoded)
}

#[cfg(target_os = "macos")]
fn load_or_create_key() -> Result<EncryptionKey, CapturePipelineError> {
    match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(value) => {
            let bytes: [u8; 32] = value
                .try_into()
                .map_err(|_| CapturePipelineError::KeyUnavailable)?;
            Ok(EncryptionKey::from_bytes(bytes))
        }
        Err(_) => {
            let mut bytes = [0_u8; 32];
            OsRng.fill_bytes(&mut bytes);
            set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &bytes)
                .map_err(|_| CapturePipelineError::KeyUnavailable)?;
            Ok(EncryptionKey::from_bytes(bytes))
        }
    }
}

#[cfg(target_os = "macos")]
fn load_existing_key() -> Result<EncryptionKey, CapturePipelineError> {
    let value = get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|_| CapturePipelineError::KeyUnavailable)?;
    let bytes: [u8; 32] = value
        .try_into()
        .map_err(|_| CapturePipelineError::KeyUnavailable)?;
    Ok(EncryptionKey::from_bytes(bytes))
}

#[cfg(not(target_os = "macos"))]
fn load_or_create_key() -> Result<EncryptionKey, CapturePipelineError> {
    // Windows capture remains disabled until its DPAPI-backed key storage and
    // Windows.Graphics.Capture adapter are implemented together.
    Err(CapturePipelineError::KeyUnavailable)
}

#[cfg(not(target_os = "macos"))]
fn load_existing_key() -> Result<EncryptionKey, CapturePipelineError> {
    Err(CapturePipelineError::KeyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_rejects_an_invalid_pixel_buffer() {
        let captured = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![0; 15],
        };

        assert!(matches!(
            encode_webp(captured, 2560),
            Err(CapturePipelineError::InvalidPixels)
        ));
    }

    #[test]
    fn encoding_respects_the_maximum_width() {
        let captured = CapturedImage {
            width: 4,
            height: 2,
            rgba: vec![255; 4 * 2 * 4],
        };
        let encoded = encode_webp(captured, 2).unwrap();
        let decoded =
            image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP).unwrap();

        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 1);
    }

    #[test]
    fn container_header_is_authenticated() {
        let key = EncryptionKey::generate();
        let capture_id = Uuid::new_v4();
        let mut container = encrypt_container(&key, capture_id, b"encoded image").unwrap();

        assert_eq!(
            decrypt_container(&key, capture_id, &container).unwrap(),
            b"encoded image"
        );
        container[8] ^= 1;
        assert!(decrypt_container(&key, capture_id, &container).is_err());
    }

    #[test]
    fn container_cannot_be_opened_under_another_capture_id() {
        let key = EncryptionKey::generate();
        let capture_id = Uuid::new_v4();
        let container = encrypt_container(&key, capture_id, b"encoded image").unwrap();

        assert!(matches!(
            decrypt_container(&key, Uuid::new_v4(), &container),
            Err(CapturePipelineError::InvalidContainer)
        ));
    }

    #[test]
    fn truncated_container_returns_no_plaintext() {
        let key = EncryptionKey::generate();
        let capture_id = Uuid::new_v4();
        let container = encrypt_container(&key, capture_id, b"encoded image").unwrap();

        assert!(matches!(
            decrypt_container(&key, capture_id, &container[..HEADER_LENGTH]),
            Err(CapturePipelineError::InvalidContainer)
        ));
    }
}
