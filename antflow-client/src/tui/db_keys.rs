// tui/db_keys.rs - Stub for DB key functions
use rusqlite::Connection;

pub fn load_provider_keys_multi(conn: &Connection) -> std::collections::HashMap<String, Vec<String>> {
    std::collections::HashMap::new()
}

pub fn save_provider_key(conn: &Connection, provider_id: &str, key: &str) {}

pub fn load_active_key_indices(conn: &Connection) -> std::collections::HashMap<String, usize> {
    std::collections::HashMap::new()
}