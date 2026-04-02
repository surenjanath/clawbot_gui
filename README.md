# Claw Code

Claw Code is a local coding-agent CLI implemented in safe Rust. It is **Claude Code inspired** and developed as a **clean-room implementation**: it aims for a strong local agent experience, but it is **not** a direct port or copy of Claude Code.

The Rust workspace is the current main product surface. The `claw` binary provides interactive sessions, one-shot prompts, workspace-aware tools, local agent workflows, and plugin-capable operation from a single workspace.

## About this copy

This tree comes from the original open-source project **[instructkr/claw-code](https://github.com/instructkr/claw-code)**. What you see here is a **personal play / sandbox** extract focused on the Rust workspace and GUI—not an official fork announcement or a replacement for upstream. Day-to-day edits were done with **[Cursor](https://cursor.com)** and other usual dev tools (Rust toolchain, Git, and whatever else was handy); it is experimental tinkering on top of someone else’s excellent foundation.

**This sandbox is published at [github.com/surenjanath/clawbot_gui](https://github.com/surenjanath/clawbot_gui).** For the canonical upstream project, history, and community, use [instructkr/claw-code](https://github.com/instructkr/claw-code).

### Git: `origin` for this repo only

If [clawbot_gui](https://github.com/surenjanath/clawbot_gui) is an **empty** GitHub repo and you want **only** this `rust/` tree in it (not the parent monorepo), run from **inside** this folder:

```bash
git init
git remote add origin https://github.com/surenjanath/clawbot_gui.git
git add .
git commit -m "Initial commit"
git branch -M main
git push -u origin main
```

If you already have a Git repo above this folder with its own `origin`, either use a **subfolder** copy of `rust/` for the steps above, or add a **second** remote there (e.g. `git remote add clawbot_gui https://github.com/surenjanath/clawbot_gui.git`) and push only what you intend (subtree or filtered history), so you do not overwrite `origin` by accident.

## Current status

- **Version:** `0.1.0`
- **Release stage:** initial public release, source-build distribution
- **Primary implementation:** Rust workspace in this repository
- **Platform focus:** macOS and Linux developer workstations

## Install, build, and run

### Prerequisites

- Rust stable toolchain
- Cargo
- Provider credentials for the model you want to use

### Authentication

Anthropic-compatible models:

```bash
export ANTHROPIC_API_KEY="..."
# Optional when using a compatible endpoint
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
```

Grok models:

```bash
export XAI_API_KEY="..."
# Optional when using a compatible endpoint
export XAI_BASE_URL="https://api.x.ai"
```

OAuth login is also available:

```bash
cargo run --bin claw -- login
```

### Install locally

```bash
cargo install --path crates/claw-cli --locked
```

### Build from source

```bash
cargo build --release -p claw-cli
```

### Run

From the workspace:

```bash
cargo run --bin claw -- --help
cargo run --bin claw --
cargo run --bin claw -- prompt "summarize this workspace"
cargo run --bin claw -- --model sonnet "review the latest changes"
```

From the release build:

```bash
./target/release/claw
./target/release/claw prompt "explain crates/runtime"
```

## Supported capabilities

- Interactive REPL and one-shot prompt execution
- Saved-session inspection and resume flows
- Built-in workspace tools for shell, file read/write/edit, search, web fetch/search, todos, and notebook updates
- Slash commands for status, compaction, config inspection, diff, export, session management, and version reporting
- Local agent and skill discovery with `claw agents` and `claw skills`
- Plugin discovery and management through the CLI and slash-command surfaces
- OAuth login/logout plus model/provider selection from the command line
- Workspace-aware instruction/config loading (`CLAW.md`, config files, permissions, plugin settings)

## Current limitations

- Public distribution is **source-build only** today; this workspace is not set up for crates.io publishing
- GitHub CI verifies `cargo check`, `cargo test`, and release builds, but automated release packaging is not yet present
- Current CI targets Ubuntu and macOS; Windows release readiness is still to be established
- Some live-provider integration coverage is opt-in because it requires external credentials and network access
- The command surface may continue to evolve during the `0.x` series

## Implementation

The Rust workspace is the active product implementation. It currently includes these crates:

- `claw-cli` — user-facing binary
- `api` — provider clients and streaming
- `runtime` — sessions, config, permissions, prompts, and runtime loop
- `tools` — built-in tool implementations
- `commands` — slash-command registry and handlers
- `plugins` — plugin discovery, registry, and lifecycle support
- `lsp` — language-server protocol support types and process helpers
- `server` and `compat-harness` — supporting services and compatibility tooling

## Roadmap

- Publish packaged release artifacts for public installs
- Add a repeatable release workflow and longer-lived changelog discipline
- Expand platform verification beyond the current CI matrix
- Add more task-focused examples and operator documentation
- Continue tightening feature coverage and UX polish across the Rust implementation

## Release notes

- Draft 0.1.0 release notes: [`docs/releases/0.1.0.md`](docs/releases/0.1.0.md)
- GUI (0.1.0 draft): [`docs/releases/gui.md`](docs/releases/gui.md)

## License

See the repository root for licensing details.
