#![allow(dead_code)]
// ui/console.rs
// All terminal output for antflow goes through this module.
// Keeping output centralized means a future UI layer only needs to replace
// calls here rather than hunting through the whole codebase.

use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Spinner
// ---------------------------------------------------------------------------

// Starts a spinner with a message and returns the handle.
// The caller must call `stop_spinner` when the operation finishes.
// Returns a ProgressBar handle that owns the spinner state.
pub fn start_spinner(message: &str) -> ProgressBar {
    // `ProgressBar::new_spinner()` creates a spinner with no fixed length.
    let pb = ProgressBar::new_spinner();

    // The template string defines how the spinner looks.
    // {spinner} is the animated character, {elapsed_precise} is the timer,
    // {msg} is the message passed in.
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&[".", "..", "...", "    "])
            .template("{spinner} {msg} [{elapsed_precise}]")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );

    pb.set_message(message.to_string());

    // `enable_steady_tick` updates the spinner at a fixed interval.
    // 120ms gives a smooth animation without hammering the terminal.
    pb.enable_steady_tick(Duration::from_millis(120));

    pb
}

// Stops and removes the spinner from the terminal.
pub fn stop_spinner(pb: ProgressBar) {
    // `finish_and_clear` removes the spinner line entirely.
    pb.finish_and_clear();
}

// ---------------------------------------------------------------------------
// Formatted output
// ---------------------------------------------------------------------------

// Prints a message from the assistant to the user.
pub fn print_assistant(message: &str) {
    println!("\n{}\n", message);
}

// Prints a one-line description of the action the AI is about to execute.
// Kept deliberately brief so it reads as a status line, not a log dump.
pub fn print_action(description: &str) {
    println!("  -> {}", description);
}

// Prints an informational message (not from the AI, not an error).
pub fn print_info(message: &str) {
    println!("{}", message);
}

// Prints an error in a way that stands out from normal output.
pub fn print_error(message: &str) {
    eprintln!("[error] {}", message);
}

// Prints a section heading used during setup and configuration screens.
pub fn print_heading(title: &str) {
    println!("\n{}", title);
    println!("{}", "=".repeat(title.len()));
}

// ---------------------------------------------------------------------------
// User input helpers
// ---------------------------------------------------------------------------

// Reads a line of input from stdin after printing a prompt string.
// Returns an empty string if stdin is closed or an error occurs.
pub fn read_line(prompt: &str) -> String {
    print!("{}", prompt);
    // `flush()` forces the prompt to appear before the user types.
    // Without this, buffered output may not show until after the Enter key.
    let _ = io::stdout().flush();

    let mut buf = String::new();
    // `read_line` blocks until the user presses Enter.
    // The newline character is included in `buf` and must be trimmed.
    io::stdin().read_line(&mut buf).unwrap_or(0);
    buf.trim().to_string()
}

// Presents a question and an optional numbered list of options,
// then reads the user's response. If options are provided the user can
// type a number to select one or type a freeform answer.
// Returns the selected option text or the raw typed answer.
pub fn ask_clarification(question: &str, options: &[serde_json::Value]) -> String {
    println!("\n{}", question);

    if !options.is_empty() {
        for (i, opt) in options.iter().enumerate() {
            if let Some(s) = opt.as_str() {
                println!("  {}. {}", i + 1, s);
            }
        }
        print!("\nYour choice (number or text): ");
    } else {
        print!("\nYour answer: ");
    }

    let _ = io::stdout().flush();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap_or(0);
    let answer = buf.trim().to_string();

    // If the user typed a number and options exist, resolve to the option text.
    if !options.is_empty() {
        if let Ok(n) = answer.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                if let Some(s) = options[n - 1].as_str() {
                    return s.to_string();
                }
            }
        }
    }

    answer
}

// Asks a yes/no question and returns true if the user answers yes.
// Accepts "y", "yes", "1" (case-insensitive) as affirmative.
pub fn confirm(question: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    let input = read_line(&format!("{} {} ", question, hint));

    if input.is_empty() {
        return default_yes;
    }

    matches!(input.to_lowercase().as_str(), "y" | "yes" | "1")
}

// Shows a numbered menu and returns the 0-based index the user selected.
// Loops until a valid selection is made.
pub fn menu(title: &str, items: &[&str]) -> usize {
    loop {
        println!("\n{}", title);
        for (i, item) in items.iter().enumerate() {
            println!("  {}. {}", i + 1, item);
        }

        let input = read_line("Select: ");
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= items.len() {
                return n - 1;
            }
        }
        println!("Please enter a number between 1 and {}.", items.len());
    }
}

// ---------------------------------------------------------------------------
// REPL prompt
// ---------------------------------------------------------------------------

// Prints the antflow input prompt and reads the user's message.
// Returns None if the user types "exit" or "quit".
pub fn repl_prompt(session_id_short: &str) -> Option<String> {
    let input = read_line(&format!("\nantflow [{}]> ", session_id_short));

    if input.is_empty() {
        return Some(String::new());
    }

    match input.to_lowercase().as_str() {
        "exit" | "quit" | "q" => None,
        _ => Some(input),
    }
}
