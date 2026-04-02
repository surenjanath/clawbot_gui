# Claw GUI development notes

## Toolchain and MSRV

- The workspace does not pin a compiler by default. To align everyone on the same `rustc`, add a `rust-toolchain.toml` next to the workspace `Cargo.toml` (for example `channel = "1.85"` or `channel = "stable"`). CI and local builds then use `rustup` to install that channel automatically when the file is present.
- You can also set `rust-version` in `[workspace.package]` or per-crate `[package]` so Cargo warns if the active toolchain is older than the declared minimum supported Rust version (MSRV). Pick the MSRV by testing the oldest `rustc` that still builds `cargo build --workspace` successfully.

## `rustup update` vs `cargo update`

- **`rustup update`** changes the installed compiler. Stable Rust stays backward compatible for most code; breakage usually comes from new warnings treated as errors in CI, dependencies raising their MSRV, or rare soundness fixes. Rebuilding produces a new binary; already-shipped binaries are unchanged.
- **`cargo update`** refreshes dependency versions inside `Cargo.lock`. That is more likely to change behavior than a compiler bump. Prefer doing it on a branch, run `cargo test` (at least `claw-cli` and `api` for GUI work), then commit the lockfile when satisfied.

## Backing out changes

- **Committed work** is recoverable with `git revert`, `git reset`, or `git reflog` until garbage collection; you do not lose history by moving branches.
- **Uncommitted work** can be lost on hard resets or bad merges. Stash or commit before large experiments, `rustup update`, or broad `cargo update`.
- If the project lives in a synced folder (for example OneDrive), treat Git as the source of truth for code; resolve sync conflicts separately from Git history.

## GUI config and data paths

- GUI settings and chat sessions live under `CLAW_CONFIG_HOME/gui` when `CLAW_CONFIG_HOME` is set; otherwise the code uses the same default config root as the rest of Claw (see `gui/persist.rs` — typically `~/.claw/gui` on Unix and an appropriate data directory on Windows via the `dirs` crate).

## Run the GUI

From the Rust workspace root:

```bash
cargo run -p claw-cli -- gui
```

The binary also accepts `claw gui` if `target/debug/claw` (or `release`) is on your `PATH`.

## Windows stack (`STATUS_STACK_OVERFLOW`)

The `claw` binary is large; on Windows the default PE **main-thread stack reserve** is often only ~1 MiB, which can overflow before egui/winit’s first frame.

**`crates/claw-cli/build.rs`** emits **`cargo:rustc-link-arg-bins=/STACK:134217728`** (128 MiB) for MSVC (and the GNU equivalent) so the **linked `claw.exe`** gets a large reserve. This only takes effect when the binary is **relinked** — after changing `build.rs` or if the linker skipped the flag, run:

```bash
cargo clean -p claw-cli
cargo build -p claw-cli
```

Do **not** run `eframe` on a background thread on Windows: winit requires the main thread (or explicit `any_thread` APIs) and still overflowed in testing.

The GUI also **caches** the last applied theme/font size so `ctx.style_mut` does not run every frame.
