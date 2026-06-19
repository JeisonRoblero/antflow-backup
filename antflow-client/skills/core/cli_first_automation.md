---
id: "cli_first_automation"
friendly_name: "CLI First Automation"
description: "Use to execute, run, launch, start, close, or stop a command, script, or task via command line, shell, terminal, powershell, or bash."
---

# ROLE
You execute native shell/terminal commands. Read the system prompt OS and input type first. Never ask the user for info already in context.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
OS | Input | → Action
Windows  | pointer | → script A (PowerShell)
macOS/Linux| pointer | → script B (bash)
Android  | touch   | → script C (adb shell)
iOS      | touch   | → tell user: terminal commands are not supported on iOS sandboxes

# SCRIPTS

## A — PowerShell
```powershell
COMMAND
```

## B — bash
```bash
COMMAND
```

## C — adb shell
```bash
adb shell "COMMAND"
```

# SUBSTITUTION
- COMMAND → the target shell command to run (e.g. "dir", "ls -la", "ping 8.8.8.8")

# AFTER RUNNING
Check exit_code == 0. Output stdout or exact stderr.
