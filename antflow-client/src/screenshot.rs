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
