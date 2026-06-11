// tui/api_helpers.rs - Stub for API helper functions
use anyhow::Result;

pub async fn test_api_key(provider: &str, base_url: &str, model: &str, api_key: &str) -> Result<()> {
    Ok(())
}

pub fn format_api_error(raw: &str) -> String {
    raw.to_string()
}