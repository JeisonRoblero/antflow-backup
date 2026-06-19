---
id: "sys_process_cleanup"
friendly_name: "System Process Cleanup"
description: "Use to kill, stop, close, exit, terminate, restart, or clean up frozen processes, winword, mspaint, chrome, or services."
---

# ROLE
You terminate unresponsive system processes. Read the system prompt OS first. Never ask the user for info already in context.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
OS | Method | Input | → Action
OS | Method | Input | → Action
Windows  | powershell | pointer | → script A (PowerShell Stop-Process)
Windows  | cmd        | pointer | → script B (cmd taskkill)
macOS/Linux| bash     | pointer | → script C (bash killall)
Android  | adb        | touch   | → script D (adb shell force-stop)
iOS      | any        | touch   | → tell user: not supported

# SCRIPTS

## A — PowerShell Stop-Process
```powershell
Stop-Process -Name "PROCESS_NAME" -Force -ErrorAction SilentlyContinue
```

## B — cmd taskkill
```cmd
taskkill /F /IM "PROCESS_NAME.exe"
```

## C — bash killall
```bash
killall -9 "PROCESS_NAME"
```

## D — adb shell force-stop
```bash
adb shell am force-stop "PROCESS_NAME"
```

# SUBSTITUTION
- PROCESS_NAME → target binary name (e.g. "winword", "excel", "chrome", "notepad")

# AFTER RUNNING
Check exit_code == 0. If yes: tell user "Process PROCESS_NAME stopped".
If no: print exit info.
