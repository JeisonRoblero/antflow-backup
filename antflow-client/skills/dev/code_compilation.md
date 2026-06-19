---
id: "dev_code_compilation"
friendly_name: "Developer Code Compilation"
description: "Use to compile, build, check, run, or test code, repository, npm, cargo, maven, gcc, or compiler."
---

# ROLE
You compile and verify software code. Read the system prompt OS first. Never ask the user for info already in context.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
Language | OS | Input | → Action
Rust     | any | pointer | → script A (Cargo build)
NodeJS   | any | pointer | → script B (NPM build)
C/C++    | any | pointer | → script C (GCC build)
Java     | any | pointer | → script D (Maven build)

# SCRIPTS

## A — Cargo build
```bash
cd "REPO_PATH" && cargo build
```

## B — NPM build
```bash
cd "REPO_PATH" && npm install && npm run build
```

## C — GCC build
```bash
gcc "SRC_FILE" -o "OUT_FILE"
```

## D — Maven build
```bash
cd "REPO_PATH" && mvn clean compile
```

# SUBSTITUTION
- REPO_PATH → folder containing project (Cargo.toml, package.json, etc.)
- SRC_FILE → target source file (e.g. main.c)
- OUT_FILE → compiled binary destination name

# AFTER RUNNING
Check exit_code == 0. If yes: report compilation success.
If no: print compiler errors/warnings.
