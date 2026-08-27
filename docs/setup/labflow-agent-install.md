# Installing the LabFlow Agent Interface

This guide is for people who want to drive the LabFlow workspace from a
local Agent (WorkBuddy, Codex CLI, or any MCP-compatible client) without
re-running the project installer.

The Agent Interface is a thin MCP adapter over the same Rust domain
services that the LabFlow Desktop UI uses. There are three pieces to
install:

1. The release binary `labflow-mcp` (stdio JSON-RPC over MCP).
2. The umbrella skill `labflow-agent` (architecture invariants, error
   contract, bootstrap rules) and the module skill `labflow-calendar`
   (Task workflows).
3. An entry in your MCP client's server list pointing at the binary.

## 0. Prerequisites

- A working LabFlow Desktop install (so the canonical user-data directory
  `~/Library/Application Support/LabFlow/` exists). The MCP server uses
  the same canonical path the Desktop app uses.
- Node ≥ 18 (the build script shells out to `npm` for the Rust build).
- Rust 1.98.0 (`rustup install 1.98.0`).

## 1. Quick path: one-shot installer

From the LabFlow repository root:

```bash
./scripts/install-labflow-mcp.sh
```

This script is idempotent and handles the rest of the guide for you on
macOS. It:

- Builds `eln-app/src-tauri/target/release/labflow-mcp`.
- Copies `.agents/skills/labflow-agent/SKILL.md` →
  `~/.workbuddy/skills/labflow-agent-contract/SKILL.md`.
- Copies `.agents/skills/labflow-calendar/SKILL.md` →
  `~/.workbuddy/skills/labflow-calendar/SKILL.md`.
- Inserts the `labflow` entry into `~/.workbuddy/mcp.json` (creates the
  file if it doesn't exist).
- If `codex` is on `PATH`, registers the server with
  `codex mcp add labflow -- <binary>`.
- Spawns the binary, runs `initialize`, lists tools, and lists
  Experiments — a full round-trip smoke test.

Re-run any time after pulling changes to the source tree.

## 2. Manual path (or non-Mac)

### 2.1 Build the binary

```bash
cd eln-app
npm install
npm run mcp:build
```

The artifact is `eln-app/src-tauri/target/release/labflow-mcp` (macOS) or
the equivalent platform-specific path. The binary uses `dirs::data_dir()`,
so a Linux build writes to `~/.local/share/LabFlow/` and a Windows build
writes to `%APPDATA%\Roaming\LabFlow\`. Cross-compile via
`cargo build --release --target x86_64-unknown-linux-gnu` (or the matching
target) after the appropriate target is added with `rustup target add`.

### 2.2 Install the skills

Codex discovers skills under `.agents/skills/` in the repository it is
working in, so for Codex users a clone of the repo is enough.

For WorkBuddy, copy the umbrella skill into the user-level skills
directory:

```bash
mkdir -p ~/.workbuddy/skills/labflow-agent-contract
cp .agents/skills/labflow-agent/SKILL.md \
   ~/.workbuddy/skills/labflow-agent-contract/SKILL.md
mkdir -p ~/.workbuddy/skills/labflow-calendar
cp .agents/skills/labflow-calendar/SKILL.md \
   ~/.workbuddy/skills/labflow-calendar/SKILL.md
```

These are runtime copies; re-copy after skill updates.

### 2.3 Register the MCP server

**WorkBuddy** — edit `~/.workbuddy/mcp.json` (NOT `~/.workbuddy/.mcp.json`)
so it contains:

```json
{
  "mcpServers": {
    "labflow": {
      "command": "/absolute/path/to/labflow-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Restart WorkBuddy so it picks up the new entry. WorkBuddy discovers
`labflow_*` tools as soon as the server answers `initialize` and `tools/list`.

**Codex CLI** — register the server in your Codex config:

```bash
codex mcp add labflow -- /absolute/path/to/labflow-mcp
codex mcp get labflow
```

The umbrella skill loads automatically when Codex is opened in a clone of
this repository.

**Other MCP clients** — point the client at the same binary path. The
transport is stdio JSON-RPC, no extra configuration required.

## 3. Verifying the install

Run the smoke test directly:

```bash
ls -la ~/.workbuddy/mcp.json
ls -la ~/.workbuddy/skills/labflow-agent-contract/SKILL.md
ls -la ~/.workbuddy/skills/labflow-calendar/SKILL.md
which labflow-mcp || ls /path/to/labflow-mcp
```

Or just ask WorkBuddy / Codex "list LabFlow Experiments". A successful
run looks like:

```
labflow_list_experiments
→ [{ "id": "...", "code": "...", "title": "..." }, ...]
```

## 4. Updating after a pull

After pulling new changes that touch the MCP surface or the skills:

```bash
git pull
./scripts/install-labflow-mcp.sh    # idempotent
# restart WorkBuddy / refresh Codex
```

The umbrella skill explicitly tells the agent to call
`labflow_list_experiments` before any Task write, and to confirm Protocol
input sample types before calling `labflow_create_protocol`. If you are
forking the skill, keep that section intact — every regression we have
seen traces back to skipping one of those two rules.

## 5. Cross-platform notes

- **macOS** — fully supported by the bundled build.
- **Linux** — build with `cargo build --release --target
  x86_64-unknown-linux-gnu`; the canonical data directory is
  `~/.local/share/LabFlow/`.
- **Windows** — `cargo build --release --target
  x86_64-pc-windows-msvc`; canonical data directory is
  `%APPDATA%\Roaming\LabFlow\`. The install script auto-targets the host
  OS via `cargo`, so on Windows it produces `labflow-mcp.exe` directly.