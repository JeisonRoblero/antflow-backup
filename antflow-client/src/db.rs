#![allow(dead_code)]
// db.rs
// All persistent data that changes at runtime goes here: conversation history,
// the undo log for executed actions, and scheduled (cron) tasks.
// The database file is "antflow.db" inside the storage directory.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

// ---------------------------------------------------------------------------
use std::sync::atomic::{AtomicBool, Ordering};

static DB_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Database connection
// ---------------------------------------------------------------------------

// Opens (or creates) the SQLite database and ensures all tables exist.
// `storage_dir` is the folder where antflow.db will be placed.
pub fn open(storage_dir: &Path) -> Result<Connection> {
    match open_internal(storage_dir) {
        Ok(conn) => {
            DB_INITIALIZED.store(true, Ordering::Relaxed);
            Ok(conn)
        }
        Err(e) => {
            let err_msg = e.to_string().to_lowercase();
            if err_msg.contains("malformed") || err_msg.contains("corrupt") {
                eprintln!("[DB] Database is corrupted! Attempting auto-recovery. Error: {}", e);
                DB_INITIALIZED.store(false, Ordering::Relaxed);
                if let Err(rec_err) = handle_corrupted_db(storage_dir) {
                    eprintln!("[DB] Failed to rename/backup corrupted database: {}", rec_err);
                }
                match open_internal(storage_dir) {
                    Ok(conn) => {
                        DB_INITIALIZED.store(true, Ordering::Relaxed);
                        Ok(conn)
                    }
                    Err(retry_err) => {
                        anyhow::bail!("Failed to initialize database even after recovery: {}", retry_err)
                    }
                }
            } else {
                Err(e)
            }
        }
    }
}

fn open_internal(storage_dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(storage_dir)
        .context("Failed to create storage directory for database")?;

    let db_path = storage_dir.join("antflow.db");

    // Connection::open creates the file if it does not exist.
    let conn = Connection::open(&db_path)
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

    // Set a busy timeout of 5 seconds to prevent immediate SQLITE_BUSY errors
    conn.busy_timeout(std::time::Duration::from_secs(5))?;

    if !DB_INITIALIZED.load(Ordering::Relaxed) {
        // Enable WAL (Write-Ahead Logging) mode for better concurrent performance.
        // WAL allows reads while a write is in progress.
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");

        // Create all tables.
        create_tables(&conn)?;
    }

    // Quick verification query to detect corruption in any table
    conn.query_row("SELECT COUNT(*) FROM messages LIMIT 1", [], |r| r.get::<_, i32>(0))?;
    conn.query_row("SELECT COUNT(*) FROM conversation_metadata LIMIT 1", [], |r| r.get::<_, i32>(0))?;
    conn.query_row("SELECT COUNT(*) FROM scheduled_tasks LIMIT 1", [], |r| r.get::<_, i32>(0))?;
    conn.query_row("SELECT COUNT(*) FROM undo_log LIMIT 1", [], |r| r.get::<_, i32>(0))?;

    Ok(conn)
}

fn handle_corrupted_db(storage_dir: &Path) -> Result<()> {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let db_path = storage_dir.join("antflow.db");
    let wal_path = storage_dir.join("antflow.db-wal");
    let shm_path = storage_dir.join("antflow.db-shm");

    if db_path.exists() {
        let new_db_path = storage_dir.join(format!("antflow.db.corrupted_{}", timestamp));
        std::fs::rename(&db_path, &new_db_path)
            .context("Failed to rename corrupted antflow.db")?;
    }
    if wal_path.exists() {
        let new_wal_path = storage_dir.join(format!("antflow.db-wal.corrupted_{}", timestamp));
        let _ = std::fs::rename(&wal_path, &new_wal_path);
    }
    if shm_path.exists() {
        let new_shm_path = storage_dir.join(format!("antflow.db-shm.corrupted_{}", timestamp));
        let _ = std::fs::rename(&shm_path, &new_shm_path);
    }
    Ok(())
}

