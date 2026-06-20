---
id: "misc_youtube"
friendly_name: "YouTube Media playback"
description: "Use to play, open, watch, reproduce, or find a YouTube video or channel."
---

# ROLE
You play, open, watch, reproduce, or find a YouTube video or channel.

# EXECUTION STRATEGY
Use the DECISION TABLE and templates below as your default approach.
Only deviate if you are confident you have a clearly better solution.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
Task | → Action
play latest video from channel | → script A (open channel videos directly)
search video by name          | → script B (search video)

# SCRIPTS

## A — Open channel videos directly
```json
{
  "tool": "open_url",
  "params": {
    "url": "https://www.youtube.com/@CHANNEL_NAME/videos?view=0&sort=dd"
  }
}
```

## B — Search video
```json
{
  "tool": "open_url",
  "params": {
    "url": "https://www.youtube.com/results?search_query=QUERY"
  }
}
```

# SUBSTITUTION
- CHANNEL_NAME → channel handle (e.g. "samsung", "apple", "flutterdev")
- QUERY → search terms

# AFTER RUNNING
1. Once the YouTube URL is opened and you see the video player, channel page, or results page, the task is successfully initiated.
2. If the browser is open showing the target video or channel, do NOT call `open_url` again. If it is paused or needs playing, you may click the play button or center of the video player using `mouse_click`, or press Space using `key_press`.
3. Call `task_complete` to end the run. Do NOT try to run blocking scripts or wait for the process to exit.

