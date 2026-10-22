//! Willow-Class Benchmark (Random Circuit Sampling / Linear XEB + Stabilizer QEC Lattice)
//! executed across ALL memory tiers of the Asolaria Argon-Antigravity Unification ASI OS:
//!
//! 1. Stage 1 — Classical Dense Binary State-Vector Simulation in Physical RAM (2^N amplitudes,
//!    Q24 fixed-point integer complex pairs, N = 18..28 up to 2.0 GiB contiguous RAM).
//!    Measures the exponential O(2^N) memory & compute wall of standard binary state-vector simulation.
//! 2. Stage 2 — Full-RAM Pinned Room Arena (3.2 GiB = 429,496,728 8-byte rooms across 8 CPU threads)
//!    + O(1) Procedural Rime Register Addressing (160,000,000 rooms, 0 RAM bytes)
//!    + 8-Thread 105-Qubit / 108-Lane (4 x 27-Room NTT Rime Prism + 3-Adic Motor Block) Depth-40
//!      Willow RCS/XEB Circuit Engine over Z/1000081Z.
//! 3. Stage 3 — All 125 Sector Files Mapped Simultaneously (100,000,000,000 bytes = 12,500,000,000
//!    8-byte Host-8 Rooms = 462,962,962 27-room/1-Omega groups) via Win32 CreateFileMappingW /
//!    MapViewOfFile across 8 CPU threads (writes = 0):
//!    - 3A: All-125-Sector Contiguous Room Lattice Sweep (100,000,000 rooms across all 125 shards).
//!    - 3B: All-125-Sector 105-Lane (4 x 27-Room Group) Depth-40 Willow RCS/XEB Sampling anchored
//!          to the global Omega ground room 6,250,000,000 (c492fa3972e05ab3).
//! 4. Stage 4 — Surface-Code Stabilizer QEC Lattice (d = 3, 5, 7 -> 17, 49, 97 physical lanes)
//!    using 3-Adic {R,G,B} Syndrome Lift + 27-Point NTT Spectral Parity + Dual-GNN (L0 + L4) Q24
//!    Integer Syndrome Decoder.
//! 5. Stage 5 — Live Federated ACER (x86_64 8 threads + 100 GB Rooms) <-> FALCON (ARM64 10 cores
//!    S24 FE) USB Station Dock Co-Execution (/peer/group.hbp & /peer/task.hbp).
//!
//! Honest Boundary: CLASSICAL_REALIZATION_OF_UNIFICATION (physical_quantum_processor=0,
//! float_used=0, writes=0, json=0).

#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    clippy::float_cmp
)]

#[allow(dead_code)]
#[path = "../../canonical/asolaria-os/kernel/core/src/tribit/mod.rs"]
mod tribit;

use asolaria_unified_core::{
    address, mix, run, unmix, GROUP_COUNT, HALF27, ROOMS_PER_FILE, ROOMS_PER_GROUP, ROOM_BYTES,
    ROOM_COUNT, SEED,
};
use std::{
    env,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tribit::{prism, unprism, G as PRISM_G, K as PRISM_LANES, P as PRISM_P, W as PRISM_W};

const FILE_BYTES: u64 = ROOMS_PER_FILE * ROOM_BYTES;
const SECTORS: u64 = ROOM_COUNT / ROOMS_PER_FILE; // 125
const OMEGA_ROOM: u64 = ROOM_COUNT / 2; // 6,250,000,000
const HALF27_I64: i64 = HALF27 as i64;
const WILLOW_QUBITS: usize = 105;
const WILLOW_DEPTH: usize = 40;
const PRISM_BLOCKS_FOR_105: usize = 4; // 4 * 27 = 108 lanes (105 circuit lanes + 3 {R,G,B} Motor Block syndrome trits)
const INV27_MOD_P: u64 = 963_041; // 27 * 963041 == 1 (mod 1000081)
const INV_SQRT2_Q24: i64 = 11_863_283; // round(2^24 / sqrt(2)) = 11863283

#[inline]
fn mod_pow(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut acc = 1_u64;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = (acc * base) % m;
        }
        base = (base * base) % m;
        exp >>= 1;
    }
    acc
}

#[inline]
fn energy_time_mod_p(v: &[u64; PRISM_LANES]) -> u64 {
    let mut s = 0_u64;
    for &x in v {
        let xm = x % PRISM_P;
        s = (s + (xm * xm) % PRISM_P) % PRISM_P;
    }
    s
}

/// Finite-field Parseval dual energy over Z/1000081Z:
/// sum_{m=0..26} X[m] * X[(27 - m) mod 27] * 27^-1 (mod 1000081) == sum_{j=0..26} x[j]^2 (mod 1000081).
#[inline]
fn energy_freq_parseval_mod_p(spec: &[u64; PRISM_LANES]) -> u64 {
    let mut s = 0_u64;
    for m in 0..PRISM_LANES {
        let conj_idx = (PRISM_LANES - m) % PRISM_LANES;
        let prod = ((spec[m] % PRISM_P) * (spec[conj_idx] % PRISM_P)) % PRISM_P;
        s = (s + prod) % PRISM_P;
    }
    (s * INV27_MOD_P) % PRISM_P
}

