// scheduler.rs
// Background task scheduler.
// Loads scheduled tasks from SQLite, waits until each one is due,
// then fires the task through the normal execution pipeline.
//
// The scheduler runs as a separate async task spawned by main().
// It communicates with the main loop through channels.

use anyhow::Result;
use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::{self, ScheduledTask};
use crate::executor::StopFlag;

// ---------------------------------------------------------------------------
// Add a new scheduled task
// ---------------------------------------------------------------------------

// Parses a cron expression and saves a new ScheduledTask to the database.
// `prompt` is the user's original request that will be re-submitted when the
// task fires.
#[allow(dead_code)]
pub fn add_task(
    conn: &rusqlite::Connection,
    prompt: &str,
    cron_expr: &str,
    description: &str,
) -> Result<ScheduledTask> {
    // `Schedule::from_str` parses the cron expression.
    // Returns an error if the expression is invalid.
    let schedule = Schedule::from_str(cron_expr)
        .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", cron_expr, e))?;

    // Calculate the first fire time from now.
    let next_run = schedule
        .upcoming(Utc)
        .next()
        .ok_or_else(|| anyhow::anyhow!("Cron expression has no upcoming occurrences"))?;

    let task = ScheduledTask {
        id: Uuid::new_v4().to_string(),
        prompt: prompt.to_string(),
        schedule: cron_expr.to_string(),
        enabled: true,
        last_run: None,
        next_run,
        created_at: Utc::now(),
    };

    db::insert_task(conn, &task)?;

    println!("\nTask scheduled: {}", description);
    println!("  First run: {}", next_run.format("%Y-%m-%d %H:%M UTC"));
    println!("  To manage scheduled tasks run: antflow tasks");

    Ok(task)
}

// ---------------------------------------------------------------------------
// Background scheduler loop
// ---------------------------------------------------------------------------

// Runs continuously in the background, checking every `poll_interval` seconds
// whether any task is due. When a task fires, its prompt is sent to the
// provided callback channel.
//
// `conn` is wrapped in an Arc<Mutex<>> so it can be safely shared between the
// scheduler task and the main async task.
// Arc = Atomically Reference Counted (shared ownership across threads).
// Mutex = Mutual Exclusion (only one task accesses the connection at a time).
pub async fn run_scheduler_loop(
    conn: Arc<Mutex<rusqlite::Connection>>,
    poll_secs: u64,
    stop: StopFlag,
    // `tx` is the sending end of a channel. When a task fires, its prompt
    // is sent through this channel to the main loop for execution.
    tx: tokio::sync::mpsc::Sender<String>,
) {
    // `tokio::time::interval` creates a timer that ticks every `poll_secs`.
    let mut interval = tokio::time::interval(
        std::time::Duration::from_secs(poll_secs)
    );

    loop {
        // Wait for the next tick before checking tasks.
        interval.tick().await;

        // Check the stop flag. `Ordering::Relaxed` is sufficient here because
        // we only need eventual visibility, not strict ordering guarantees.
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        let now = Utc::now();

        // Lock the database connection for the duration of this check.
        // The `guard` variable holds the lock; it is released when it drops.
        let guard = conn.lock().await;

        let tasks = match db::load_tasks(&guard) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[scheduler] Failed to load tasks: {}", e);
                continue;
            }
        };

        for task in tasks {
            if !task.enabled {
                continue;
            }

            // Check if the task is due (next_run is in the past or now).
            if task.next_run <= now {
                // Send the prompt to the main loop via the channel.
                // If the channel is full, skip this cycle to avoid blocking.
                let _ = tx.try_send(task.prompt.clone());

                // Calculate the next fire time using the cron expression.
                let next = next_run_from(&task.schedule, now);
                if let Some(next_run) = next {
                    let _ = db::update_task_run(&guard, &task.id, next_run);
                }
            }
        }
        // `guard` drops here, releasing the Mutex lock.
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Returns the next scheduled time after `after` for a cron expression.
fn next_run_from(cron_expr: &str, after: chrono::DateTime<Utc>) -> Option<chrono::DateTime<Utc>> {
    let schedule = Schedule::from_str(cron_expr).ok()?;
    schedule.after(&after).next()
}

// Lists all scheduled tasks in a human-readable table format.
pub fn print_task_list(conn: &rusqlite::Connection) -> Result<()> {
    let tasks = db::load_tasks(conn)?;

    if tasks.is_empty() {
        println!("No scheduled tasks.");
        println!("To schedule a task, describe it with a time: \"Send me a reminder every Monday at 9am\"");
        return Ok(());
    }

    println!("\nScheduled tasks:");
    println!("{:<4} {:<36} {:<20} {:<8} {}", "#", "ID", "Next Run", "Status", "Prompt");
    println!("{}", "-".repeat(90));

    for (i, task) in tasks.iter().enumerate() {
        let status = if task.enabled { "active" } else { "paused" };
        let next = task.next_run.format("%Y-%m-%d %H:%M").to_string();
        let prompt_preview: String = task.prompt.chars().take(40).collect();
        println!(
            "{:<4} {:<36} {:<20} {:<8} {}",
            i + 1, task.id, next, status, prompt_preview
        );
    }

    Ok(())
}