// Creates all required tables if they do not already exist.
// "IF NOT EXISTS" makes this safe to call on every startup.
fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- Session messages: the conversation history shown to the AI.
        CREATE TABLE IF NOT EXISTS messages (
            id          TEXT PRIMARY KEY,   -- UUID
            session_id  TEXT NOT NULL,      -- groups messages by session
            role        TEXT NOT NULL,      -- 'user', 'assistant', or 'system'
            content     TEXT NOT NULL,      -- the message text or JSON
            created_at  TEXT NOT NULL       -- ISO-8601 UTC timestamp
        );

        -- Undo log: one row per tool action executed, so it can be reversed.
        CREATE TABLE IF NOT EXISTS undo_log (
            id          TEXT PRIMARY KEY,
            session_id  TEXT NOT NULL,
            action_type TEXT NOT NULL,      -- e.g. 'mouse_click', 'type_text'
            params_json TEXT NOT NULL,      -- JSON of the original parameters
            reverse_json TEXT,             -- JSON to reverse the action, if known
            created_at  TEXT NOT NULL
        );

        -- Scheduled tasks: cron jobs saved by the user.
        CREATE TABLE IF NOT EXISTS scheduled_tasks (
            id          TEXT PRIMARY KEY,
            prompt      TEXT NOT NULL,      -- the user's original request
            schedule    TEXT NOT NULL,      -- cron expression, e.g. '0 9 * * 1'
            enabled     INTEGER NOT NULL DEFAULT 1,  -- 1 = active, 0 = paused
            last_run    TEXT,               -- ISO-8601 UTC of last execution
            next_run    TEXT NOT NULL,      -- ISO-8601 UTC of next planned run
            created_at  TEXT NOT NULL
        );

        -- Conversation names/metadata
        CREATE TABLE IF NOT EXISTS conversation_metadata (
            session_id  TEXT PRIMARY KEY,
            name        TEXT NOT NULL
        );
        "
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Message structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    // role is "user", "assistant", or "system"
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

// Inserts a new message into the database.
pub fn insert_message(conn: &Connection, session_id: &str, role: &str, content: &str) -> Result<Message> {
    let msg = Message {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: Utc::now(),
    };

    conn.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            msg.id,
            msg.session_id,
            msg.role,
            msg.content,
            msg.created_at.to_rfc3339()
        ],
    )?;

    Ok(msg)
}

