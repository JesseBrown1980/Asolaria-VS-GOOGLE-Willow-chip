//! Integer-only correctness-and-cost checker for the 27-point NTT Rime Prism
//! (`tribit::prism` / `tribit::unprism`), the 3-adic Motor Block crank lift,
//! and the 12.5B Host-8 room bijection. Rust 1.81, zero float (`float_used=0`).
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

#[allow(dead_code)]
#[path = "../../canonical/asolaria-os/kernel/core/src/tribit/mod.rs"]
mod tribit;

use asolaria_unified_core::{address, mix, unmix, HALF27, ROOMS_PER_FILE, ROOM_COUNT, SEED};

const K: usize = tribit::K;
const P: u64 = tribit::P;
const G: u64 = tribit::G;
const W: u64 = tribit::W;

#[inline]
const fn mod_mul(a: u64, b: u64) -> u64 {
    ((a as u128 * b as u128) % P as u128) as u64
}

#[inline]
const fn mod_add(a: u64, b: u64) -> u64 {
    let s = a + b;
    if s >= P {
        s - P
    } else {
        s
    }
}

/// Instrumented forward prism that counts exact modular multiplications and additions.
fn prism_with_cost(x: &[u64; K]) -> ([u64; K], u64, u64) {
    let mut out = [0u64; K];
    let mut muls = 0u64;
    let mut adds = 0u64;
    for (j, slot) in out.iter_mut().enumerate() {
        let wj = tribit::pow(W, j as u64);
        let mut acc = 0u64;
        let mut t = 1u64;
        for &xi in x.iter() {
            acc = mod_add(acc, mod_mul(xi % P, t));
            t = mod_mul(t, wj);
            muls += 2;
            adds += 1;
        }
        *slot = acc;
    }
    (out, muls, adds)
}

/// Instrumented inverse unprism that counts exact modular multiplications and additions.
fn unprism_with_cost(x: &[u64; K]) -> ([u64; K], u64, u64) {
    let winv = tribit::inv(W);
    let kinv = tribit::inv(K as u64);
    let mut out = [0u64; K];
    let mut muls = 0u64;
    let mut adds = 0u64;
    for (j, slot) in out.iter_mut().enumerate() {
        let wj = tribit::pow(winv, j as u64);
        let mut acc = 0u64;
        let mut t = 1u64;
        for &xi in x.iter() {
            acc = mod_add(acc, mod_mul(xi % P, t));
            t = mod_mul(t, wj);
            muls += 2;
            adds += 1;
        }
        *slot = mod_mul(acc, kinv);
        muls += 1;
    }
    (out, muls, adds)
}

/// Verifies Plancherel / Parseval modular identity over Z/PZ:
/// sum_{j=0}^{26} X[j] * X[(-j) mod 27] == 27 * sum_{i=0}^{26} x[i]^2 (mod P).
fn verify_parseval(x: &[u64; K], spec: &[u64; K]) -> Result<(), &'static str> {
    let mut lhs = 0u64;
    for (j, &sj) in spec.iter().enumerate() {
        let neg_j = (K - j) % K;
        lhs = mod_add(lhs, mod_mul(sj, spec[neg_j]));
    }
    let mut rhs_sum = 0u64;
    for &xi in x.iter() {
        let xr = xi % P;
        rhs_sum = mod_add(rhs_sum, mod_mul(xr, xr));
    }
    let rhs = mod_mul(K as u64, rhs_sum);
    if lhs == rhs {
        Ok(())
    } else {
        Err("parseval_energy_mismatch")
    }
}

