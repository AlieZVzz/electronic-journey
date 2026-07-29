use sha2::{Digest, Sha256};

const PIXEL_FINGERPRINT_VERSION: &[u8] = b"electronic-journey-pixels-v1";

pub fn normalize_alpha_and_hash(width: u32, height: u32, rgba: &mut [u8]) -> Option<String> {
    let expected_length = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)?;
    if rgba.len() != expected_length {
        return None;
    }

    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = u8::MAX;
    }

    let mut digest = Sha256::new();
    digest.update(PIXEL_FINGERPRINT_VERSION);
    digest.update(width.to_le_bytes());
    digest.update(height.to_le_bytes());
    digest.update(rgba);
    Some(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_padding_does_not_change_the_normalized_fingerprint() {
        let mut transparent = vec![12, 34, 56, 0];
        let mut opaque = vec![12, 34, 56, 255];

        assert_eq!(
            normalize_alpha_and_hash(1, 1, &mut transparent),
            normalize_alpha_and_hash(1, 1, &mut opaque)
        );
        assert_eq!(transparent[3], u8::MAX);
    }

    #[test]
    fn dimensions_and_pixel_changes_change_the_fingerprint() {
        let mut first = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let mut changed = first.clone();
        changed[1] += 1;
        let mut reshaped = first.clone();

        let first_hash = normalize_alpha_and_hash(2, 1, &mut first).unwrap();
        assert_ne!(
            first_hash,
            normalize_alpha_and_hash(2, 1, &mut changed).unwrap()
        );
        assert_ne!(
            first_hash,
            normalize_alpha_and_hash(1, 2, &mut reshaped).unwrap()
        );
    }

    #[test]
    fn invalid_pixel_length_is_rejected() {
        assert!(normalize_alpha_and_hash(2, 2, &mut [0; 15]).is_none());
    }
}