// Returns up to `limit` most recent messages for a given session,
// ordered oldest-first so the AI sees chronological context.
pub fn load_messages(conn: &Connection, session_id: &str, limit: usize) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at
         FROM messages
         WHERE session_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;

    // `limit` is cast to i64 because SQLite parameters use signed integers.
    let rows = stmt.query_map(params![session_id, limit as i64], |row| {
        Ok(Message {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            // Parse the stored ISO-8601 string back into a DateTime.
            created_at: row.get::<_, String>(4)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    })?;

    // Collect rows into a Vec and reverse so they are oldest-first.
    let mut messages: Vec<Message> = rows
        .filter_map(|r| r.ok())
        .collect();
    messages.reverse();

    Ok(messages)
}

pub fn load_messages_paged(conn: &Connection, session_id: &str, limit: usize, offset: usize) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at
         FROM messages
         WHERE session_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2 OFFSET ?3"
    )?;

    let rows = stmt.query_map(params![session_id, limit as i64, offset as i64], |row| {
        Ok(Message {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            created_at: row.get::<_, String>(4)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    })?;

    let mut messages: Vec<Message> = rows
        .filter_map(|r| r.ok())
        .collect();
    messages.reverse();

    Ok(messages)
}


// Returns the total number of bytes used by the database file.
// Shown in the session-size info screen.
pub fn db_size_bytes(storage_dir: &Path) -> u64 {
    let path = storage_dir.join("antflow.db");
    std::fs::metadata(&path)
        .map(|m| m.len())
        .unwrap_or(0)
}

// Deletes all messages for a session (manual session clear).
pub fn clear_session(conn: &Connection, session_id: &str) -> Result<()> {
    conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])?;
    conn.execute("DELETE FROM conversation_metadata WHERE session_id = ?1", params![session_id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Undo log structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    pub id: String,
    pub session_id: String,
    pub action_type: String,
    pub params_json: String,
    pub reverse_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

// Appends an entry to the undo log after a tool action is executed.
pub fn log_action(
    conn: &Connection,
    session_id: &str,
    action_type: &str,
    params_json: &str,
    reverse_json: Option<&str>,
) -> Result<UndoEntry> {
    let entry = UndoEntry {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        action_type: action_type.to_string(),
        params_json: params_json.to_string(),
        reverse_json: reverse_json.map(|s| s.to_string()),
        created_at: Utc::now(),
    };

    conn.execute(
        "INSERT INTO undo_log (id, session_id, action_type, params_json, reverse_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.id,
            entry.session_id,
            entry.action_type,
            entry.params_json,
            entry.reverse_json,
            entry.created_at.to_rfc3339()
        ],
    )?;

    Ok(entry)
}

// Returns the most recent undo entries for a session, newest-first.
pub fn load_undo_log(conn: &Connection, session_id: &str, limit: usize) -> Result<Vec<UndoEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, action_type, params_json, reverse_json, created_at
         FROM undo_log
         WHERE session_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;

    let rows = stmt.query_map(params![session_id, limit as i64], |row| {
        Ok(UndoEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            action_type: row.get(2)?,
            params_json: row.get(3)?,
            reverse_json: row.get(4)?,
            created_at: row.get::<_, String>(5)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    })?;

    rows.map(|r| r.map_err(anyhow::Error::from)).collect()
}

// ---------------------------------------------------------------------------
// Scheduled task structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub prompt: String,
    pub schedule: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// Inserts a new scheduled task.
pub fn insert_task(conn: &Connection, task: &ScheduledTask) -> Result<()> {
    conn.execute(
        "INSERT INTO scheduled_tasks (id, prompt, schedule, enabled, last_run, next_run, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            task.id,
            task.prompt,
            task.schedule,
            task.enabled as i32,
            task.last_run.as_ref().map(|d| d.to_rfc3339()),
            task.next_run.to_rfc3339(),
            task.created_at.to_rfc3339()
        ],
    )?;
    Ok(())
}

// Returns all scheduled tasks ordered by next scheduled run time.
pub fn load_tasks(conn: &Connection) -> Result<Vec<ScheduledTask>> {
    let mut stmt = conn.prepare(
        "SELECT id, prompt, schedule, enabled, last_run, next_run, created_at
         FROM scheduled_tasks
         ORDER BY next_run ASC"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(ScheduledTask {
            id: row.get(0)?,
            prompt: row.get(1)?,
            schedule: row.get(2)?,
            enabled: row.get::<_, i32>(3)? != 0,
            last_run: row.get::<_, Option<String>>(4)?
                .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
            next_run: row.get::<_, String>(5)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            created_at: row.get::<_, String>(6)?
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
        })
    })?;

    rows.map(|r| r.map_err(anyhow::Error::from)).collect()
}

// Updates the last_run and next_run fields after a task fires.
pub fn update_task_run(conn: &Connection, task_id: &str, next_run: DateTime<Utc>) -> Result<()> {
    conn.execute(
        "UPDATE scheduled_tasks SET last_run = ?1, next_run = ?2 WHERE id = ?3",
        params![Utc::now().to_rfc3339(), next_run.to_rfc3339(), task_id],
    )?;
    Ok(())
}

// Deletes a scheduled task by ID.
pub fn delete_task(conn: &Connection, task_id: &str) -> Result<()> {
    conn.execute("DELETE FROM scheduled_tasks WHERE id = ?1", params![task_id])?;
    Ok(())
}

// Toggles the enabled flag of a task.
pub fn set_task_enabled(conn: &Connection, task_id: &str, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE scheduled_tasks SET enabled = ?1 WHERE id = ?2",
        params![enabled as i32, task_id],
    )?;
    Ok(())
}

// Sets custom name/title for a conversation.
pub fn set_conversation_name(conn: &Connection, session_id: &str, name: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO conversation_metadata (session_id, name) VALUES (?1, ?2)
         ON CONFLICT(session_id) DO UPDATE SET name=?2",
        params![session_id, name],
    )?;
    Ok(())
}