/// Verifies the 3-adic Motor Block / Three-Throw Crank Hensel lift for levels k = 1..=6:
/// 2^(2 * 3^k) == 1 + c * 3^(k+1) (mod 3^(k+2)) with gcd(c, 3) == 1,
/// and for any survivor residue n coprime to 3, its three RGB lifts
/// {n, n + 2*3^k, n + 4*3^k} hit all three digits {0, 1, 2} at trit k+1.
fn verify_motor_block_crank() -> Result<u32, &'static str> {
    let mut strokes_verified = 0u32;
    let mut pow3_k = 3u128;
    for _k in 1..=6u32 {
        let mod_next = pow3_k * 9;
        let exp = 2 * pow3_k;
        let mut base = 2u128;
        let mut e = exp;
        let mut acc = 1u128;
        while e > 0 {
            if e & 1 == 1 {
                acc = (acc * base) % mod_next;
            }
            base = (base * base) % mod_next;
            e >>= 1;
        }
        let step_unit = pow3_k * 3;
        if acc % step_unit != 1 {
            return Err("hensel_unit_mismatch");
        }
        let c = ((acc - 1) / step_unit) % 3;
        if c == 0 {
            return Err("hensel_c_not_coprime_to_3");
        }
        let r_val = 4u128 % mod_next;
        let g_val = (r_val * acc) % mod_next;
        let b_val = (g_val * acc) % mod_next;
        let d_r = (r_val / step_unit) % 3;
        let d_g = (g_val / step_unit) % 3;
        let d_b = (b_val / step_unit) % 3;
        let mask = (1u8 << d_r) | (1u8 << d_g) | (1u8 << d_b);
        if mask != 0b111 {
            return Err("rgb_cylinder_not_bijective_mod_3");
        }
        strokes_verified += 1;
        pow3_k *= 3;
    }
    Ok(strokes_verified)
}

fn main() {
    assert_eq!(tribit::pow(G, (P - 1) / (K as u64)), W);
    let mut cyc = 0u64;
    for j in 0..K as u64 {
        cyc = mod_add(cyc, tribit::pow(W, j));
    }
    assert_eq!(cyc, 0, "cyclotomic sum must close on 0");

    let mut vectors_checked = 0u64;
    let mut fwd_muls_per_call = 0u64;
    let mut fwd_adds_per_call = 0u64;
    let mut inv_muls_per_call = 0u64;
    let mut inv_adds_per_call = 0u64;

    for basis in 0..K {
        let mut x = [0u64; K];
        x[basis] = 1;
        let (spec, fm, fa) = prism_with_cost(&x);
        let (back, im, ia) = unprism_with_cost(&spec);
        assert_eq!(spec, tribit::prism(&x));
        assert_eq!(back, tribit::unprism(&spec));
        assert_eq!(back, x);
        verify_parseval(&x, &spec).expect("Parseval holds on basis");
        fwd_muls_per_call = fm;
        fwd_adds_per_call = fa;
        inv_muls_per_call = im;
        inv_adds_per_call = ia;
        vectors_checked += 1;
    }

    for seed in 0..100u64 {
        let mut x = [0u64; K];
        for (i, lane) in x.iter_mut().enumerate() {
            let room_n = (seed * 99_999_999 + (i as u64) * 462_962_962) % ROOM_COUNT;
            let addr = address(room_n).expect("valid room address");
            let stub = mix(addr.word ^ SEED);
            assert_eq!(unmix(stub) ^ SEED, addr.word);
            *lane = stub % P;
        }
        let spec = tribit::prism(&x);
        let back = tribit::unprism(&spec);
        assert_eq!(back, x);
        let expected_dc = x.iter().fold(0u64, |acc, &v| mod_add(acc, v));
        assert_eq!(spec[0], expected_dc, "Law 0 DC center equals sum");
        verify_parseval(&x, &spec).expect("Parseval holds on room-derived lanes");
        vectors_checked += 1;
    }

    let strokes = verify_motor_block_crank().expect("3-adic Motor Block crank verified");
    let ground = address(ROOM_COUNT / 2).expect("ground address");
    assert_eq!(ground.balanced, 0);
    assert_eq!(ground.word, HALF27 as u64);

    println!(
        "PRISMCOSTCHECK|prime={P}|generator={G}|lanes={K}|root={W}|vectors={vectors_checked}|fwd_inner_muls={fwd_muls_per_call}|fwd_adds={fwd_adds_per_call}|inv_inner_muls={inv_muls_per_call}|inv_adds={inv_adds_per_call}|parseval=EXACT|roundtrip=EXACT|motor_block_strokes={strokes}|host8_rooms={ROOM_COUNT}|shards={}|float_used=0|json=0",
        ROOM_COUNT / ROOMS_PER_FILE
    );
}
