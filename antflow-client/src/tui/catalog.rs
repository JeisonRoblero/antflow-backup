// tui/catalog.rs - Stub for model catalogs

pub const CLOUD_MODELS: &[(&str, &str)] = &[];

pub const OLLAMA_CATALOG: &[(&str, f32, bool)] = &[];

pub fn estimate_token_limit(model: &str) -> u32 {
    8192
}