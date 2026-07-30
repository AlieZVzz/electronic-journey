use sha2::{Digest, Sha256};

use crate::capture::PixelRect;

const PIXEL_FINGERPRINT_VERSION: &[u8] = b"electronic-journey-pixels-v1";
const STABLE_CONTENT_FINGERPRINT_VERSION: &[u8] = b"electronic-journey-stable-content-v1";

pub const COMPARISON_POLICY: &str = "system-ui-v1";

pub struct PixelFingerprints {
    pub pixel_sha256: String,
    pub stable_content_sha256: Option<String>,
}

pub fn normalize_alpha_and_hash(width: u32, height: u32, rgba: &mut [u8]) -> Option<String> {
    normalize_alpha_and_fingerprint(width, height, rgba, None).map(|value| value.pixel_sha256)
}

pub fn normalize_alpha_and_fingerprint(
    width: u32,
    height: u32,
    rgba: &mut [u8],
    comparison_exclusions: Option<&[PixelRect]>,
) -> Option<PixelFingerprints> {
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
    digest.update(&*rgba);
    let pixel_sha256 = hex::encode(digest.finalize());

    let stable_content_sha256 = comparison_exclusions
        .and_then(|exclusions| stable_content_hash(width, height, rgba, exclusions));
    Some(PixelFingerprints {
        pixel_sha256,
        stable_content_sha256,
    })
}

fn stable_content_hash(
    width: u32,
    height: u32,
    rgba: &[u8],
    exclusions: &[PixelRect],
) -> Option<String> {
    if width == 0 || height == 0 {
        return None;
    }
    let mut exclusions = exclusions.to_vec();
    exclusions.sort_unstable();
    for (index, exclusion) in exclusions.iter().enumerate() {
        let right = exclusion.x.checked_add(exclusion.width)?;
        let bottom = exclusion.y.checked_add(exclusion.height)?;
        let thin_horizontal_edge =
            (exclusion.y == 0 || bottom == height) && exclusion.height <= (height / 10).max(1);
        let thin_vertical_edge =
            (exclusion.x == 0 || right == width) && exclusion.width <= (width / 10).max(1);
        if exclusion.width == 0
            || exclusion.height == 0
            || right > width
            || bottom > height
            || (!thin_horizontal_edge && !thin_vertical_edge)
        {
            return None;
        }
        if exclusions[..index]
            .iter()
            .any(|other| rectangles_overlap(*other, *exclusion))
        {
            return None;
        }
    }

    let mut digest = Sha256::new();
    digest.update(STABLE_CONTENT_FINGERPRINT_VERSION);
    digest.update(COMPARISON_POLICY.as_bytes());
    digest.update(width.to_le_bytes());
    digest.update(height.to_le_bytes());
    digest.update(u32::try_from(exclusions.len()).ok()?.to_le_bytes());
    for exclusion in &exclusions {
        digest.update(exclusion.x.to_le_bytes());
        digest.update(exclusion.y.to_le_bytes());
        digest.update(exclusion.width.to_le_bytes());
        digest.update(exclusion.height.to_le_bytes());
    }

    let row_length = usize::try_from(width).ok()?.checked_mul(4)?;
    for y in 0..height {
        let mut intervals: Vec<(u32, u32)> = exclusions
            .iter()
            .filter(|rect| y >= rect.y && y < rect.y + rect.height)
            .map(|rect| (rect.x, rect.x + rect.width))
            .collect();
        intervals.sort_unstable();
        let row_start = usize::try_from(y).ok()?.checked_mul(row_length)?;
        let mut content_x = 0;
        for (start, end) in intervals {
            if content_x < start {
                let start_byte =
                    row_start.checked_add(usize::try_from(content_x).ok()?.checked_mul(4)?)?;
                let end_byte =
                    row_start.checked_add(usize::try_from(start).ok()?.checked_mul(4)?)?;
                digest.update(&rgba[start_byte..end_byte]);
            }
            content_x = end;
        }
        if content_x < width {
            let start_byte =
                row_start.checked_add(usize::try_from(content_x).ok()?.checked_mul(4)?)?;
            let row_end = row_start.checked_add(row_length)?;
            digest.update(&rgba[start_byte..row_end]);
        }
    }
    Some(hex::encode(digest.finalize()))
}

fn rectangles_overlap(first: PixelRect, second: PixelRect) -> bool {
    first.x < second.x + second.width
        && second.x < first.x + first.width
        && first.y < second.y + second.height
        && second.y < first.y + first.height
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

    #[test]
    fn stable_content_ignores_only_the_declared_edge_region() {
        let exclusion = PixelRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        };
        let mut original = vec![
            10, 10, 10, 255, 20, 20, 20, 255, 30, 30, 30, 255, 40, 40, 40, 255, 50, 50, 50, 255,
            60, 60, 60, 255,
        ];
        let first =
            normalize_alpha_and_fingerprint(3, 2, &mut original, Some(&[exclusion])).unwrap();

        let mut system_bar_changed = original.clone();
        system_bar_changed[0] = 200;
        let second =
            normalize_alpha_and_fingerprint(3, 2, &mut system_bar_changed, Some(&[exclusion]))
                .unwrap();
        assert_ne!(first.pixel_sha256, second.pixel_sha256);
        assert_eq!(first.stable_content_sha256, second.stable_content_sha256);

        let mut content_changed = original.clone();
        content_changed[12] = 200;
        let third = normalize_alpha_and_fingerprint(3, 2, &mut content_changed, Some(&[exclusion]))
            .unwrap();
        assert_ne!(first.stable_content_sha256, third.stable_content_sha256);
    }

    #[test]
    fn stable_content_rejects_invalid_or_overlapping_regions() {
        let mut rgba = vec![0; 4 * 4 * 4];
        let out_of_bounds = PixelRect {
            x: 0,
            y: 0,
            width: 5,
            height: 1,
        };
        let fingerprint =
            normalize_alpha_and_fingerprint(4, 4, &mut rgba, Some(&[out_of_bounds])).unwrap();
        assert!(fingerprint.stable_content_sha256.is_none());

        let overlapping = [
            PixelRect {
                x: 0,
                y: 0,
                width: 4,
                height: 1,
            },
            PixelRect {
                x: 0,
                y: 0,
                width: 1,
                height: 4,
            },
        ];
        let fingerprint =
            normalize_alpha_and_fingerprint(4, 4, &mut rgba, Some(&overlapping)).unwrap();
        assert!(fingerprint.stable_content_sha256.is_none());

        let oversized = PixelRect {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        let fingerprint =
            normalize_alpha_and_fingerprint(4, 4, &mut rgba, Some(&[oversized])).unwrap();
        assert!(fingerprint.stable_content_sha256.is_none());
    }
}
