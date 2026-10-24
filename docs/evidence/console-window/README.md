# Child-process console windows — evidence

Branch: `fix/no-console-window` (from `main` @ `5c0c3a0`) · Captured: 2026-10-24

## Symptom

Selecting a language server in Settings (or letting a server start / restart) popped a
black console window titled with the server's full path, e.g.
`%USERPROFILE%\.cargo\bin\rust-analyzer.exe`, on top of the IDE. Closing that window kills
the server.

## Root cause

The IDE is a GUI-subsystem executable in release builds
([`src-tauri/src/main.rs`:2](../../../src-tauri/src/main.rs#L2)), so it has **no console**.
Windows gives every console-subsystem child such a process starts a *new* console window
unless the spawn passes `CREATE_NO_WINDOW` (`0x0800_0000`).

`Peer::spawn` — the single spawn point for language servers — did not pass it:

- [`crates/ide-jsonrpc/src/lib.rs`:244](../../../crates/ide-jsonrpc/src/lib.rs#L244)
  `tokio::process::Command` + `.spawn()` and no creation flags.

The same gap existed in the rustup-proxy version probe
([`crates/ide-lsp/src/launch.rs`](../../../crates/ide-lsp/src/launch.rs)), the DAP adapter,
debugpy probe, debuggee and `cargo` spawns
([`crates/ide-dap/src`](../../../crates/ide-dap/src)), SDK probes
([`crates/ide-sdk/src/exec.rs`](../../../crates/ide-sdk/src/exec.rs)), run-configuration
probes ([`crates/ide-run/src/wrap.rs`](../../../crates/ide-run/src/wrap.rs)), the coverage
runner ([`crates/ide-coverage/src/lib.rs`](../../../crates/ide-coverage/src/lib.rs)) and
tool diagnostics / downloads / `tar` extraction
([`crates/ide-tools/src`](../../../crates/ide-tools/src)) — while git, cargo builds, `gh`,
`docker` and run configurations already passed the flag inline (five copies of the magic
constant). Six subsystems missed it, five remembered it: a consistency bug.

Why *every* selection triggered it: Settings writes `languages.*` or `tools.*` →
[`commands/app.rs`:152-158](../../../src-tauri/src/commands/app.rs#L152-L158) (and a tool
install finishes through [`commands/tools.rs`:84-87](../../../src-tauri/src/commands/tools.rs#L84-L87))
→ `LspManager::reconfigure()` → the affected servers are stopped and started again → a new
process → a new console window.

## Fix

New single source of truth:
[`crates/ide-core/src/process.rs`](../../../crates/ide-core/src/process.rs) —
`CREATE_NO_WINDOW` plus `hide_console_window(&mut std::process::Command)`, a no-op off
Windows. Every child spawn now goes through it (tokio commands via `as_std_mut()`), and the
inline copies were folded into it. 17 files changed, +57/−53.

## Verification

| # | Check | Command | Result |
| - | ----- | ------- | ------ |
| 1 | Target app is console-less | `file <install dir>/openidea.exe` | `PE32+ executable for MS Windows 6.00 (GUI)` — no console to inherit |
| 2 | Mechanism: plain spawn vs. flag, from a console-less parent | `mechanism-probe.rs` (this directory) | [mechanism-probe.txt](mechanism-probe.txt): plain `console window 0xd109ec, visible=1`, flagged `no console window (GetConsoleWindow = NULL)` |
| 3 | **Production code path** (`Peer::spawn`) | `cargo run -p ide-jsonrpc --example console_check` | [peer-check.txt](peer-check.txt): plain tokio spawn `console window 0x2d01ee, visible=1`, `Peer::spawn` `console without a window` |
| 4 | Type-check | `cargo check` on the 12 touched crates | passes |
| 5 | Lints | `cargo clippy --all-targets` on the 12 touched crates | 0 warnings |
| 6 | Tests | `cargo test` on ide-core, ide-jsonrpc, ide-lsp, ide-dap, ide-sdk, ide-tools, ide-build, ide-run, ide-coverage | 249 passed, 0 failed, 8 ignored (`ide-vcs` left out: its `git_workflows` test needs an MSYS `sh` this sandbox cannot create, a known environment limit) |

### Not verified here

The packaged application was not rebuilt in this session, so the *end-user* observation
(no black window after a settings-driven restart) was not re-run against a fresh build. The
installed build that showed the symptom (0.1.9) still has the old code, and its running
instance holds the single-instance lock. Rebuild and install to confirm:

```bash
pnpm tauri build          # then install the MSI/NSIS bundle
```

In `pnpm tauri dev` the black window never appeared: a debug build is a console-subsystem
process, so children inherited the terminal's console. The bug only shows in release builds
(the installed app, a packaged bundle, or `cargo run --release`).

### Reproducing the checks

1. Mechanism (OS level, no IDE code): `rustc mechanism-probe.rs -O -o mechanism-probe.exe`
   from the repository root, run it, read `target/console-probe/probe.log`.
2. Production path: `cargo run -p ide-jsonrpc --example console_check`, then read
   `target/console-probe/peer-check.log`.

Both binaries detach from the caller's console first (`FreeConsole`), so they reproduce the
release IDE's console-less environment; they detect the child's console window with
`AttachConsole` + `GetConsoleWindow` + `IsWindowVisible`.