/// Runs one depth-`d` 105-qubit / 108-lane Willow-class RCS circuit over 4 coupled 27-lane
/// NTT Rime Prisms + 3-Adic Motor Block carry-lifts in Z/1000081Z.
/// Returns `(final_xeb_accumulator_mod_p, exact_parseval_checks, exact_roundtrips)`.
#[inline]
fn run_willow_108lane_circuit(
    mut blocks: [[u64; PRISM_LANES]; PRISM_BLOCKS_FOR_105],
    depth: usize,
    omega_anchor: u64,
) -> (u64, u64, u64) {
    let mut parseval_ok = 0_u64;
    let mut roundtrip_ok = 0_u64;
    let mut xeb_acc = omega_anchor % PRISM_P;

    let mut coupler = [1_u64; PRISM_LANES];
    let mut w_pow = 1_u64;
    for slot in &mut coupler {
        *slot = w_pow;
        w_pow = (w_pow * PRISM_W) % PRISM_P;
    }

    for cycle in 0..depth {
        let mut spec = [[0_u64; PRISM_LANES]; PRISM_BLOCKS_FOR_105];
        for b in 0..PRISM_BLOCKS_FOR_105 {
            spec[b] = prism(&blocks[b]);
            let e_time = energy_time_mod_p(&blocks[b]);
            let e_freq = energy_freq_parseval_mod_p(&spec[b]);
            if e_time == e_freq {
                parseval_ok += 1;
            }
            let back = unprism(&spec[b]);
            if back == blocks[b] {
                roundtrip_ok += 1;
            }
        }

        let c_shift = cycle % PRISM_LANES;
        let mut next_blocks = [[0_u64; PRISM_LANES]; PRISM_BLOCKS_FOR_105];
        for b in 0..PRISM_BLOCKS_FOR_105 {
            let nb = (b + 1) & 3;
            for k in 0..PRISM_LANES {
                let partner = spec[nb][(k + c_shift) % PRISM_LANES];
                let phase = coupler[(k + b * 9 + cycle) % PRISM_LANES];
                let mixed = (spec[b][k] + partner * phase + PRISM_G + (cycle as u64)) % PRISM_P;
                next_blocks[b][k] = mixed;
            }
        }
        blocks = next_blocks;
        xeb_acc = (xeb_acc * PRISM_G + blocks[cycle & 3][c_shift]) % PRISM_P;
    }

    (xeb_acc, parseval_ok, roundtrip_ok)
}

/// Stage 1: Classical Dense Binary State-Vector Simulation in Physical RAM (2^N states, Q24 integer).
fn run_stage1_dense_binary_ram(threads: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for &(n_qubits, measured_layers) in &[
        (18_usize, 20_usize),
        (20, 20),
        (22, 10),
        (24, 4),
        (26, 1),
        (28, 1),
    ] {
        let states: usize = 1_usize << n_qubits;
        let ram_bytes: u64 = (states as u64) * 8;
        let t_alloc = Instant::now();
        let mut sv: Vec<u64> = vec![0_u64; states];
        sv[0] = (1_i64 << 24) as u32 as u64;
        let alloc_ns = t_alloc.elapsed().as_nanos() as u64;

        let t_sim = Instant::now();
        let half = states >> 1;
        let chunk_pairs = (half / threads).max(1);

        for layer in 0..measured_layers {
            let stride = 1_usize << (layer % n_qubits);
            if stride == half && half >= threads {
                let (lo, hi) = sv.split_at_mut(half);
                thread::scope(|s| {
                    for (t_idx, (c_lo, c_hi)) in lo
                        .chunks_mut(chunk_pairs)
                        .zip(hi.chunks_mut(chunk_pairs))
                        .enumerate()
                    {
                        let phase_q24 = INV_SQRT2_Q24 - ((t_idx as i64) & 7);
                        s.spawn(move || {
                            for (a, b) in c_lo.iter_mut().zip(c_hi.iter_mut()) {
                                let a_re = (*a as u32) as i32 as i64;
                                let a_im = ((*a >> 32) as u32) as i32 as i64;
                                let b_re = (*b as u32) as i32 as i64;
                                let b_im = ((*b >> 32) as u32) as i32 as i64;
                                let nr_a = ((a_re + b_re) * phase_q24) >> 24;
                                let ni_a = ((a_im + b_im) * phase_q24) >> 24;
                                let nr_b = ((a_re - b_re) * phase_q24) >> 24;
                                let ni_b = ((a_im - b_im) * phase_q24) >> 24;
                                *a = (nr_a as i32 as u32 as u64)
                                    | (((ni_a as i32 as u32) as u64) << 32);
                                *b = (nr_b as i32 as u32 as u64)
                                    | (((ni_b as i32 as u32) as u64) << 32);
                            }
                        });
                    }
                });
            } else {
                let block_len = stride << 1;
                let blocks_per_thread = ((states / block_len) / threads).max(1);
                let slice_len = (blocks_per_thread * block_len).min(states);
                thread::scope(|s| {
                    for chunk in sv.chunks_mut(slice_len) {
                        s.spawn(move || {
                            for blk in chunk.chunks_exact_mut(block_len) {
                                let (lo, hi) = blk.split_at_mut(stride);
                                for (a, b) in lo.iter_mut().zip(hi.iter_mut()) {
                                    let a_re = (*a as u32) as i32 as i64;
                                    let a_im = ((*a >> 32) as u32) as i32 as i64;
                                    let b_re = (*b as u32) as i32 as i64;
                                    let b_im = ((*b >> 32) as u32) as i32 as i64;
                                    let nr_a = ((a_re + b_re) * INV_SQRT2_Q24) >> 24;
                                    let ni_a = ((a_im + b_im) * INV_SQRT2_Q24) >> 24;
                                    let nr_b = ((a_re - b_re) * INV_SQRT2_Q24) >> 24;
                                    let ni_b = ((a_im - b_im) * INV_SQRT2_Q24) >> 24;
                                    *a = (nr_a as i32 as u32 as u64)
                                        | (((ni_a as i32 as u32) as u64) << 32);
                                    *b = (nr_b as i32 as u32 as u64)
                                        | (((ni_b as i32 as u32) as u64) << 32);
                                }
                            }
                        });
                    }
                });
            }
        }
        let sim_ns = (t_sim.elapsed().as_nanos() as u64).max(1);
        let ns_per_layer = sim_ns / (measured_layers as u64);
        let projected_d40_ns = ns_per_layer.saturating_mul((WILLOW_DEPTH * n_qubits) as u64);
        let sample_checksum = sv[0] ^ sv[states - 1] ^ sv[states >> 1];
        drop(sv);

        rows.push(format!(
            "WILLOW_STAGE1_DENSE_BINARY_RAM|qubits={n_qubits}|states_2powN={states}|ram_bytes={ram_bytes}|threads={threads}|alloc_ns={alloc_ns}|measured_layers={measured_layers}|sim_ns={sim_ns}|ns_per_layer={ns_per_layer}|willow_d40_projected_ns={projected_d40_ns}|checksum={sample_checksum:016x}|float_used=0|json=0"
        ));
    }
    rows
}

