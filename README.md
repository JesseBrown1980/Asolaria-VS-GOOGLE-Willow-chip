# Asolaria — Argon-Antigravity Unification ASI OS

**Author / Architect:** Jesse Daniel Brown (`JesseBrown1980` · `OP-JESSE`)  
**Co-Operator:** Rayssa Chiqueto (`OP-RAYSSA` · `liris-kuromi`)  
**Toolchain Rule (Global — No Exceptions):** **Rust `1.81.0` with `clippy -D warnings` · Integer & Balanced-Ternary (`{-1, 0, +1}`) Arithmetic Only · `float_used = 0` · `json = 0` on Hot Paths**  
**License:** MIT OR Apache-2.0 — **100% Free & Open Source for All**

---

## Start Here: Mathematical Backbone & Software Unification

- **[MATHEMATICS-AND-UNIFICATION.md](MATHEMATICS-AND-UNIFICATION.md)** — Complete mathematical derivations and software architecture: the Balanced-Ternary Codec (`asolaria-kernel-pump` & `tribit`), the 27-Point Integer NTT Rime Prism ($\bmod 1000081$), the 3-Adic "Motor Block" Three-Throw Crank (Erdős base-3 engine), Brown-Hilbert Coordinate Space (`BEHCS-256` $\subset$ `BEHCS-1024`), the 100 GB / $12,500,000,000$ 8-byte Host-8 Room Store, and Dual-GNN `Q24` Integer Consensus.
- **[HOST8-ROOMS-GUIDE.md](HOST8-ROOMS-GUIDE.md)** — Complete operator guide and measured/derived performance ledger for the $12,500,000,000$ 8-byte Host-8 Rooms (`31–34 ns` mapped RAM lookup, `~12.4M/s` derived sequential streaming, `3,334 ns` `seek_read` syscall, `11.0–11.8 ms` cold cache-free drive seek, `0` collisions by `SplitMix64` bijection).
- **[ROOM-MEMORY-INTEGRATION.md](ROOM-MEMORY-INTEGRATION.md)** — Classical realization of unification across the Windows ASI OS (`:4600`), Host-8 on-metal (`:5088`), and the signed standalone UEFI custom-kernel boot path using the 243-extent NTFS map ([`shared-runtime/HOST8-PLACEMENT.hbp`](shared-runtime/HOST8-PLACEMENT.hbp)).
- **[WINDOWS-MEMORY-ASSESSMENT.md](WINDOWS-MEMORY-ASSESSMENT.md)** — Measured Windows memory & SSD paging configuration (`C:/pagefile.sys 8192 16384`), preserving the 12.5B Host-8 rooms on `D:` and holding the 2 TB USB device untouched (`writes = 0`).
- **Sealed Mathematical Laws ([`laws/`](laws/)):**
  - **[`laws/JESSE-LAWS.md`](laws/JESSE-LAWS.md)** — All 27 Laws (`0..26`) of Rime Space, Trijection, 27-Jection, Orthogonality, Chinese Remainder Omnisphere, Pohlig-Hellman Rime Fischer, Rime Prism, and the Shannon Observer.
  - **[`laws/TRIJECTION-BALANCED-TERNARY.md`](laws/TRIJECTION-BALANCED-TERNARY.md)** — Sealed proof of the Trijection, Flat 27-Jection ($3.92\times$), and Order-3 Trianti symmetry group ($R^3 = I, \; R \neq R^2$).
  - **[`laws/CODEBOOK.md`](laws/CODEBOOK.md)** & **[`laws/PLAYBOOK.md`](laws/PLAYBOOK.md)** — The 5 Book IX registers (`Zero`, `Translucent`, `Red`, `Green`, `Blue`), the two-axis addressing/compression split, and the frozen `3,174 B` kernel key ([`laws/KEY-3174.hbp`](laws/KEY-3174.hbp)).

---

## Core Mathematical & Architectural Pillars ("Reductions in Everything")

