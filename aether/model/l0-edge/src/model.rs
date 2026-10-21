//! `EdgeLevelGNN.forward` (gnn_baseline.py:43-67) as inference_server.py:24 runs it, with torch_geometric 2.8.1
//! `GCNConv` (gcn_conv.py:45-113 `gcn_norm`, 227-268 `forward`; loop.py:585-657), in Q24 integers.

use crate::features::FRAC_BITS;
use crate::weights::{CONV1_B, CONV1_W, CONV2_B, CONV2_W, EDGE1_B, EDGE1_W, EDGE2_B, EDGE2_W};
use alloc::{vec, vec::Vec};
use core::fmt;

const HIDDEN: usize = 64;
const ONE_Q24: i64 = 1 << FRAC_BITS;
const NORM_BITS: u32 = 32;
const LOGIT_BITS: u32 = 2 * FRAC_BITS;
const EXP_BITS: u32 = 62;
const ONE_EXP: u128 = 1 << EXP_BITS;
const LN2_EXP: u128 = 3_196_577_161_300_663_915;
const SIGMOID_SATURATION_Q48: u128 = 18 << LOGIT_BITS;
/// `exp_neg` is within 12 units of 2^-62 of e^-|t| (at most 18 series terms, each rounded to 1/2 unit, and
/// `LN2_EXP` within 53/1000 of a unit per octave), so its quotient is within 2^-34 of 2^24 * sigmoid(t).
/// Copied from model/l1-prototype/src/model.rs:29-31; `sigmoid_q24` and `round_near_half` through `big_cmp` below
/// are copied from lines 191-309 of that file (HEAD f67b300; see PROVENANCE.md, "Duplicated code").
const TIE_WINDOW_BITS: u32 = 32;