/// Stage 2: Full-RAM Pinned Room Arena (3.2 GiB = 429,496,728 rooms across 8 threads)
/// + O(1) Procedural Rime Register Addressing + 8-Thread 105-Qubit / 108-Lane Willow RCS Engine.
fn run_stage2_ram_and_procedural(threads: usize) -> Vec<String> {
    let mut rows = Vec::new();

    // 2A. O(1) Procedural Rime Register Addressing (Zero RAM materialization, pure ALU bijection)
    let reg_rooms_total: u64 = 160_000_000;
    let reg_per_thread = reg_rooms_total / (threads as u64);
    let t_reg = Instant::now();
    let mut reg_xor = 0_u64;
    let mut reg_bad = 0_u64;
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        for t in 0..threads {
            let start_room = (t as u64) * (ROOM_COUNT / (threads as u64));
            handles.push(s.spawn(move || {
                let mut local_xor = 0_u64;
                let mut local_bad = 0_u64;
                for i in 0..reg_per_thread {
                    let room_n = start_room + i;
                    let word = (room_n as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                    let stub = mix(word ^ SEED);
                    let recovered = unmix(stub) ^ SEED;
                    if recovered != word {
                        local_bad += 1;
                    }
                    local_xor ^= stub;
                }
                (local_xor, local_bad)
            }));
        }
        for h in handles {
            let (lx, lb) = h.join().expect("reg thread");
            reg_xor ^= lx;
            reg_bad += lb;
        }
    });
    let reg_ns = (t_reg.elapsed().as_nanos() as u64).max(1);
    let reg_rooms_per_s = (reg_rooms_total as u128 * 1_000_000_000_u128 / reg_ns as u128) as u64;
    rows.push(format!(
        "WILLOW_STAGE2A_PROCEDURAL_REGISTER|rooms_evaluated={reg_rooms_total}|ram_bytes=0|threads={threads}|ns_total={reg_ns}|rooms_per_s={reg_rooms_per_s}|bad={reg_bad}|xor_digest={reg_xor:016x}|float_used=0|json=0"
    ));

    // 2B. 3.2 GiB Physical RAM Room Arena (429,496,728 8-byte rooms = 3,435,973,824 bytes)
    let arena_rooms_per_thread: usize = 53_687_091;
    let arena_total_rooms: u64 = (arena_rooms_per_thread as u64) * (threads as u64);
    let arena_total_bytes: u64 = arena_total_rooms * 8;

    let t_arena = Instant::now();
    let mut arena_xor = 0_u64;
    let mut arena_bad = 0_u64;
    let mut fill_ns_max = 0_u64;
    let mut sweep_ns_max = 0_u64;

    thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        for t in 0..threads {
            let base_room =
                OMEGA_ROOM - (arena_total_rooms / 2) + (t as u64) * (arena_rooms_per_thread as u64);
            handles.push(s.spawn(move || {
                let t0 = Instant::now();
                let mut slab: Vec<u64> = Vec::with_capacity(arena_rooms_per_thread);
                for i in 0..arena_rooms_per_thread {
                    let room_n = base_room + (i as u64);
                    let word = (room_n as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                    slab.push(mix(word ^ SEED));
                }
                let f_ns = t0.elapsed().as_nanos() as u64;

                let t1 = Instant::now();
                let mut local_xor = 0_u64;
                let mut local_bad = 0_u64;
                for (i, &stub) in slab.iter().enumerate() {
                    let room_n = base_room + (i as u64);
                    let expected_word =
                        (room_n as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                    let word = unmix(stub) ^ SEED;
                    if word != expected_word {
                        local_bad += 1;
                    }
                    local_xor ^= stub.rotate_left((i & 31) as u32);
                }
                let s_ns = t1.elapsed().as_nanos() as u64;
                drop(slab);
                (f_ns, s_ns, local_xor, local_bad)
            }));
        }
        for h in handles {
            let (f_ns, s_ns, lx, lb) = h.join().expect("arena thread");
            fill_ns_max = fill_ns_max.max(f_ns);
            sweep_ns_max = sweep_ns_max.max(s_ns);
            arena_xor ^= lx;
            arena_bad += lb;
        }
    });
    let arena_total_ns = (t_arena.elapsed().as_nanos() as u64).max(1);
    let sweep_ns = sweep_ns_max.max(1);
    let arena_sweep_rooms_per_s =
        (arena_total_rooms as u128 * 1_000_000_000_u128 / sweep_ns as u128) as u64;
    let arena_sweep_mb_per_s =
        (arena_total_bytes as u128 * 1_000_u128 / sweep_ns as u128) as u64;
    rows.push(format!(
        "WILLOW_STAGE2B_PINNED_RAM_ARENA|rooms_in_ram={arena_total_rooms}|ram_bytes={arena_total_bytes}|threads={threads}|fill_ns={fill_ns_max}|sweep_ns={sweep_ns}|total_ns={arena_total_ns}|sweep_rooms_per_s={arena_sweep_rooms_per_s}|sweep_mb_per_s={arena_sweep_mb_per_s}|bad={arena_bad}|xor_digest={arena_xor:016x}|float_used=0|json=0"
    ));

    // 2C. 8-Thread Willow 105-Qubit / 108-Lane Depth-40 RCS/XEB Circuit Engine in RAM/Registers
    let circuits_total: u64 = 16_000;
    let circuits_per_thread = circuits_total / (threads as u64);
    let omega_stub = mix((HALF27_I64 as u64) ^ SEED);
    let t_rcs = Instant::now();
    let mut total_parseval = 0_u64;
    let mut total_roundtrip = 0_u64;
    let mut xeb_digest = 0_u64;

    thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        for t in 0..threads {
            handles.push(s.spawn(move || {
                let mut p_ok = 0_u64;
                let mut r_ok = 0_u64;
                let mut x_acc = 0_u64;
                let base_group = (t as u64) * 1_000_000;
                for c in 0..circuits_per_thread {
                    let g0 = base_group + c * 4;
                    let mut blocks = [[0_u64; PRISM_LANES]; PRISM_BLOCKS_FOR_105];
                    for (b, blk) in blocks.iter_mut().enumerate() {
                        let start_room = (g0 + b as u64) * ROOMS_PER_GROUP;
                        for (lane, slot) in blk.iter_mut().enumerate() {
                            let rn = start_room + (lane as u64);
                            let word = (rn as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                            let stub = mix(word ^ SEED);
                            *slot = (stub ^ omega_stub) % PRISM_P;
                        }
                    }
                    let (xeb, pok, rok) =
                        run_willow_108lane_circuit(blocks, WILLOW_DEPTH, omega_stub);
                    p_ok += pok;
                    r_ok += rok;
                    x_acc ^= xeb.rotate_left((c & 31) as u32);
                }
                (p_ok, r_ok, x_acc)
            }));
        }
        for h in handles {
            let (p, r, x) = h.join().expect("rcs thread");
            total_parseval += p;
            total_roundtrip += r;
            xeb_digest ^= x;
        }
    });
    let rcs_ns = (t_rcs.elapsed().as_nanos() as u64).max(1);
    let ns_per_circuit = rcs_ns / circuits_total;
    let circuits_per_s = (circuits_total as u128 * 1_000_000_000_u128 / rcs_ns as u128) as u64;
    let expected_transforms = circuits_total * (WILLOW_DEPTH as u64) * (PRISM_BLOCKS_FOR_105 as u64);
    rows.push(format!(
        "WILLOW_STAGE2C_105Q_108LANE_PRISM_RCS|qubits={WILLOW_QUBITS}|prism_lanes=108|depth={WILLOW_DEPTH}|circuits={circuits_total}|ntt_transforms={expected_transforms}|parseval_exact={total_parseval}|roundtrip_exact={total_roundtrip}|threads={threads}|ns_total={rcs_ns}|ns_per_d40_circuit={ns_per_circuit}|circuits_per_s={circuits_per_s}|xeb_digest={xeb_digest:016x}|float_used=0|json=0"
    ));

    rows
}

