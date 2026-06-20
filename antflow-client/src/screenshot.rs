// screenshot.rs
// Captures the primary display and runs the redaction pipeline.
// Returns raw PNG bytes ready to be base64-encoded and sent to the AI.

use anyhow::{Context, Result};
use image::DynamicImage;

use crate::redact::{self, RedactPatterns};

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

// Captures the primary screen and returns a DynamicImage.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn capture_primary() -> Result<DynamicImage> {
    use screenshots::Screen;
    
    // `Screen::all()` returns all connected monitors.
    let screens = Screen::all().context("Failed to enumerate screens")?;
    let screen = screens.into_iter().next().context("No screen found")?;

    // `capture()` takes a full screenshot and returns an `image::RgbaImage`.
    let rgba_img = screen.capture().context("Failed to capture screen")?;

    // Wrap in DynamicImage so the redact module can work with it generically.
    Ok(DynamicImage::ImageRgba8(rgba_img))
}

#[cfg(target_os = "android")]
pub fn capture_primary() -> Result<DynamicImage> {
    use std::process::Command;

    // Run native Android screencap utility
    let output = Command::new("screencap")
        .arg("-p")
        .output()
        .context("Failed to execute system 'screencap' tool")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Android screencap failed: {}", stderr.trim());
    }

    let img = image::load_from_memory(&output.stdout)
        .context("Failed to parse system screencap PNG output")?;

    Ok(img)
}

#[cfg(target_os = "ios")]
pub fn capture_primary() -> Result<DynamicImage> {
    anyhow::bail!("Direct frame capture is not supported on iOS due to platform security sandbox restrictions.")
}

// ---------------------------------------------------------------------------
// Full pipeline: capture → redact → encode
// ---------------------------------------------------------------------------

// Captures the screen, redacts sensitive content, and returns PNG bytes.
#[cfg(not(target_os = "ios"))]
pub fn capture_redacted(patterns: &RedactPatterns) -> Result<Vec<u8>> {
    let raw = capture_primary()?;
    let redacted = redact::redact(raw, patterns);
    let png_bytes = redact::encode_png(&redacted)?;
    Ok(png_bytes)
}

#[cfg(target_os = "ios")]
pub fn capture_redacted(_patterns: &RedactPatterns) -> Result<Vec<u8>> {
    anyhow::bail!("Direct frame capture is not supported on iOS due to platform security sandbox restrictions.")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Element {
    pub id: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn extract_ocr_elements(image: &DynamicImage) -> Result<Vec<Element>> {
    use gag::Gag;

    let original_w = image.width();
    let original_h = image.height();

    // Scale down to a standard processing width if it is large to speed up Tesseract OCR significantly
    let scale_factor = if original_w > 1024 {
        1024.0 / original_w as f64
    } else {
        1.0
    };

    let processed_image = if scale_factor < 1.0 {
        let new_w = (original_w as f64 * scale_factor) as u32;
        let new_h = (original_h as f64 * scale_factor) as u32;
        image.resize(new_w, new_h, image::imageops::FilterType::Triangle)
    } else {
        image.clone()
    };

    let tmp_path = std::env::temp_dir().join("antflow_ocr_elements_tmp.png");
    processed_image.save(&tmp_path)?;

    let args = rusty_tesseract::Args {
        lang: "eng".to_string(),
        config_variables: std::collections::HashMap::from([
            ("tessedit_create_tsv".to_string(), "1".to_string()),
        ]),
        dpi: Some(150),
        psm: Some(11),
        oem: Some(3),
    };

    let input = rusty_tesseract::Image::from_path(&tmp_path)?;
    let tsv = {
        let _gag = Gag::stdout().ok();
        rusty_tesseract::image_to_data(&input, &args)?
    };

    let _ = std::fs::remove_file(&tmp_path);

    let mut elements = Vec::new();
    let mut id_counter = 1;

    for line in tsv.output.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect::<Vec<&str>>();
        if cols.len() < 12 {
            continue;
        }

        let left: u32 = cols[6].parse().unwrap_or(0);
        let top: u32 = cols[7].parse().unwrap_or(0);
        let width: u32 = cols[8].parse().unwrap_or(0);
        let height: u32 = cols[9].parse().unwrap_or(0);
        let conf: f32 = cols[10].parse().unwrap_or(0.0);
        let text = cols[11].trim();

        if conf < 40.0 || text.is_empty() {
            continue;
        }

        let orig_left = (left as f64 / scale_factor).round() as i32;
        let orig_top = (top as f64 / scale_factor).round() as i32;
        let orig_width = (width as f64 / scale_factor).round() as i32;
        let orig_height = (height as f64 / scale_factor).round() as i32;

        let center_x = orig_left + orig_width / 2;
        let center_y = orig_top + orig_height / 2;

        elements.push(Element {
            id: format!("el_{}", id_counter),
            label: text.to_string(),
            x: center_x,
            y: center_y,
            w: orig_width,
            h: orig_height,
        });
        id_counter += 1;
    }

    Ok(elements)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn extract_ocr_elements(_image: &DynamicImage) -> Result<Vec<Element>> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "ios"))]