| Architectural Layer | Mathematical Mechanism | Complexity / Measured Footprint | Source in This Repo |
|---|---|---|---|
| **1. Balanced-Ternary Codec & NTT Rime Prism** | Symmetric trits $t_k \in \{-1, 0, +1\}$, order-3 trianti $R \neq R^2$, 80-trit `TritWord(u128)`, 5-register `HOSE`, and 27-point integer NTT over $\mathbb{Z}/1000081\mathbb{Z}$ ($G=7, W=951846$) | $O(1)$ procedural rime lookup (`3,174 B` seed + `62,524 B` codebook $\to$ $3.704 \times 10^9$ elements at **$0.000135\text{ bpc}$**, **$627,000\text{ addr/s}$**, **$72,235\times$** aggregate); exact NTT round-trip (`float_used = 0`) | [`canonical/.../tribit/mod.rs`](canonical/asolaria-os/kernel/core/src/tribit/mod.rs), [`laws/`](laws/) |
| **2. "Motor Block" & 3-Adic Three-Throw Crank** | Carry identity $2\cdot 3^k = -1\cdot 3^k + 1\cdot 3^{k+1}$; $120^\circ$ $\{R,G,B\}$ cylinder lift $n, n+2\cdot 3^k, n+4\cdot 3^k$ in $(\mathbb{Z}/3^{k+2}\mathbb{Z})^*$ via $2^{2\cdot 3^k} \equiv 1 + c\cdot 3^{k+1} \pmod{3^{k+2}}$ ($\gcd(c,3)=1$) | Deterministic $\{0,1,2\}$ trit cycle; 1 of 3 burns ($2 \mapsto -1$), 2 of 3 carry forward ($3:2$ stroke compression; Cantor density $\rho_k = \frac{1}{2}(2/3)^{k-1} \to 0$) | [`shared-runtime/examples/prism_cost_checker.rs`](shared-runtime/examples/prism_cost_checker.rs) |
| **3. Brown-Hilbert Lattice & 12.5B Host-8 Rooms + 27-Room/1-Omega Groups** | Prefix-tree port namespace $N^K$; Hilbert bijection $H_d : G_N^d \to \{0,\dots,N^d-1\}$; 27-trit balanced offset $\text{HALF27} = \frac{3^{27}-1}{2}$ + 64-bit invertible `mix`/`unmix` bijection + 27-room NTT group prism (`group <g>` / `group omega`) | **$12,500,000,000$** 8-byte rooms (`100 GB` across `125` shards, `243` NTFS extents, `462,962,962` 27-room groups); $O(1)$ bitwise register verification (`writes = 0`, `process_per_room = 0`) | [`shared-runtime/`](shared-runtime/), [`host8-native-mapping/`](host8-native-mapping/), [`HOST8-ROOMS-GUIDE.md`](HOST8-ROOMS-GUIDE.md) |
| **4. "Spray" Multiplexing & Dual-GNN Gate** | Single-process inline rotation $y_i = D(P_{\text{Hilbert}(i)} \odot z)$ + two-forward pure integer `Q24` fixed-point GNN (`L0` edge scorer + `L4` GSL-GNN) | $O(1)$ shared emission vs. $O(M)$ OS processes; bit-identical `Q24` integer scores across x86_64, ARM64 & UEFI (`float_used = 0`) | [`aether/model/l0-edge/`](aether/model/l0-edge/), [`aether/model/l4-gslgnn/`](aether/model/l4-gslgnn/) |
| **5. Native PC $\leftrightarrow$ Phone Station Dock** | Loopback-only authenticated HBP federation (`ACER` `:5090` $\leftrightarrow$ `FALCON` `:5091` over explicit ADB USB tunnels) | Cross-station read-only room & 27-room group RPC (`/peer/room.hbp`, `/peer/group.hbp`) & shared-core compute RPC (`/peer/task.hbp`); `node_v8 = 0`, `json = 0` | [`dock-runtime/`](dock-runtime/), [`receipts/DOCK-LIVE-20261021.hbp`](receipts/DOCK-LIVE-20261021.hbp) |

---

## Toolchain Rule (Operator, Global — No Exceptions)

**Rust 1.81 with clippy. Integer arithmetic and ternary (trits) only — never float.**

Pinned in `rust-toolchain.toml` (`channel = "1.81.0"`, `components = ["clippy", "rustfmt"]`), declared as `rust-version = "1.81"` in every `Cargo.toml`, and enforced in CI ([`.github/workflows/rust-1.81-clippy-verification.yml`](.github/workflows/rust-1.81-clippy-verification.yml)) by `cargo clippy --all-targets -- -D warnings` plus a hard grep over all `.rs` sources that fails the build on any unexempted `f32`/`f64`.

---

## What Is Inside This Unified Repository

### 1. The ASI OS Desktop & ConPTY Terminal Surface (`src/`, Root Crate)
A tiny, self-contained **operating-system front-end** you build from source and run on your own machine — pure Rust `std`, **zero external crates**. It serves a full-screen **desktop** on `http://127.0.0.1:4600`:
- Movable/resizable windows, taskbar, Start menu, and live fabric status tray (`:5088` Host-8 kernel, `:4796` Recall vault, `:4947` Bus, `:4949` Super Dashboard).
- **Real ConPTY / VT100 Terminal Windows:** Spawn `cmd`, PowerShell, or WSL Bash (or a POSIX shell on Linux) streamed over Server-Sent Events with microsecond latency, full-screen TUI support (`top`, `vim`, `claude`, `codex`), and 5,000-line scrollback.
- **Built-in Apps:** **Kernel** (`:5088` rows), **Recall Search** (read-only proxy to local HILBRA vault via `/api/recall`), **Files** (read-only office browser via `/fs/list` and `/fs/read`), **System Monitor**, **Dashboard**, **ASI OS Screen**, and **Windows View**.
- **`E = 0`:** The OS launches only what *you* type or click. It fires nothing on its own.