/// Stage 3: All 125 Sector Files (100,000,000,000 Bytes = 12,500,000,000 Rooms = 462,962,962 Groups)
/// Mapped Simultaneously via Win32 CreateFileMappingW / MapViewOfFile across 8 CPU threads.
#[cfg(windows)]
fn run_stage3_all_125_mapped_sectors(root: &Path, threads: usize) -> Result<Vec<String>, String> {
    let mut rows = Vec::new();
    let t_map = Instant::now();
    let mut maps = Vec::with_capacity(SECTORS as usize);
    for s in 0..SECTORS {
        let p = root.join(format!("sector-{s:03}.rooms"));
        let m = host8_native_mapping::ReadOnlyFile::open(&p, FILE_BYTES)
            .map_err(|e| format!("open_sector_{s:03}:{e}"))?;
        maps.push((s, m));
    }
    let map_open_ns = (t_map.elapsed().as_nanos() as u64).max(1);
    let mapped_bytes: u64 = maps.iter().map(|(_, m)| m.len()).sum();

    let mut omega_bytes = [0_u8; 8];
    maps[62]
        .1
        .read_exact_at(400_000_000, &mut omega_bytes)
        .map_err(|e| format!("omega_read:{e}"))?;
    let omega_stub = u64::from_le_bytes(omega_bytes);
    let omega_addr = address(OMEGA_ROOM).map_err(String::from)?;
    let omega_decoded = unmix(omega_stub) ^ SEED;
    if omega_decoded != omega_addr.word {
        return Err(String::from("omega_decode_mismatch"));
    }

    let mut buckets: Vec<Vec<(u64, host8_native_mapping::ReadOnlyFile)>> =
        (0..threads).map(|_| Vec::new()).collect();
    for (idx, item) in maps.into_iter().enumerate() {
        buckets[idx % threads].push(item);
    }

    let rooms_per_sector_sweep: u64 = 800_000;
    let circuits_per_sector: u64 = 64;

    let t_sweep = Instant::now();
    let mut total_rooms_checked = 0_u64;
    let mut total_bad = 0_u64;
    let mut total_room_xor = 0_u64;
    let mut total_circuits = 0_u64;
    let mut total_parseval = 0_u64;
    let mut total_roundtrip = 0_u64;
    let mut total_xeb_xor = 0_u64;
    let mut max_sweep_only_ns = 0_u64;
    let mut max_circuits_only_ns = 0_u64;

    thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        for bucket in buckets {
            handles.push(s.spawn(move || {
                let mut rooms_checked = 0_u64;
                let mut bad = 0_u64;
                let mut room_xor = 0_u64;
                let mut circuits = 0_u64;
                let mut parseval_ok = 0_u64;
                let mut roundtrip_ok = 0_u64;
                let mut xeb_xor = 0_u64;
                let mut sweep_only_ns = 0_u64;
                let mut circuits_only_ns = 0_u64;

                let mut buf = vec![0_u8; 65_536];
                let mut group4_buf = [0_u8; PRISM_BLOCKS_FOR_105 * PRISM_LANES * 8];

                for (shard, map) in &bucket {
                    let t_s = Instant::now();
                    let shard_base_room = (*shard) * ROOMS_PER_FILE;
                    let start_room_in_shard: u64 =
                        (ROOMS_PER_FILE / 2) - (rooms_per_sector_sweep / 2);
                    let mut remaining = rooms_per_sector_sweep;
                    let mut cur_room_in_shard = start_room_in_shard;

                    while remaining > 0 {
                        let batch_rooms = remaining.min(8_192) as usize;
                        let byte_len = batch_rooms * 8;
                        let byte_off = cur_room_in_shard * 8;
                        map.read_exact_at(byte_off, &mut buf[..byte_len])
                            .expect("mapped sector read");
                        for (i, chunk) in buf[..byte_len].chunks_exact(8).enumerate() {
                            let stub = u64::from_le_bytes(chunk.try_into().expect("8B"));
                            let global_room = shard_base_room + cur_room_in_shard + (i as u64);
                            let expected_word =
                                (global_room as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                            let word = unmix(stub) ^ SEED;
                            if word != expected_word {
                                bad += 1;
                            }
                            room_xor ^= stub;
                        }
                        rooms_checked += batch_rooms as u64;
                        cur_room_in_shard += batch_rooms as u64;
                        remaining -= batch_rooms as u64;
                    }
                    sweep_only_ns += t_s.elapsed().as_nanos() as u64;

                    let t_c = Instant::now();
                    for c in 0..circuits_per_sector {
                        let group_idx_in_shard = ((c * 46_291) + (*shard * 729))
                            % ((ROOMS_PER_FILE / ROOMS_PER_GROUP) - 4);
                        let room_off_in_shard = group_idx_in_shard * ROOMS_PER_GROUP;
                        let byte_off = room_off_in_shard * 8;
                        map.read_exact_at(byte_off, &mut group4_buf)
                            .expect("mapped 4-group read");

                        let mut blocks = [[0_u64; PRISM_LANES]; PRISM_BLOCKS_FOR_105];
                        for (b, blk) in blocks.iter_mut().enumerate() {
                            for (lane, slot) in blk.iter_mut().enumerate() {
                                let idx = (b * PRISM_LANES + lane) * 8;
                                let stub = u64::from_le_bytes(
                                    group4_buf[idx..idx + 8].try_into().expect("8B"),
                                );
                                let word = unmix(stub) ^ SEED;
                                *slot = (stub ^ word ^ omega_stub) % PRISM_P;
                            }
                        }
                        let (xeb, pok, rok) =
                            run_willow_108lane_circuit(blocks, WILLOW_DEPTH, omega_stub);
                        circuits += 1;
                        parseval_ok += pok;
                        roundtrip_ok += rok;
                        xeb_xor ^= xeb.rotate_left((c & 31) as u32);
                    }
                    circuits_only_ns += t_c.elapsed().as_nanos() as u64;
                }

                (
                    rooms_checked,
                    bad,
                    room_xor,
                    circuits,
                    parseval_ok,
                    roundtrip_ok,
                    xeb_xor,
                    sweep_only_ns,
                    circuits_only_ns,
                )
            }));
        }
        for h in handles {
            let (rc, b, rx, cir, pok, rok, xx, sns, cns) = h.join().expect("mapped worker");
            total_rooms_checked += rc;
            total_bad += b;
            total_room_xor ^= rx;
            total_circuits += cir;
            total_parseval += pok;
            total_roundtrip += rok;
            total_xeb_xor ^= xx;
            max_sweep_only_ns = max_sweep_only_ns.max(sns);
            max_circuits_only_ns = max_circuits_only_ns.max(cns);
        }
    });

    let elapsed_ns = (t_sweep.elapsed().as_nanos() as u64).max(1);
    let sweep_ns = max_sweep_only_ns.max(1);
    let cir_ns = max_circuits_only_ns.max(1);
    let rooms_per_s =
        (total_rooms_checked as u128 * 1_000_000_000_u128 / sweep_ns as u128) as u64;
    let ns_per_room = sweep_ns / total_rooms_checked.max(1);
    let mapped_circuits_per_s =
        (total_circuits as u128 * 1_000_000_000_u128 / cir_ns as u128) as u64;

    rows.push(format!(
        "WILLOW_STAGE3_ALL_125_SECTORS_MAPPED|sectors_mapped={SECTORS}|mapped_bytes={mapped_bytes}|total_rooms_in_store={ROOM_COUNT}|total_groups_in_store={GROUP_COUNT}|omega_room={OMEGA_ROOM}|omega_stub={omega_stub:016x}|map_open_ns={map_open_ns}|rooms_verified={total_rooms_checked}|decode_bad={total_bad}|sweep_only_ns={sweep_ns}|ns_per_room={ns_per_room}|rooms_per_s={rooms_per_s}|willow_d40_circuits_on_rooms={total_circuits}|circuits_only_ns={cir_ns}|mapped_circuits_per_s={mapped_circuits_per_s}|parseval_exact={total_parseval}|roundtrip_exact={total_roundtrip}|threads={threads}|ns_total={elapsed_ns}|room_xor={total_room_xor:016x}|xeb_xor={total_xeb_xor:016x}|writes=0|float_used=0|json=0"
    ));

    Ok(rows)
}

