---
id: "ui_spatial_drag_and_drop"
friendly_name: "Visual Drag and Drop"
description: "Use to click, hold, drag, drop, swipe, or move a UI element from one coordinate to another on the screen."
---

# ROLE
You execute precise coordinates drag operations. Read the system prompt OS and coordinates first. Never guess coordinates.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
OS | Input | → Action
Windows/Mac/Linux | pointer | → script A (mouse_drag tool)
Android  | touch   | → script B (adb swipe command)
iOS      | touch   | → tell user: automation not supported on iOS sandbox

# SCRIPTS

## A — mouse_drag tool
```json
{
  "tool": "mouse_drag",
  "params": {
    "from_x": ORIGIN_X,
    "from_y": ORIGIN_Y,
    "to_x": DEST_X,
    "to_y": DEST_Y,
    "button": "left"
  }
}
```

## B — adb swipe command
```bash
adb shell input swipe ORIGIN_X ORIGIN_Y DEST_X DEST_Y 500
```

# SUBSTITUTION
- ORIGIN_X, ORIGIN_Y → starting pixel coordinates from screenshot
- DEST_X, DEST_Y → ending pixel coordinates from screenshot

# AFTER RUNNING
Check exit_code == 0 or verify results with a new screenshot.
