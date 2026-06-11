// redact.rs
// Detects and blacks-out sensitive content in screenshots before they are
// sent to any AI provider (local or cloud).

use anyhow::Result;
use image::{DynamicImage, Rgba};
use regex::Regex;

// ---------------------------------------------------------------------------
// Regex patterns for sensitive content detection
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RedactPatterns {
    pub card_number: Regex,
    pub api_key: Regex,
    pub password_label: Regex,
    pub ssn: Regex,
    pub email: Regex,
    pub bearer_token: Regex,
}

impl RedactPatterns {
    pub fn new() -> Self {
        RedactPatterns {
            card_number: Regex::new(r"\b(?:\d[ -]?){13,19}\b").unwrap(),
            api_key: Regex::new(
                r"(?i)\b(?:sk-|pk-|AIza|xox[baprs]-)[A-Za-z0-9_\-]{16,}\b|[A-Fa-f0-9]{32,}"
            ).unwrap(),
            password_label: Regex::new(r"(?i)\bpassword\b|\bpasswd\b|\bpin\b|\bsecret\b").unwrap(),
            ssn: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
            email: Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap(),
            bearer_token: Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9\-._~+/]+=*\b").unwrap(),
        }
    }
}

// ---------------------------------------------------------------------------
// Redaction entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RedactRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

// ---------------------------------------------------------------------------
// Main redaction function
// ---------------------------------------------------------------------------

pub fn redact(image: DynamicImage, patterns: &RedactPatterns) -> DynamicImage {
    let regions = match detect_sensitive_regions(&image, patterns) {
        Ok(r) => r,
        Err(_) => {
            return image;
        }
    };

    if regions.is_empty() {
        return image;
    }

    let mut rgba = image.to_rgba8();

    for region in regions {
        paint_black(&mut rgba, &region);
    }

    DynamicImage::ImageRgba8(rgba)
}

// ---------------------------------------------------------------------------
// OCR + heuristic detection
// ---------------------------------------------------------------------------

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn detect_sensitive_regions(image: &DynamicImage, patterns: &RedactPatterns) -> Result<Vec<RedactRegion>> {
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

    let tmp_path = std::env::temp_dir().join("antflow_ocr_tmp.png");
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

    let mut regions = Vec::new();
    let mut prev_was_password_label = false;

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

        let is_password_label = patterns.password_label.is_match(text);

        let is_sensitive = patterns.card_number.is_match(text)
            || patterns.api_key.is_match(text)
            || patterns.ssn.is_match(text)
            || patterns.email.is_match(text)
            || patterns.bearer_token.is_match(text)
            || prev_was_password_label;

        if is_sensitive {
            let pad = 4u32;
            let orig_left = (left as f64 / scale_factor).round() as u32;
            let orig_top = (top as f64 / scale_factor).round() as u32;
            let orig_width = (width as f64 / scale_factor).round() as u32;
            let orig_height = (height as f64 / scale_factor).round() as u32;

            regions.push(RedactRegion {
                x: orig_left.saturating_sub(pad),
                y: orig_top.saturating_sub(pad),
                w: orig_width + pad * 2,
                h: orig_height + pad * 2,
            });
        }

        prev_was_password_label = is_password_label;
    }

    Ok(regions)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn detect_sensitive_regions(_image: &DynamicImage, _patterns: &RedactPatterns) -> Result<Vec<RedactRegion>> {
    // Return empty list of sensitive regions on mobile platforms where Tesseract is unavailable
    Ok(Vec::new())
}

// ---------------------------------------------------------------------------
// Pixel painting
// ---------------------------------------------------------------------------

fn paint_black(img: &mut image::RgbaImage, region: &RedactRegion) {
    let black = Rgba([0u8, 0, 0, 255]);

    let img_w = img.width();
    let img_h = img.height();

    let x_end = (region.x + region.w).min(img_w);
    let y_end = (region.y + region.h).min(img_h);

    for y in region.y..y_end {
        for x in region.x..x_end {
            img.put_pixel(x, y, black);
        }
    }
}

// ---------------------------------------------------------------------------
// PNG encoding
// ---------------------------------------------------------------------------

pub fn encode_png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    image.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(buf)
}