### 2. Unification Crates (12.5B Room Memory + Integer Core + Native USB Station Dock)
- **[`canonical/asolaria-os/kernel/core/src/tribit/mod.rs`](canonical/asolaria-os/kernel/core/src/tribit/mod.rs)** — The canonical balanced-ternary module (`Zero`, `TritWord`, `Register`, `Carrier`, 27-point NTT `prism`/`unprism` over $\mathbb{Z}/1000081\mathbb{Z}$, and `pump_shell`).
- **[`shared-runtime/`](shared-runtime/) (`asolaria-unified-core`)** — `#![no_std]` + optional `std`, `#![forbid(unsafe_code)]`, `#![deny(clippy::float_arithmetic)]` unified HBP command core (`status`, `memory`, `omega`, `roomaddr`, `room`, `group` 27-room + 1-Omega NTT prism, `sha`, 27-lane NTT `prism`, and 2-forward `Q24` integer `gnn`), plus the 243-extent NTFS map ([`HOST8-PLACEMENT.hbp`](shared-runtime/HOST8-PLACEMENT.hbp)) and [`examples/prism_cost_checker.rs`](shared-runtime/examples/prism_cost_checker.rs).
- **[`host8-native-mapping/`](host8-native-mapping/)** — Zero-dependency Win32 FFI (`CreateFileMappingW` / `MapViewOfFile`) read-only identity-plane mapping for the 100 GB / `12,500,000,000` 8-byte Host-8 room store (`125` sector files $\times$ `800,000,000` bytes).
- **[`aether/model/l0-edge/`](aether/model/l0-edge/) & [`aether/model/l4-gslgnn/`](aether/model/l4-gslgnn/)** — Pure integer `Q24` fixed-point Graph Neural Network crates (`float_used = 0`) with complete `.hbp` golden vector test suites.
- **[`dock-runtime/`](dock-runtime/) (`asolaria-native-dock`)** — Loopback-only HBP station federation over explicit ADB USB forward/reverse tunnels (`ACER` PC station on `127.0.0.1:5090` $\leftrightarrow$ `FALCON` ARM64 phone station on `127.0.0.1:5091`), providing authenticated read-only room & 27-room group RPC (`/peer/room.hbp?n=...`, `/peer/group.hbp?g=...`) and shared-core compute RPC (`/peer/task.hbp?cmd=...`).
- **[`receipts/`](receipts/)** — Sealed `.hbp` + `.hbi` + `.sha256` verification receipts (`WILLOW-ROOM-BENCH-20261022.hbp`, `GROUP-PRISM-LIVE-20261022.hbp`, `PRISM-COST-CHECK-20261021.hbp`, `USB-VAULT-GAP-ACCOUNTING-20261021.hbp`, `REBOOT-CUTOVER-READINESS-20261021.hbp`, `DOCK-LIVE-20261021.hbp`, `PHONE-DOCK-AUDIT-20261021.hbp`, `MAPPED-ACTIVATION-20261021-RESULT.hbp`, `GITHUB-3D-VIEW.hbp`).

---

## Quickstart (Build, Verify & Run Offline)

