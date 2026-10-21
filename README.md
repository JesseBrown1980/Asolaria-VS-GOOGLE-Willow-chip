# Asolaria ASI OS

## Toolchain rule (operator, global — no exceptions)

**Rust 1.81 with clippy. Integer arithmetic and ternary (trits) only — never float.**

Pinned in `rust-toolchain.toml` (`channel = "1.81.0"`, `components = ["clippy", "rustfmt"]`),
declared as `rust-version = "1.81"` in every `Cargo.toml`, and enforced in CI by
`cargo clippy --all-targets -- -D warnings` plus a hard grep that fails the build on any
`f32`/`f64` in `src/` or `tests/`.

Any receipt in this repository naming a toolchain other than 1.81 records a run made outside
the rule. It is retained as history, not as the toolchain of record.

A tiny, self-contained **operating-system front-end** you build from source and run on your own machine — the same surface the Asolaria fabric runs on. Pure Rust `std`, **zero external crates**. It serves a full-screen **desktop** on `http://127.0.0.1:4600`: a wallpaper and icon grid, movable/resizable windows (snap to halves, maximize, minimize), a taskbar with a Start menu (Win key, Alt+S, or just type on the desktop), and a tray showing your local fabric's live status.

Apps, each in its own window: **Command Prompt / PowerShell / WSL Bash** (real shells, one per window, each on a Windows pseudo-console (ConPTY) rendered by a built-in VT100/xterm emulator: colours, full-screen programs like `top`/`vim`/the claude TUI, resize, 5,000-line scrollback, selection + Ctrl+Shift+C/V; the title bar says `PTY`, or `PIPE` if ConPTY was unavailable), **Claude** and **Codex** (run in a shell), **System Monitor**, **Kernel** (host8 `:5088` rows), **Recall Search** (your local HILBRA vault, proxied read-only through `/api/recall`), **Files** (read-only browser of the backend office via `/fs/list` and `/fs/read`: no `..`, no absolute paths, symlink escapes refused, text files ≤ 1 MiB), **Dashboard**, **ASI OS Screen**, **Windows View** (view-only), **Settings** (theme, wallpaper, icon size, accent) and **About**.

You become an **Asolaria fabric participant**: you mint your **own local secret key** (it never leaves your machine), run the OS surface, and — as you bring up the fabric daemons — use **Hilbra** (the shared-key HBI/HBP layer), **recall** (a local inverted-index vault), and the **8-byte-host** kernel with its stubbed rooms, exactly like a full node.

> **E = 0.** The OS launches only what *you* type or click. It fires nothing on its own.

## Quickstart