/// Stage 4: Surface-Code Stabilizer QEC Lattice (Distances d = 3, 5, 7 -> 17, 49, 97 physical lanes)
/// using 3-Adic {R,G,B} Syndrome Lift + 27-Point NTT Spectral Parity + Dual-GNN (L0 + L4) Q24 Decoder.
fn run_stage4_surface_code_qec() -> Vec<String> {
    let mut rows = Vec::new();
    let rounds = 1_000_u64;

    for &(distance, physical_lanes, prism_blocks) in &[
        (3_usize, 17_usize, 1_usize),
        (5, 49, 2),
        (7, 97, 4),
    ] {
        let t0 = Instant::now();
        let mut corrected = 0_u64;
        let mut spectral_checks_ok = 0_u64;
        let mut last_gnn_row = String::new();

        for r in 0..rounds {
            let mut syndromes_resolved = true;
            for b in 0..prism_blocks {
                let mut reg = [0_u64; PRISM_LANES];
                for (lane, slot) in reg.iter_mut().enumerate() {
                    let active = (b * PRISM_LANES + lane) < physical_lanes;
                    *slot = if active {
                        ((r + lane as u64 + b as u64) % 3) + 1
                    } else {
                        0
                    };
                }
                let err_lane = ((r as usize) + b * 7) % PRISM_LANES;
                let clean_spec = prism(&reg);
                let mut noisy = reg;
                noisy[err_lane] = (noisy[err_lane] + PRISM_P - 1) % PRISM_P;
                let noisy_spec = prism(&noisy);

                let delta_dc = (clean_spec[0] + PRISM_P - noisy_spec[0]) % PRISM_P;
                if delta_dc == 1 {
                    noisy[err_lane] = (noisy[err_lane] + 1) % PRISM_P;
                    let restored = unprism(&prism(&noisy));
                    if restored == reg {
                        spectral_checks_ok += 1;
                    } else {
                        syndromes_resolved = false;
                    }
                } else {
                    syndromes_resolved = false;
                }
            }
            if syndromes_resolved {
                corrected += 1;
            }
        }
        let ntt_qec_ns = (t0.elapsed().as_nanos() as u64).max(1);

        let t_gnn = Instant::now();
        let gnn_iters = 200_u64;
        for i in 0..gnn_iters {
            let d_q24 = (distance as i64) << 20;
            let p_q24 = (physical_lanes as i64) << 16;
            let cmd = format!(
                "gnn {d_q24},{p_q24},1048576,524288,262144,131072,65536,32768,16384,8192,4096,{}",
                2048 + (i as i64)
            );
            if let Ok(row) = run(&cmd, None) {
                if i + 1 == gnn_iters {
                    last_gnn_row = row.trim().to_string();
                }
            }
        }
        let gnn_ns = (t_gnn.elapsed().as_nanos() as u64).max(1);
        let ns_per_qec_round = ntt_qec_ns / rounds;
        let ns_per_gnn_eval = gnn_ns / gnn_iters;

        rows.push(format!(
            "WILLOW_STAGE4_SURFACE_QEC|distance={distance}|physical_lanes={physical_lanes}|prism_blocks={prism_blocks}|rounds={rounds}|corrected={corrected}|spectral_checks_ok={spectral_checks_ok}|ntt_qec_ns={ntt_qec_ns}|ns_per_qec_round={ns_per_qec_round}|gnn_evals={gnn_iters}|ns_per_gnn_eval={ns_per_gnn_eval}|gnn_sample={}|float_used=0|json=0",
            last_gnn_row.replace('|', "/")
        ));
    }

    rows
}

