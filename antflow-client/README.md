# antflow

An automation agent. Runs on Windows, macOS, and Linux.
Fully portable: can run from a USB drive without leaving traces.

---

## Requirements

- Rust (<https://rustup.rs>)
- Tesseract OCR (for screenshot redaction)
  - Linux:   sudo apt install tesseract-ocr
  - macOS:   brew install tesseract
  - Windows: <https://github.com/UB-Mannheim/tesseract/wiki>

---

## Build

```bash
# Development build (fast compile, larger binary):
cargo build

# Release build (small binary, optimized):
cargo build --release
```

The binary will be at:

- `target/debug/antflow`     (dev build)
- `target/release/antflow`   (release build)

---

## Run

```bash
# From the project directory:
./target/release/antflow

# Or copy the binary anywhere and run it:
./antflow
```

On first run the setup wizard will start automatically.

---

## Commands

```bash
antflow              Start the agent (runs wizard on first launch)
antflow setup        Re-run the setup wizard
antflow config       Change a specific setting
antflow tasks        List all scheduled tasks
antflow stop         Stop the background scheduler
```

---

## Project structure

```bash
src/
├── main.rs           # Application entry point: REPL loop & command routing
├── config.rs         # Load/save configuration (TOML)
├── db.rs             # SQLite layer: sessions, undo log, scheduled tasks
├── device.rs         # Hardware & OS information collection
├── redact.rs         # OCR-based detection and redaction of sensitive content
├── screenshot.rs     # Screen capture and PNG encoding
├── prompt.rs         # System prompt construction & tool schema builder
├── tools.rs          # Tooling: mouse, keyboard, clipboard, shell commands
├── search.rs         # DuckDuckGo search + page fetching
├── executor.rs       # Task execution loop
├── scheduler.rs      # Background job scheduler (cron-style)
├── wizard.rs         # First-run setup wizard
│
├── ai/
│   ├── mod.rs        # AI provider trait + provider factory
│   ├── ollama.rs     # Local Ollama provider implementation
│   ├── openai.rs     # OpenAI provider implementation
│   └── generic.rs    # Generic OpenAI-compatible provider (other clouds)
│
└── ui/
    ├── mod.rs        # UI module declaration
    └── console.rs    # Terminal UI: output, spinners, prompts
```

---

## Data files

In portable mode (default), all data is stored next to the binary:

```bash
antflow           the binary
config.toml       user settings (human-editable)
antflow.db        SQLite: conversation history, tasks, undo log
```

---

## USB / portable use

Copy the binary (and its data files if already configured) to a USB drive.
Run it on any computer that has the same OS without installing anything.
All data stays on the USB.