Requires the [Rust toolchain](https://rustup.rs) — and nothing else (the build never touches the network).

```sh
git clone https://github.com/JesseBrown1980/asolaria-asi-os
cd asolaria-asi-os

# Linux / macOS:
sh scripts/install.sh

# Windows (PowerShell):
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

That one command **builds** the OS, **mints your local key + seat**, and **launches** it. Then open **http://127.0.0.1:4600**.

## What you get

- **A real terminal surface.** Spawn `cmd` / PowerShell / WSL bash (or a plain shell on bare-metal Linux) — fully interactive, streamed over Server-Sent-Events, microsecond framework latency. Run `claude`, `codex`, or anything else inside them.
- **A live fabric strip.** Tiles poll your local daemons and show them up/down: the 8-byte kernel (`:5088`), recall vault (`:4796`), bus (`:4947`), dashboard (`:4949`), and your sovereignty vault.
- **Windows as a window.** One tile opens a Windows environment (Windows Sandbox if available, else the host desktop) as a window *over* Asolaria — so Asolaria can be your surface and Windows is just something you pop open. (No-op on non-Windows hosts.)
- **Your own identity.** `scripts/keygen` mints `~/.asolaria/recall.key` plus a local seat name/pid. The key is your HMAC secret for Hilbra/recall — **key-off-the-wire**: never transmitted, never committed to git (see `.gitignore`).

## Becoming a full fabric participant

The OS surface runs on its own. To light up the rest of the strip you run the fabric daemons alongside it — each is its own small process:

- **recall** — a local inverted-index vault (`:4796`). Public **level-0** search is provably PII-free; everything above level-0 requires *your* key.
- **8-byte host (host8)** — the Rust kernel (`:5088`) with content-addressed **stubbed rooms** that spin up on demand.
- **Hilbra** — the shared-key HBI/HBP transport that lets nodes cross-search with keys that stay off the wire.

The OS auto-detects whichever of these are listening and lights their tile.

**To run all of it in one command** — the OS *plus* recall *plus* the 8-byte host, under your own key — use the full-node installer (details in **[FABRIC-NODE.md](FABRIC-NODE.md)**):

```sh
sh scripts/install-full-node.sh      # Windows:  powershell -ExecutionPolicy Bypass -File scripts\install-full-node.ps1
```

The engines are already public: **[asolaria-federation-1024](https://github.com/JesseBrown1980/asolaria-federation-1024)** (recall-serve, host8-serve, kernel, agent-runtime, stubbed rooms — 17 crates) · **[Hilbra](https://github.com/JesseBrown1980/Hilbra)** (the Asolaria-internet protocol) · **[asolaria-behcs-256](https://github.com/JesseBrown1980/asolaria-behcs-256)** (the glyph substrate). The full-node installer clones + builds them for you; your corpus starts empty and is never published.

## Unification Crates (12.5B Room Memory + Native PC/Phone Station Dock)

This repository also includes the portable Rust 1.81 (`float_used=0`, `json=0` hot path, balanced-ternary `{-, 0, +}`) crates that unify the post-Windows ASI OS, Host8-on-metal, pre-Windows standalone UEFI boot path, and USB-docked station federation:

- **`host8-native-mapping/`** — Zero-dependency Win32 FFI (`CreateFileMappingW` / `MapViewOfFile`) read-only identity-plane mapping for the 100 GB / `12,500,000,000` 8-byte Host8 room store (`125` sector files × `800,000,000` bytes).
- **`shared-runtime/` (`asolaria-unified-core`)** — `#![no_std]` + optional `std` unified HBP command core (`status`, `memory`, `omega`, `roomaddr`, `room`, `sha`, 27-lane NTT `prism`, and 2-forward Q24 integer `gnn` over `aether/model/l0-edge` + `aether/model/l4-gslgnn`), reusing the canonical `tribit` module (`canonical/asolaria-os/kernel/core/src/tribit/mod.rs`) and NTFS extent map (`HOST8-PLACEMENT.hbp`).
- **`dock-runtime/` (`asolaria-native-dock`)** — Loopback-only HBP station federation over explicit ADB USB forward/reverse tunnels (`ACER` PC station on `127.0.0.1:5090` ↔ `FALCON` ARM64 phone station on `127.0.0.1:5091`), preserving each station's local identity and address space while providing authenticated read-only room RPC (`/peer/room.hbp?n=...`) and pure shared-core compute RPC (`/peer/task.hbp?cmd=...`).
- **`receipts/`** — Sealed HBP + `.hbi` + `.sha256` verification receipts (`MAPPED-ACTIVATION-20261021-RESULT.hbp`, `PHONE-DOCK-AUDIT-20261021.hbp`, `DOCK-LIVE-20261021.hbp`, `GITHUB-3D-VIEW.hbp`), alongside [ROOM-MEMORY-INTEGRATION.md](ROOM-MEMORY-INTEGRATION.md) and [WINDOWS-MEMORY-ASSESSMENT.md](WINDOWS-MEMORY-ASSESSMENT.md).

## Autostart (optional)

`deploy/` ships a **systemd** unit, a **.desktop** autostart entry, and a full-screen launcher (`deploy/start-asolaria-asi-os.sh`) for a bare Asolaria-on-metal Linux install. Point the paths at your install and enable them to have the OS come up full-screen at boot.

## Honest boundary

- **Runnable now:** the OS front-end in this repo — build it, run it, use it. That part is real and complete.
- **Yours to run:** the fabric daemons (recall / host8 / Hilbra) are separate processes; the OS *shows* them when they're up. This repo ships the surface + your identity, not those binaries.
- **Frontier:** the larger Asolaria ideas (the universe/cosmology framing) are documented, tagged, and honestly bounded in the fabric repos — held as vision, not asserted as physical fact.

## Requirements

- Rust ≥ 1.81 (`rustup`). Zero other dependencies — `cargo build` works fully offline.
- Linux, macOS, or Windows. Cross-compiles to a Linux target for bare-metal Asolaria-on-metal.

## License

MIT OR Apache-2.0 — do what you want; build your own node.