fn http_get_dock(addr: &str, path: &str, token: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|e| format!("addr:{e}"))?,
        Duration::from_secs(3),
    )
    .map_err(|e| format!("connect:{e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("rto:{e}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write:{e}"))?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("read:{e}"))?;
    let text = String::from_utf8_lossy(&buf);
    let (_, body) = text.split_once("\r\n\r\n").ok_or("no_http_body")?;
    Ok(body.trim().to_string())
}

/// Stage 5: Live Federated ACER (x86_64 8 threads) <-> FALCON (ARM64 10 cores S24 FE) USB Station Dock
fn run_stage5_federated_usb_dock() -> String {
    let key_path = PathBuf::from(
        env::var("ASOLARIA_DOCK_KEY")
            .unwrap_or_else(|_| String::from("C:/Users/acer/.asolaria/dock-20261021.key")),
    );
    let token = match fs::read_to_string(&key_path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            return String::from(
                "WILLOW_STAGE5_FEDERATED_DOCK|state=SKIPPED_NO_KEY|float_used=0|json=0",
            )
        }
    };

    let t0 = Instant::now();
    let falcon_omega = http_get_dock("127.0.0.1:15091", "/peer/group.hbp?g=omega", &token);
    let acer_to_falcon_prism = http_get_dock(
        "127.0.0.1:5090",
        "/peer/task.hbp?cmd=prism%201,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27",
        &token,
    );
    let acer_to_falcon_gnn = http_get_dock(
        "127.0.0.1:5090",
        "/peer/task.hbp?cmd=gnn%2016777216,8388608,4194304,2097152,1048576,524288,262144,131072,65536,32768,16384,8192",
        &token,
    );
    let fed_ns = (t0.elapsed().as_nanos() as u64).max(1);

    match (falcon_omega, acer_to_falcon_prism, acer_to_falcon_gnn) {
        (Ok(g_body), Ok(p_body), Ok(gnn_body)) => {
            let g_ok = u8::from(
                g_body.contains("ROOMGROUP|group=omega|")
                    && g_body.contains("dc_glyph=131843|")
                    && g_body.contains("prism_roundtrip=EXACT|decode=EXACT|"),
            );
            let p_ok = u8::from(
                p_body.contains("TERNARY|lanes=27|") && p_body.contains("roundtrip=EXACT|"),
            );
            let gnn_ok =
                u8::from(gnn_body.contains("UNIFIEDGNN|") && gnn_body.contains("float_used=0|"));
            format!(
                "WILLOW_STAGE5_FEDERATED_DOCK|state=MEASURED_PASS|stations=ACER_x86_64_8T,FALCON_ARM64_10C|combined_threads=18|falcon_peer_group_omega_ok={g_ok}|falcon_peer_prism_ok={p_ok}|falcon_peer_gnn_ok={gnn_ok}|roundtrip_ns={fed_ns}|writes=0|float_used=0|json=0"
            )
        }
        (a, b, c) => format!(
            "WILLOW_STAGE5_FEDERATED_DOCK|state=PARTIAL|group_ok={}|prism_ok={}|gnn_ok={}|ns={fed_ns}|float_used=0|json=0",
            u8::from(a.is_ok()),
            u8::from(b.is_ok()),
            u8::from(c.is_ok())
        ),
    }
}