/// Why [`edge_scores`] gives no scores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// Edge number `edge` names a node index `>= x.len()`.
    NodeIndexOutOfRange { edge: usize },
    /// No nodes or no edges; inference_server.py answers 400 for both.
    EmptyGraph,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::NodeIndexOutOfRange { edge } => {
                write!(f, "edge {edge} names a node index out of range")
            }
            ModelError::EmptyGraph => f.write_str("the graph has no nodes or no edges"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ModelError {}

/// Sigmoid score per edge, Q24 in 0..=2^24: EdgeLevelGNN.forward as inference_server.py runs it.
/// Intermediates stay in i128 range for any i64 input while every in-degree is <= MAX_EVENTS + 1.
pub fn edge_scores(x: &[[i64; 6]], edges: &[(usize, usize)]) -> Result<Vec<i64>, ModelError> {
    if x.is_empty() || edges.is_empty() {
        return Err(ModelError::EmptyGraph);
    }
    if let Some(edge) = edges
        .iter()
        .position(|&(source, target)| source >= x.len() || target >= x.len())
    {
        return Err(ModelError::NodeIndexOutOfRange { edge });
    }
    let head = EdgeHead::new(&embed(x, edges));
    Ok(edges
        .iter()
        .map(|&(source, target)| head.score(source, target))
        .collect())
}

/// The request asolaria-score sends to :4792: nodes [actor, target], edges [[0, 1]].
pub fn score_pair(actor: &[i64; 6], target: &[i64; 6]) -> i64 {
    let head = EdgeHead::new(&embed(&[*actor, *target], &[(0, 1)]));
    head.score(0, 1)
}

/// `+p.toFixed(4)` of a Q24 score as e4 units (round half up on the exact Q24 value).
/// `p_q24` is clamped to 0..=2^24 first.
pub fn q24_to_e4(p_q24: i64) -> u32 {
    let p = i128::from(p_q24.clamp(0, ONE_Q24));
    let e4 = (p * 20_000 + i128::from(ONE_Q24)) >> (FRAC_BITS + 1);
    u32::try_from(e4).unwrap_or(10_000)
}

fn round_shift(value: i128, bits: u32) -> i128 {
    (value + (1 << (bits - 1))) >> bits
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut root = 1u128 << ((128 - n.leading_zeros()).div_ceil(2));
    loop {
        let next = (root + n / root) / 2;
        if next >= root {
            return root;
        }
        root = next;
    }
}

fn gcn_coefficient(source_degree: u64, target_degree: u64) -> i128 {
    let product = u128::from(source_degree) * u128::from(target_degree);
    let root = isqrt((1u128 << (2 * NORM_BITS + 2)) / product);
    ((root + 1) / 2) as i128
}

struct Arc {
    source: usize,
    target: usize,
    coefficient: i128,
}

fn normalized_arcs(node_count: usize, edges: &[(usize, usize)]) -> Vec<Arc> {
    let pairs: Vec<(usize, usize)> = edges
        .iter()
        .copied()
        .filter(|&(source, target)| source != target)
        .chain((0..node_count).map(|node| (node, node)))
        .collect();
    let mut in_degree = vec![0u64; node_count];
    for &(_, target) in &pairs {
        in_degree[target] += 1;
    }
    pairs
        .into_iter()
        .map(|(source, target)| Arc {
            source,
            target,
            coefficient: gcn_coefficient(in_degree[source], in_degree[target]),
        })
        .collect()
}

fn linear_q24<const IN: usize>(row: &[i128; IN], weight: &[i32; IN]) -> i128 {
    row.iter()
        .zip(weight)
        .map(|(&value, &w)| value * i128::from(w))
        .sum()
}

fn gcn_conv<const IN: usize>(
    arcs: &[Arc],
    input: &[[i128; IN]],
    weight: &[[i32; IN]; HIDDEN],
    bias: &[i32; HIDDEN],
) -> Vec<[i128; HIDDEN]> {
    let lin: Vec<[i128; HIDDEN]> = input
        .iter()
        .map(|row| core::array::from_fn(|out| round_shift(linear_q24(row, &weight[out]), FRAC_BITS)))
        .collect();
    let mut sums = vec![bias.map(|b| i128::from(b) << NORM_BITS); input.len()];
    for arc in arcs {
        for (sum, &value) in sums[arc.target].iter_mut().zip(&lin[arc.source]) {
            *sum += arc.coefficient * value;
        }
    }
    sums.into_iter()
        .map(|row| row.map(|sum| round_shift(sum, NORM_BITS)))
        .collect()
}

fn embed(x: &[[i64; 6]], edges: &[(usize, usize)]) -> Vec<[i128; HIDDEN]> {
    let arcs = normalized_arcs(x.len(), edges);
    let input: Vec<[i128; 6]> = x.iter().map(|row| row.map(i128::from)).collect();
    let conv1 = gcn_conv(&arcs, &input, &CONV1_W, &CONV1_B);
    let relu1: Vec<[i128; HIDDEN]> = conv1.into_iter().map(|row| row.map(|v| v.max(0))).collect();
    gcn_conv(&arcs, &relu1, &CONV2_W, &CONV2_B)
}

struct EdgeHead {
    source_terms: Vec<[i128; HIDDEN]>,
    target_terms: Vec<[i128; HIDDEN]>,
}

impl EdgeHead {
    fn new(embeddings: &[[i128; HIDDEN]]) -> Self {
        let weight: &[[i32; 2 * HIDDEN]; HIDDEN] = &EDGE1_W;
        let half_terms = |offset: usize| -> Vec<[i128; HIDDEN]> {
            embeddings
                .iter()
                .map(|h| {
                    core::array::from_fn(|out| {
                        h.iter()
                            .zip(&weight[out][offset..offset + HIDDEN])
                            .map(|(&value, &w)| value * i128::from(w))
                            .sum()
                    })
                })
                .collect()
        };
        EdgeHead {
            source_terms: half_terms(0),
            target_terms: half_terms(HIDDEN),
        }
    }

    fn score(&self, source: usize, target: usize) -> i64 {
        let mut logit_q48 = i128::from(EDGE2_B) << FRAC_BITS;
        let (bias, w_out): (&[i32; HIDDEN], &[i32; HIDDEN]) = (&EDGE1_B, &EDGE2_W);
        let terms = self.source_terms[source]
            .iter()
            .zip(&self.target_terms[target]);
        for (((&from_source, &from_target), &bias), &w_out) in terms.zip(bias).zip(w_out) {
            let sum = (i128::from(bias) << FRAC_BITS) + from_source + from_target;
            let hidden = round_shift(sum, FRAC_BITS).max(0);
            logit_q48 += hidden * i128::from(w_out);
        }
        sigmoid_q24(logit_q48)
    }
}

fn exp_neg(magnitude: u128) -> u128 {
    let octaves = magnitude / LN2_EXP;
    let remainder = magnitude - octaves * LN2_EXP;
    let mut sum = ONE_EXP;
    let mut term = ONE_EXP;
    let mut n: u128 = 1;
    while term != 0 {
        term = (term * remainder + (n << (EXP_BITS - 1))) / (n << EXP_BITS);
        if n % 2 == 1 {
            sum -= term;
        } else {
            sum += term;
        }
        n += 1;
    }
    match u32::try_from(octaves) {
        Ok(0) => sum,
        Ok(shift) if shift < EXP_BITS => (sum + (1 << (shift - 1))) >> shift,
        _ => 0,
    }
}

/// `1 / (1 + e^-t)` of `t` in Q48, as Q24 rounded half up: `floor(2^24 * sigmoid(t) + 1/2)` for every logit.
/// When the series quotient lies within 2^-32 of a half, [`round_near_half`] decides the side exactly.
fn sigmoid_q24(logit_q48: i128) -> i64 {
    let magnitude = logit_q48.unsigned_abs();
    if magnitude > SIGMOID_SATURATION_Q48 {
        return if logit_q48 > 0 { ONE_Q24 } else { 0 };
    }
    let decay = exp_neg(magnitude << (EXP_BITS - LOGIT_BITS));
    let denominator = ONE_EXP + decay;
    let numerator = if logit_q48 >= 0 { ONE_EXP } else { decay };
    let dividend = (numerator << (FRAC_BITS + 1)) + denominator;
    let window = 2 * denominator;
    let (rounded, remainder) = (dividend / window, dividend % window);
    let rounded = if remainder << TIE_WINDOW_BITS < window {
        round_near_half(logit_q48, magnitude, rounded)
    } else if (window - remainder) << TIE_WINDOW_BITS <= window {
        round_near_half(logit_q48, magnitude, rounded + 1)
    } else {
        rounded
    };
    i64::try_from(rounded).unwrap_or(ONE_Q24)
}

/// `upper` if `2^24 * sigmoid(t) > upper - 1/2`, else `upper - 1`. With `a = 2 upper - 1` and `b = 2^25 - a`, that is
/// `b e^|t| > a` for `t > 0` and `a e^|t| < b` for `t < 0`; equality needs a rational `e^|t|`, so `t = 0`.
fn round_near_half(logit_q48: i128, magnitude: u128, upper: u128) -> u128 {
    let a = 2 * upper - 1;
    let b = (1 << (FRAC_BITS + 1)) - a;
    let above = if logit_q48 > 0 {
        exp_exceeds(magnitude, b, a)
    } else {
        !exp_exceeds(magnitude, a, b)
    };
    if above {
        upper
    } else {
        upper - 1
    }
}

/// Whether `factor * e^(m / 2^48) > bound`, from lower and upper sums of the series of `e^(m / 2^48)` in units of
/// `2^-(64 limbs)`, doubling `limbs` until the two sums fall on the same side of `bound`.
fn exp_exceeds(m: u128, factor: u128, bound: u128) -> bool {
    let mut limbs = 2;
    loop {
        let (low, high) = exp_series_bounds(m, limbs);
        let mut target = vec![0; limbs + 2];
        target[limbs] = bound as u64;
        if big_cmp(&big_mul_small(&low, factor as u64), &target).is_gt() {
            return true;
        }
        if big_cmp(&big_mul_small(&high, factor as u64), &target).is_lt() {
            return false;
        }
        limbs *= 2;
    }
}

/// `(low, high)` with `low <= 2^(64 limbs) e^(m / 2^48) <= high`: the terms `y^n / n!` rounded down and up, and,
/// once `n + 1 >= 2y`, the remaining terms bounded by the last upper term.
fn exp_series_bounds(m: u128, limbs: usize) -> (Vec<u64>, Vec<u64>) {
    let mut one = vec![0; limbs + 2];
    one[limbs] = 1;
    let (mut low, mut high) = (one.clone(), one.clone());
    let (mut low_term, mut high_term) = (one.clone(), one);
    let mut n: u64 = 1;
    loop {
        low_term = big_scale(&low_term, m as u64, n, false);
        high_term = big_scale(&high_term, m as u64, n, true);
        big_add(&mut low, &low_term);
        big_add(&mut high, &high_term);
        let tail_bounded = u128::from(n + 1) << LOGIT_BITS >= 2 * m;
        if tail_bounded && high_term[1..].iter().all(|&limb| limb == 0) && high_term[0] <= 1 {
            big_add(&mut high, &high_term);
            return (low, high);
        }
        n += 1;
    }
}

/// `term * m / (n 2^48)`, rounded down or up, on little-endian u64 limbs of a fixed width.
fn big_scale(term: &[u64], m: u64, n: u64, round_up: bool) -> Vec<u64> {
    let mut value = big_mul_small(term, m);
    for divisor in [1 << LOGIT_BITS, n] {
        let mut remainder = 0u128;
        for limb in value.iter_mut().rev() {
            let current = (remainder << 64) | u128::from(*limb);
            *limb = (current / u128::from(divisor)) as u64;
            remainder = current % u128::from(divisor);
        }
        if round_up && remainder != 0 {
            big_add(&mut value, &[1]);
        }
    }
    value
}

fn big_mul_small(value: &[u64], factor: u64) -> Vec<u64> {
    let mut carry = 0u128;
    value
        .iter()
        .map(|&limb| {
            let product = u128::from(limb) * u128::from(factor) + carry;
            carry = product >> 64;
            product as u64
        })
        .collect()
}

fn big_add(sum: &mut [u64], value: &[u64]) {
    let mut carry = 0u128;
    for (i, limb) in sum.iter_mut().enumerate() {
        let total = u128::from(*limb) + u128::from(value.get(i).copied().unwrap_or(0)) + carry;
        *limb = total as u64;
        carry = total >> 64;
    }
}

fn big_cmp(a: &[u64], b: &[u64]) -> core::cmp::Ordering {
    a.iter().rev().cmp(b.iter().rev())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MAX_EVENTS;

    const EXPECTED: &str = include_str!("../tests/fixtures/expected-model.hbp");
    /// SIGMOID rows of the fixture: 1511 over `[-20, 20]`, then the 736 boundary rows within 2^-32 of a half.
    const SIGMOID_ROWS: usize = 2247;
    const BOUNDARY_ROWS: usize = 736;

    fn fields(line: &str) -> impl Iterator<Item = (&str, &str)> {
        line.split('|')
            .skip(1)
            .map(|field| field.split_once('=').expect("key=value"))
    }

    fn row_value(line: &str, key: &str) -> i128 {
        fields(line)
            .find(|&(k, _)| k == key)
            .map(|(_, v)| v.parse().expect("integer"))
            .expect("field present")
    }

    #[test]
    fn ln2_matches_the_exact_constant() {
        let line = EXPECTED
            .lines()
            .find(|l| l.starts_with("CONST|name=LN2_Q62|"))
            .expect("LN2_Q62 row");
        assert_eq!(row_value(line, "value"), LN2_EXP as i128);
    }

    #[test]
    fn sigmoid_matches_the_exact_reference() {
        let mut rows = 0;
        for line in EXPECTED.lines().filter(|l| l.starts_with("SIGMOID|")) {
            let logit = row_value(line, "logit_q48");
            let want = row_value(line, "q24");
            assert_eq!(i128::from(sigmoid_q24(logit)), want, "{line}");
            rows += 1;
        }
        assert_eq!(rows, SIGMOID_ROWS);
    }

    /// The rounding this crate used before the exact decision: the series quotient alone.
    fn series_only_q24(logit_q48: i128) -> i64 {
        let magnitude = logit_q48.unsigned_abs();
        if magnitude > SIGMOID_SATURATION_Q48 {
            return if logit_q48 > 0 { ONE_Q24 } else { 0 };
        }
        let decay = exp_neg(magnitude << (EXP_BITS - LOGIT_BITS));
        let denominator = ONE_EXP + decay;
        let numerator = if logit_q48 >= 0 { ONE_EXP } else { decay };
        let rounded = ((numerator << (FRAC_BITS + 1)) + denominator) / (2 * denominator);
        i64::try_from(rounded).unwrap_or(ONE_Q24)
    }

    #[test]
    fn series_rounding_alone_is_one_off_on_20_of_the_736_boundary_rows() {
        let rows: Vec<(i128, i128)> = EXPECTED
            .lines()
            .filter(|l| l.starts_with("SIGMOID|"))
            .map(|l| (row_value(l, "logit_q48"), row_value(l, "q24")))
            .collect();
        assert_eq!(rows.len(), SIGMOID_ROWS);
        let (wide, boundary) = rows.split_at(SIGMOID_ROWS - BOUNDARY_ROWS);
        let off = |rows: &[(i128, i128)]| {
            rows.iter()
                .filter(|&&(logit, want)| {
                    let diff = i128::from(series_only_q24(logit)) - want;
                    assert!(diff.abs() <= 1, "logit {logit}: {diff}");
                    diff != 0
                })
                .count()
        };
        assert_eq!((off(wide), off(boundary)), (0, 20));
    }

    #[test]
    fn sigmoid_is_monotone_and_symmetric() {
        let mut previous = 0;
        let step = 1i128 << 40;
        let mut logit = -(20i128 << 48);
        while logit <= 20i128 << 48 {
            let p = sigmoid_q24(logit);
            assert!(p >= previous, "logit {logit}");
            assert_eq!(p + sigmoid_q24(-logit), ONE_Q24, "logit {logit}");
            previous = p;
            logit += step;
        }
        assert_eq!(sigmoid_q24(0), ONE_Q24 / 2);
        assert_eq!(sigmoid_q24(i128::MAX), ONE_Q24);
        assert_eq!(sigmoid_q24(i128::MIN), 0);
    }

    #[test]
    fn exp_exceeds_doubles_the_precision_until_the_sums_decide() {
        // p / q is a convergent of e with 0 < q e - p < 17 / 10^21, which the 128-bit sums leave undecided.
        let (p, q) = (5_739_439_214_861_417_731u128, 2_111_421_691_000_680_031u128);
        let (low, high) = exp_series_bounds(1 << LOGIT_BITS, 2);
        let target = [0, 0, p as u64, 0];
        assert!(big_cmp(&big_mul_small(&low, q as u64), &target).is_le());
        assert!(big_cmp(&big_mul_small(&high, q as u64), &target).is_ge());
        assert!(exp_exceeds(1 << LOGIT_BITS, q, p));
        assert!(!exp_exceeds(1 << LOGIT_BITS, q, p + 1));
    }

    #[test]
    fn coefficients_are_rounded_inverse_square_roots() {
        assert_eq!(gcn_coefficient(1, 1), 1 << 32);
        assert_eq!(gcn_coefficient(1, 4), 1 << 31);
        assert_eq!(gcn_coefficient(2, 2), 1 << 31);
        // round(2^32 / sqrt(2)) and round(2^32 / sqrt(3)): c with (2c - 1)^2 d <= 2^66 < (2c + 1)^2 d, d = 2 and 3.
        assert_eq!(gcn_coefficient(1, 2), 3_037_000_500);
        assert_eq!(gcn_coefficient(3, 1), 2_479_700_525);
        for (c, d) in [(3_037_000_500u128, 2u128), (2_479_700_525, 3)] {
            assert!((2 * c - 1) * (2 * c - 1) * d <= 1 << 66);
            assert!((2 * c + 1) * (2 * c + 1) * d > 1 << 66);
        }
        let d = MAX_EVENTS as u64 + 1;
        let c = gcn_coefficient(d, d) as u128;
        assert!((2 * c - 1) * (2 * c - 1) * u128::from(d) * u128::from(d) <= 1 << 66);
        assert!((2 * c + 1) * (2 * c + 1) * u128::from(d) * u128::from(d) > 1 << 66);
    }

    #[test]
    fn isqrt_is_the_floor_square_root() {
        for n in (0u128..5000).chain([u128::MAX, 1 << 66, (1 << 66) - 1, 1 << 127]) {
            let r = isqrt(n);
            assert!(r * r <= n, "{n}");
            assert!((r + 1).checked_mul(r + 1).map_or(true, |s| s > n), "{n}");
        }
    }

    fn max_row_l1<const IN: usize>(weight: &[[i32; IN]]) -> u128 {
        weight
            .iter()
            .map(|row| row.iter().map(|w| u128::from(w.unsigned_abs())).sum())
            .max()
            .unwrap_or(0)
    }

    fn max_abs(values: &[i32]) -> u128 {
        values
            .iter()
            .map(|w| u128::from(w.unsigned_abs()))
            .max()
            .unwrap_or(0)
    }

    fn checked(value: Option<u128>) -> u128 {
        let value = value.expect("bound fits u128");
        assert!(value <= i128::MAX as u128, "bound {value} exceeds i128");
        value
    }

    #[test]
    fn no_intermediate_overflows_for_any_i64_input_up_to_max_events_edges() {
        // In-degree d <= MAX_EVENTS + 1; the coefficients into one node sum to <= 2^32 * sqrt(d) + d / 2.
        let degree = MAX_EVENTS as u128 + 1;
        let coefficient_sum = (isqrt(degree << (2 * NORM_BITS)) + 1) + degree.div_ceil(2);
        let edge1_l1: u128 = max_row_l1(&EDGE1_W);
        let mut magnitude: u128 = 1 << 63;
        for (weight_l1, bias) in [
            (max_row_l1(&CONV1_W), max_abs(&CONV1_B)),
            (max_row_l1(&CONV2_W), max_abs(&CONV2_B)),
        ] {
            let lin_sum = checked(magnitude.checked_mul(weight_l1));
            let lin = (lin_sum >> FRAC_BITS) + 1;
            let sum = checked(
                coefficient_sum
                    .checked_mul(lin)
                    .and_then(|v| v.checked_add(bias << NORM_BITS))
                    .and_then(|v| v.checked_add(1 << (NORM_BITS - 1))),
            );
            magnitude = (sum >> NORM_BITS) + 1;
        }
        let head_sum = checked(
            magnitude
                .checked_mul(edge1_l1)
                .and_then(|v| v.checked_add(max_abs(&EDGE1_B) << FRAC_BITS))
                .and_then(|v| v.checked_add(1 << (FRAC_BITS - 1))),
        );
        let hidden = (head_sum >> FRAC_BITS) + 1;
        let edge2_l1: u128 = EDGE2_W.iter().map(|w| u128::from(w.unsigned_abs())).sum();
        let logit = checked(
            hidden
                .checked_mul(edge2_l1)
                .and_then(|v| v.checked_add(u128::from(EDGE2_B.unsigned_abs()) << FRAC_BITS)),
        );
        assert!(
            logit < 1 << 120,
            "logit bound 2^{}",
            128 - logit.leading_zeros()
        );
    }
}