pub fn capture_redacted_with_elements(patterns: &RedactPatterns) -> Result<(Vec<u8>, Vec<Element>)> {
    let raw = capture_primary()?;
    let redacted = redact::redact(raw, patterns);
    let elements = extract_ocr_elements(&redacted).unwrap_or_default();
    let png_bytes = redact::encode_png(&redacted)?;
    Ok((png_bytes, elements))
}

#[cfg(target_os = "ios")]
pub fn capture_redacted_with_elements(_patterns: &RedactPatterns) -> Result<(Vec<u8>, Vec<Element>)> {
    anyhow::bail!("Direct frame capture is not supported on iOS due to platform security sandbox restrictions.")
}

// ---------------------------------------------------------------------------
// Base64 helper (Compiled globally on all targets)
// ---------------------------------------------------------------------------

// Encodes raw bytes to a base64 string for embedding in JSON payloads.
pub fn to_base64(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut output = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;

    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] as u32 } else { 0 };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        output.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        output.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if i + 1 < bytes.len() {
            output.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }

        if i + 2 < bytes.len() {
            output.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }

        i += 3;
    }

    output
}

pub fn draw_indicator(img: &mut DynamicImage, points: &[(i32, i32)]) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    let mut rgba_img = img.to_rgba8();

    let color = image::Rgba([255, 0, 0, 255]); // Neon red indicator

    for &(x, y) in points {
        // Draw crosshair
        for dx in -15..=15 {
            let px = x + dx;
            if px >= 0 && px < width && y >= 0 && y < height {
                rgba_img.put_pixel(px as u32, y as u32, color);
            }
        }
        for dy in -15..=15 {
            let py = y + dy;
            if x >= 0 && x < width && py >= 0 && py < height {
                rgba_img.put_pixel(x as u32, py as u32, color);
            }
        }
        // Draw circle of radius 8
        for r in 1..=8 {
            for theta in 0..360 {
                let rad = (theta as f32).to_radians();
                let px = x + (r as f32 * rad.cos()) as i32;
                let py = y + (r as f32 * rad.sin()) as i32;
                if px >= 0 && px < width && py >= 0 && py < height {
                    rgba_img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }

    if points.len() == 2 {
        let (x1, y1) = points[0];
        let (x2, y2) = points[1];
        let steps = 100;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let px = x1 + (t * (x2 - x1) as f32) as i32;
            let py = y1 + (t * (y2 - y1) as f32) as i32;
            if px >= 0 && px < width && py >= 0 && py < height {
                rgba_img.put_pixel(px as u32, py as u32, color);
            }
        }
    }

    *img = DynamicImage::ImageRgba8(rgba_img);
}

#[cfg(not(target_os = "ios"))]
pub fn capture_with_overlay(patterns: &RedactPatterns, points: &[(i32, i32)]) -> Result<Vec<u8>> {
    let mut raw = capture_primary()?;
    draw_indicator(&mut raw, points);
    let redacted = redact::redact(raw, patterns);
    let png_bytes = redact::encode_png(&redacted)?;
    Ok(png_bytes)
}

#[cfg(target_os = "ios")]
pub fn capture_with_overlay(_patterns: &RedactPatterns, _points: &[(i32, i32)]) -> Result<Vec<u8>> {
    anyhow::bail!("Visual actions not supported on iOS.")
}
