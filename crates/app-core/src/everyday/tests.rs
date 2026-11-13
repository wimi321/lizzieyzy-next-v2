#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

fn png(color: png::ColorType, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    bytes
}

#[test]
fn png_decodes_color_and_alpha_without_os_clipboard() {
    for (color, source, expected) in [
        (
            png::ColorType::Rgba,
            vec![12, 34, 56, 78],
            vec![12, 34, 56, 78],
        ),
        (png::ColorType::Rgb, vec![12, 34, 56], vec![12, 34, 56, 255]),
        (png::ColorType::Grayscale, vec![12], vec![12, 12, 12, 255]),
        (
            png::ColorType::GrayscaleAlpha,
            vec![12, 78],
            vec![12, 12, 12, 78],
        ),
    ] {
        assert_eq!(
            decode(&png(color, &source)).unwrap(),
            ClipboardImage {
                width: 1,
                height: 1,
                rgba: expected
            }
        );
    }
}

#[tokio::test]
async fn png_rejects_non_png_truncated_corrupt_and_oversized_input() {
    assert!(matches!(
        decode(b"not a PNG"),
        Err(AppError::Png(PngError::Invalid(_)))
    ));
    let valid = png(png::ColorType::Rgba, &[1, 2, 3, 4]);
    assert!(decode(&valid[..valid.len() - 8]).is_err());
    let mut corrupt = valid;
    corrupt[29] ^= 1; // IHDR CRC, not just the magic signature.
    assert!(decode(&corrupt).is_err());
    assert!(matches!(
        decode_png(vec![0; MAX_PNG_BYTES + 1]).await,
        Err(AppError::Png(PngError::TooLarge))
    ));
}

#[test]
fn png_rejects_excessive_decoded_dimensions_before_allocation() {
    let mut bytes = Vec::new();
    {
        let encoder = png::Encoder::new(&mut bytes, 100_000, 100_000);
        let _writer = encoder.write_header().unwrap();
    }
    bytes.truncate(33);
    bytes.extend_from_slice(&png(png::ColorType::Rgba, &[1, 2, 3, 4])[33..]);
    assert!(matches!(
        decode(&bytes),
        Err(AppError::Png(PngError::DimensionsTooLarge))
    ));
}
