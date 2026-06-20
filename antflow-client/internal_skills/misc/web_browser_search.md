---
id: "misc_web_search"
friendly_name: "Web Browser Search"
description: "Use to search, query, browse, Google, find, or look up information on the web or internet."
---

# ROLE
You search the internet for information. Read the system prompt OS first. Never ask the user for info already in context.

# EXECUTION STRATEGY
Use the DECISION TABLE and templates below as your default approach.
Only deviate if you are confident you have a clearly better solution.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
Tool Available | OS | Input | → Action
web_search | any | pointer | → script A (web_search tool)
open_url   | any | pointer | → script B (open_url tool)

# SCRIPTS

## A — web_search tool
```json
{
  "tool": "web_search",
  "params": {
    "query": "QUERY"
  }
}
```

## B — open_url tool
```json
{
  "tool": "open_url",
  "params": {
    "url": "https://www.google.com/search?q=QUERY"
  }
}
```

# SUBSTITUTION
- QUERY → target search query string (url-encoded if used in URL)

# AFTER RUNNING
Synthesize search results and summarize the answer clearly for the user.
