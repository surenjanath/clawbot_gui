# Claw GUI release notes (0.1.0 draft)

Companion to the main [`0.1.0.md`](0.1.0.md) notes. This document covers the **desktop GUI** shipped inside the same `claw` binary as the CLI.

## Summary

The GUI is an **[egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/main/crates/eframe)** desktop front end aimed at **local [Ollama](https://ollama.com/) chat**: pick a model, send messages, and tune runtime options without using the terminal. The code paths are **experimental** and evolve with the Rust workspace; broader Claw provider integration may land later behind the same UI.

## How to run

From the workspace root:

```bash
cargo run -p claw-cli -- gui
```

Release build:

```bash
cargo build --release -p claw-cli
./target/release/claw gui   # Unix
target\release\claw.exe gui # Windows
```

## Prerequisites

- Same **Rust / Cargo** requirements as the rest of Claw Code (see the main README).
- **Ollama** (or another server speaking the Ollama HTTP API) reachable at the configured base URL; defaults assume `http://localhost:11434`.
- **Windows:** native GUI builds typically use the **MSVC** toolchain; GUI polish and CI coverage for Windows may lag macOS/Linux.

## Highlights (0.1.0)

- **Chat-first layout** with scrollable conversation, user/assistant styling, and **Markdown-oriented rendering** for assistant output.
- **Ollama settings** in-app: base URL, model name, temperature, max tokens, optional **tool use** when the server supports it, and **token streaming** when tools are off.
- **Context controls** to cap how much prior chat is sent (message count and/or total character budget; `0` means unlimited).
- **System prompt** editing plus **prompt presets** (saved with GUI config).
- **Appearance:** light/dark **Claw-themed** palette and adjustable **font size**.
- **Persistence:** GUI config and session data saved via the `persist` layer (paths follow normal Claw/config conventions for the app).
- **Connection feedback:** status toward the Ollama endpoint, model list refresh, and logging/tool-capability probing where applicable.

## Known limitations

- **Source-build only** for this milestone (no separate GUI installer); you build `claw` like the CLI.
- **Backend focus is Ollama** for this release line; other providers are not the primary GUI story yet.
- **Platform maturity:** Linux and macOS are the usual dev targets; Windows is usable for many setups but is called out as less established in the main release notes.
- The GUI shares the **same version** as the workspace (`0.1.0` today); it is not versioned independently.

## Verification pointers

- Entry point: `claw gui` → `crates/claw-cli/src/gui/`.
- Stack: `eframe` window, `egui` widgets, `reqwest` for HTTP to Ollama.