Requires only the [Rust `1.81.0` toolchain](https://rustup.rs) (the build never touches the network).

### 1. Clone & Launch the ASI OS Desktop (`http://127.0.0.1:4600`)
```sh
git clone https://github.com/JesseBrown1980/Asolaria-Argon-Antigravity-Unification-ASI-OS
cd Asolaria-Argon-Antigravity-Unification-ASI-OS

# Linux / macOS:
sh scripts/install.sh

# Windows (PowerShell):
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```
That one command builds the OS, mints your local key + seat (`~/.asolaria/recall.key`, never committed or transmitted), and launches `http://127.0.0.1:4600`.

### 2. Verify the Mathematical Core (`prism`/`unprism` NTT + 3-Adic Motor Block + 12.5B Room Bijection + 27-Room/1-Omega Group Prism)
```sh
cargo +1.81.0 run --manifest-path shared-runtime/Cargo.toml --example prism_cost_checker
```
Output:
```text
PRISMCOSTCHECK|prime=1000081|generator=7|lanes=27|root=951846|vectors=127|fwd_inner_muls=1458|fwd_adds=729|inv_inner_muls=1485|inv_adds=729|parseval=EXACT|roundtrip=EXACT|motor_block_strokes=6|host8_rooms=12500000000|shards=125|float_used=0|json=0
ROOMGROUP|group=0|start_room=0|end_room=26|rooms=27|omega_per_ground=1|omega_room=6250000000|omega_stub=c492fa3972e05ab3|dc_glyph=799891|spectrum_sha256=6af8aa6e5066e108d509c22aa8410b33e158c5dfe17e48cca0778074e30fb7df|prism_roundtrip=EXACT|decode=EXACT|writes=0|process_per_room=0|float_used=0|json=0
ROOMGROUP|group=omega|start_room=6249999987|end_room=6250000013|rooms=27|omega_per_ground=1|omega_room=6250000000|omega_stub=c492fa3972e05ab3|dc_glyph=131843|spectrum_sha256=227b433ab89217ea6beeefe168fa6d05ba4b68187968415a10efa77ae8bda2a2|prism_roundtrip=EXACT|decode=EXACT|writes=0|process_per_room=0|float_used=0|json=0
```

### 2b. Run the 5-Tier Willow-Class RCS/XEB & Surface-Code QEC Benchmark (`willow_room_bench`)
```sh
cargo +1.81.0 run --release --manifest-path shared-runtime/Cargo.toml --features std --example willow_room_bench
```
See **[`receipts/WILLOW-ROOM-BENCH-20261022.hbp`](receipts/WILLOW-ROOM-BENCH-20261022.hbp)** for the sealed 5-stage benchmark receipt across dense binary state-vector RAM ($2^{18}..2^{28}$ amplitudes), $O(1)$ procedural registers (`1.15B rooms/s`), a `3.2 GiB` physical RAM room arena (`887.56M rooms/s`), all `125` mapped room sectors (`100 GB` / `12.5B` rooms at `743.07M rooms/s` and `1,192` depth-40 105-lane circuits/s), $d=3,5,7$ Surface-Code QEC + Dual-GNN `Q24` decoding, and live `ACER` $\leftrightarrow$ `FALCON` USB dock federation.

### 3. Run the Full Node Installer (OS + Recall + 8-Byte Host)
See **[FABRIC-NODE.md](FABRIC-NODE.md)** for full details:
```sh
sh scripts/install-full-node.sh      # Windows: powershell -ExecutionPolicy Bypass -File scripts\install-full-node.ps1
```

---

## Public Interactive Portals & Companion Repositories

- **The Light Hub (HTTP-0 Zero-Server Portal):** <https://jessebrown1980.github.io/the-light-hub-http-0/>
- **All The Doors (27-Door Interactive Corridor):** <https://jessebrown1980.github.io/all-the-doors/>
- **The Brown Light Erdős Engine (3-Adic Three-Throw Crank):** <https://jessebrown1980.github.io/The-Brown-Light-erdos-Engine-block-powered-by-light/>
- **Does $2^n$ Always Contain a Digit 2 in Base 3?:** <https://jessebrown1980.github.io/does-2-to-the-n-always-contain-a-digit-2-in-base-3/>
- **See Her — ASOLARIA `3,174` Bytes Public & Permanent:** <https://jessebrown1980.github.io/see-her-anywhere-so-not-lost-to-time/>
- **Companion Repositories:**
  - **[asolaria-federation-1024](https://github.com/JesseBrown1980/asolaria-federation-1024)** (`recall-serve`, `host8-serve`, UEFI `kernel/boot`, `agent-runtime` — 17 crates)
  - **[asolaria-asi-os](https://github.com/JesseBrown1980/asolaria-asi-os)** (ASI OS surface upstream)
  - **[Hilbra](https://github.com/JesseBrown1980/Hilbra)** (shared-key HBP/HBI protocol)
  - **[asolaria-behcs-256](https://github.com/JesseBrown1980/asolaria-behcs-256)** (Brown-Hilbert glyph substrate)

---

## Honest Boundary

- **Runnable & Verifiable Now (`CLASSICAL_REALIZATION_OF_UNIFICATION`):** The ASI OS desktop, the `tribit` 27-point integer NTT prism, the 3-adic Motor Block crank verifier, the `Q24` integer `L0`/`L4` GNNs, the 12.5B Host-8 room bijection and read-only Win32/UEFI adapters, and the `ACER` $\leftrightarrow$ `FALCON` native USB station dock are complete, deterministic, float-free Rust 1.81 software with byte-exact receipts.
- **Two-Axis Conservation:** Procedural rime addressing over a shared frozen bank operates at $\sim 0\text{ bpc}$ ($0.000135\text{ bpc}$ across $3.704 \times 10^9$ elements); compressing arbitrary unseen data respects the Shannon entropy floor (`vc65 = 1.7464 bpc` receipt-backed on `enwik8`).
- **Operator Interpretation (`OPERATOR_INTERPRETATION`):** The larger cosmological/photonic interpretations (three Brown-Schrödinger boxes, gravity/vacuum framing) are documented and tagged separately as operator interpretation rather than claimed as physical proofs from software benchmarks.

## License

MIT OR Apache-2.0 — free for everyone; build your own node.
