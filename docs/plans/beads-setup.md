# Plan: Beads Setup for icloud-dedupe

## What is Beads?

[Beads](https://github.com/steveyegge/beads) is a git-backed task tracker for AI coding agents by Steve Yegge. Tasks are stored as JSONL in `.beads/` — versioned, mergeable, persistent across sessions.

**Why use it:**
- Tasks survive Claude Code session restarts and compactions
- Git-native: branch, merge, history
- Dependency-aware DAG (not flat lists)
- Designed for AI agents, not humans with GUIs

## Installation Steps

### 1. Install beads CLI (`bd`)

```bash
# Option A: Homebrew
brew tap steveyegge/beads
brew install beads

# Option B: Curl script
curl -fsSL https://raw.githubusercontent.com/steveyegge/beads/main/scripts/install.sh | bash
```

Verify:
```bash
bd --version
```

### 2. Install beads-mcp (MCP server for Claude)

```bash
pip install beads-mcp
```

### 3. Initialize beads in the repo

```bash
cd ~/DATA_PROG/RUST/icloud-dedupe
bd init
```

This creates `.beads/` directory with `issues.jsonl`.

### 4. Set up Claude Code integration

```bash
bd setup claude
```

This installs:
- **SessionStart hook**: runs `bd prime` at session start, injects context
- **PreCompact hook**: ensures beads state is saved before compaction

The `bd prime` output explicitly tells Claude: "Track ALL work in beads (no TodoWrite tool, no markdown TODOs)"

### 5. Install viewer (optional but recommended)

```bash
# beads_viewer (bv) - most popular TUI viewer
brew tap Dicklesworthstone/beads_viewer
brew install beads_viewer

# Or for web UI
npx beads-ui start
```

## Post-Setup Verification

```bash
# Check beads is initialized
bd status

# Check Claude hooks are installed
cat ~/.claude/hooks.json | jq '.SessionStart, .PreCompact'

# Create a test issue
bd add "Test issue" --type task

# List issues
bd list

# View in TUI
bv
```

## Workflow with Beads

### Creating issues
```bash
bd add "Implement pattern detection" --type task
bd add "Add unit tests for patterns" --type task --blocked-by <id>
```

### During Claude Code session
- Claude sees beads context at session start via `bd prime`
- Claude should use `bd` commands instead of TodoWrite/TaskCreate
- Issues persist across compactions and session restarts

### Viewing progress
```bash
bd list              # all issues
bd ready             # what can be worked on now (unblocked)
bd show <id>         # details of one issue
bv                   # TUI viewer
```

## Files Created

```
.beads/
  issues.jsonl       # the task database (commit this!)
  .gitattributes     # merge driver config
```

## Gotchas

1. **Commit `.beads/`** — it's the whole point
2. **Don't mix with TodoWrite** — beads replaces it for this project
3. **Use `bd ready`** — shows unblocked tasks, respects dependencies
4. **Run `bd prime`** — if Claude seems to forget about beads mid-session

## References

- [Main repo](https://github.com/steveyegge/beads)
- [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer)
- [Claude plugin docs](https://deepwiki.com/steveyegge/beads/9.2-claude-plugin)
- [Installation guide](https://github.com/steveyegge/beads/blob/main/docs/INSTALLING.md)