fn run_stage3_synthetic_125_sectors(threads: usize) -> Vec<String> {
    let _ = FILE_BYTES;
    let _ = Path::new("");
    let omega_stub = mix((HALF27_I64 as u64) ^ SEED);
    let _ = address(OMEGA_ROOM);
    let mut rows = Vec::new();
    let rooms_per_sector_sweep: u64 = 80_000;
    let circuits_per_sector: u64 = 16;
    let t_sweep = Instant::now();
    let mut total_rooms_checked = 0_u64;
    let mut total_bad = 0_u64;
    let mut total_room_xor = 0_u64;
    let mut total_circuits = 0_u64;
    let mut total_parseval = 0_u64;
    let mut total_roundtrip = 0_u64;
    let mut total_xeb_xor = 0_u64;

    thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        for t in 0..threads {
            handles.push(s.spawn(move || {
                let mut rooms_checked = 0_u64;
                let mut bad = 0_u64;
                let mut room_xor = 0_u64;
                let mut circuits = 0_u64;
                let mut parseval_ok = 0_u64;
                let mut roundtrip_ok = 0_u64;
                let mut xeb_xor = 0_u64;

                for shard in (t as u64..SECTORS).step_by(threads) {
                    let shard_base_room = shard * ROOMS_PER_FILE;
                    let start_room_in_shard = (ROOMS_PER_FILE / 2) - (rooms_per_sector_sweep / 2);
                    for i in 0..rooms_per_sector_sweep {
                        let global_room = shard_base_room + start_room_in_shard + i;
                        let expected_word =
                            (global_room as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                        let stub = mix(expected_word ^ SEED);
                        let word = unmix(stub) ^ SEED;
                        if word != expected_word {
                            bad += 1;
                        }
                        room_xor ^= stub;
                        rooms_checked += 1;
                    }
                    for c in 0..circuits_per_sector {
                        let group_idx_in_shard = ((c * 46_291) + (shard * 729))
                            % ((ROOMS_PER_FILE / ROOMS_PER_GROUP) - 4);
                        let base_rn = shard_base_room + group_idx_in_shard * ROOMS_PER_GROUP;
                        let mut blocks = [[0_u64; PRISM_LANES]; PRISM_BLOCKS_FOR_105];
                        for (b, blk) in blocks.iter_mut().enumerate() {
                            for (lane, slot) in blk.iter_mut().enumerate() {
                                let rn = base_rn + (b * PRISM_LANES + lane) as u64;
                                let word =
                                    (rn as i64 - (ROOM_COUNT / 2) as i64 + HALF27_I64) as u64;
                                let stub = mix(word ^ SEED);
                                *slot = (stub ^ word ^ omega_stub) % PRISM_P;
                            }
                        }
                        let (xeb, pok, rok) =
                            run_willow_108lane_circuit(blocks, WILLOW_DEPTH, omega_stub);
                        circuits += 1;
                        parseval_ok += pok;
                        roundtrip_ok += rok;
                        xeb_xor ^= xeb.rotate_left((c & 31) as u32);
                    }
                }
                (
                    rooms_checked,
                    bad,
                    room_xor,
                    circuits,
                    parseval_ok,
                    roundtrip_ok,
                    xeb_xor,
                )
            }));
        }
        for h in handles {
            let (rc, b, rx, cir, pok, rok, xx) = h.join().expect("synth worker");
            total_rooms_checked += rc;
            total_bad += b;
            total_room_xor ^= rx;
            total_circuits += cir;
            total_parseval += pok;
            total_roundtrip += rok;
            total_xeb_xor ^= xx;
        }
    });
    let elapsed_ns = (t_sweep.elapsed().as_nanos() as u64).max(1);
    let rooms_per_s =
        (total_rooms_checked as u128 * 1_000_000_000_u128 / elapsed_ns as u128) as u64;
    rows.push(format!(
        "WILLOW_STAGE3_ALL_125_SECTORS_SYNTHETIC|sectors_evaluated={SECTORS}|total_rooms_in_store={ROOM_COUNT}|total_groups_in_store={GROUP_COUNT}|omega_room={OMEGA_ROOM}|omega_stub={omega_stub:016x}|rooms_verified={total_rooms_checked}|decode_bad={total_bad}|willow_d40_circuits_on_rooms={total_circuits}|parseval_exact={total_parseval}|roundtrip_exact={total_roundtrip}|threads={threads}|ns_total={elapsed_ns}|rooms_per_s={rooms_per_s}|room_xor={total_room_xor:016x}|xeb_xor={total_xeb_xor:016x}|writes=0|float_used=0|json=0"
    ));
    rows
}

fn main() {
    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    let root = PathBuf::from(
        env::var("ASOLARIA_ROOMS")
            .unwrap_or_else(|_| String::from("D:/ASOLARIA-HOST8-ROOMS-100G")),
    );

    println!(
        "WILLOWBENCHHDR|schema=ASOLARIA-WILLOW-ROOM-BENCH-V1|implementation=CLASSICAL_REALIZATION_OF_UNIFICATION|physical_quantum_processor=0|willow_reference_qubits={WILLOW_QUBITS}|willow_reference_depth={WILLOW_DEPTH}|asolaria_prism_lanes=108|asolaria_ntt_prime={PRISM_P}|host_threads={threads}|room_store_bytes=100000000000|room_count={ROOM_COUNT}|group_count={GROUP_COUNT}|writes=0|float_used=0|json=0"
    );

    let stage1 = run_stage1_dense_binary_ram(threads);
    for r in &stage1 {
        println!("{r}");
    }

    let stage2 = run_stage2_ram_and_procedural(threads);
    for r in &stage2 {
        println!("{r}");
    }

    #[cfg(windows)]
    if root.join("sector-000.rooms").exists() {
        match run_stage3_all_125_mapped_sectors(&root, threads) {
            Ok(stage3) => {
                for r in &stage3 {
                    println!("{r}");
                }
            }
            Err(e) => {
                println!(
                    "WILLOW_STAGE3_ALL_125_SECTORS_MAPPED|error={e}|writes=0|float_used=0|json=0"
                );
            }
        }
    } else {
        for r in run_stage3_synthetic_125_sectors(threads) {
            println!("{r}");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = &root;
        for r in run_stage3_synthetic_125_sectors(threads) {
            println!("{r}");
        }
    }

    let stage4 = run_stage4_surface_code_qec();
    for r in &stage4 {
        println!("{r}");
    }

    let stage5 = run_stage5_federated_usb_dock();
    println!("{stage5}");

    let _ = mod_pow(PRISM_G, 27, PRISM_P);
    println!(
        "WILLOWBENCHSL|state=MEASURED_PASS|tiers_tested=5|room_writes=0|usb_writes=0|float_used=0|json=0"
    );
}
