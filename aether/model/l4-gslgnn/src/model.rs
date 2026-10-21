//! `GSLGNN.forward` (gsl_gnn.py:171-220) in eval mode, as gslgnn_inference_server.py:21-36 builds it and
//! line 54 calls it: torch_geometric 2.8.1 `GCNConv` and `to_dense_adj` (gsl_gnn.py:17-19), in Q24 integers.

use crate::weights::{
    CLS0_B, CLS0_W, CLS3_B, CLS3_W, CLS5_B, CLS5_W, CONV1_ORIG_B, CONV1_ORIG_W, CONV1_W,
    CONV2_ORIG_B, CONV2_ORIG_W, CONV2_W, KEY_B, KEY_W, QUERY_B, QUERY_W, THRESHOLD,
};
use alloc::{vec, vec::Vec};
use core::fmt;

/// Fraction bits of every Q24 value.
pub const FRAC_BITS: u32 = 24;
/// Features per node (`node_input_dim=6`, gslgnn_inference_server.py:21).
pub const NODE_FEATURES: usize = 6;
/// Node embedding width (`hidden_dim=64`).
pub const HIDDEN: usize = 64;
/// Largest |input| in Q24 units (2^40, i.e. 65536.0) under which no intermediate can overflow.
pub const MAX_INPUT_Q24: i64 = 1 << 40;
/// Largest node count under which no intermediate can overflow.
pub const MAX_NODES: usize = 1 << 20;
/// Largest edge count under which no intermediate can overflow.
pub const MAX_EDGES: usize = 1 << 20;

const GSL_HIDDEN: usize = 32;
const CLS_HIDDEN: usize = 32;
const ONE_Q24: i64 = 1 << FRAC_BITS;
/// float32(sqrt(32)) = 11863283 * 2^-21: torch divides the float32 scores by the float32 of the Python
/// scalar `keys.shape[1] ** 0.5` (gsl_gnn.py:71).
const SQRT32_F32_SIGNIFICAND: i128 = 11_863_283;
const SQRT32_F32_SHIFT: u32 = 21;
/// Adjacency entries are held in Q25, so `0.5 * sigmoid` of a Q24 sigmoid is exact.
const ADJ_ONE: i128 = 1 << (FRAC_BITS + 1);
const NORM_BITS: u32 = 32;
const EXP_BITS: u32 = 62;
const ONE_EXP: u128 = 1 << EXP_BITS;
const LN2_EXP: u128 = 3_196_577_161_300_663_915;
const SIGMOID_SATURATION_Q48: u128 = 18 << (2 * FRAC_BITS);

/// Why [`edge_scores`] gives no scores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// No nodes or no edges; gslgnn_inference_server.py:49-50 answers 400 for no edges.
    EmptyGraph,
    /// Edge number `edge` names a node index `>= x.len()`; torch raises IndexError there.
    NodeIndexOutOfRange { edge: usize },
    /// Node number `node` has a feature outside `-MAX_INPUT_Q24..=MAX_INPUT_Q24`.
    InputOutOfRange { node: usize },
    /// More than [`MAX_NODES`] nodes.
    TooManyNodes,
    /// More than [`MAX_EDGES`] edges.
    TooManyEdges,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::EmptyGraph => f.write_str("the graph has no nodes or no edges"),
            ModelError::NodeIndexOutOfRange { edge } => {
                write!(f, "edge {edge} names a node index out of range")
            }
            ModelError::InputOutOfRange { node } => {
                write!(
                    f,
                    "node {node} has a feature beyond {MAX_INPUT_Q24} Q24 units"
                )
            }
            ModelError::TooManyNodes => write!(f, "more than {MAX_NODES} nodes"),
            ModelError::TooManyEdges => write!(f, "more than {MAX_EDGES} edges"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ModelError {}

/// `probs[:, 1]` per edge, Q24 in 0..=2^24: `GSLGNN(6, 64).forward(x, edge_index)` in eval mode.
/// `x` holds one row of Q24 features per node; `edges` the `(row, col)` pairs of `edge_index`.
pub fn edge_scores(
    x: &[[i64; NODE_FEATURES]],
    edges: &[(usize, usize)],
) -> Result<Vec<i64>, ModelError> {
    check(x, edges)?;
    let input: Vec<[i128; NODE_FEATURES]> = x.iter().map(|row| row.map(i128::from)).collect();
    let learned = LearnedGraph::new(&input, edges);
    let h_learned = learned.encode(&input);
    let h_original = encode_with_original_graph(&input, edges);
    let head = EdgeHead::new(&h_learned, &h_original);
    Ok(edges
        .iter()
        .map(|&(row, col)| head.score(row, col))
        .collect())
}

/// `+p.toFixed(4)` of a Q24 score as e4 units (round half up on the exact Q24 value), as
/// asolaria-score.mjs:128 applies it to `scores[0]`. `p_q24` is clamped to 0..=2^24 first.
pub fn q24_to_e4(p_q24: i64) -> u32 {
    let p = i128::from(p_q24.clamp(0, ONE_Q24));
    let e4 = (p * 20_000 + i128::from(ONE_Q24)) >> (FRAC_BITS + 1);
    u32::try_from(e4).unwrap_or(10_000)
}

fn check(x: &[[i64; NODE_FEATURES]], edges: &[(usize, usize)]) -> Result<(), ModelError> {
    if x.is_empty() || edges.is_empty() {
        return Err(ModelError::EmptyGraph);
    }
    if x.len() > MAX_NODES {
        return Err(ModelError::TooManyNodes);
    }
    if edges.len() > MAX_EDGES {
        return Err(ModelError::TooManyEdges);
    }
    if let Some(edge) = edges
        .iter()
        .position(|&(row, col)| row >= x.len() || col >= x.len())
    {
        return Err(ModelError::NodeIndexOutOfRange { edge });
    }
    if let Some(node) = x
        .iter()
        .position(|row| row.iter().any(|v| v.unsigned_abs() > MAX_INPUT_Q24 as u64))
    {
        return Err(ModelError::InputOutOfRange { node });
    }
    Ok(())
}

fn round_shift(value: i128, bits: u32) -> i128 {
    (value + (1 << (bits - 1))) >> bits
}

/// floor(numerator / denominator + 1/2) for denominator > 0.
fn div_round(numerator: i128, denominator: i128) -> i128 {
    (2 * numerator + denominator).div_euclid(2 * denominator)
}

/// `nn.Linear` with bias: Q24 inputs, one round half up to Q24.
fn linear_q24<const IN: usize, const OUT: usize>(
    input: &[i128; IN],
    weight: &[[i32; IN]; OUT],
    bias: &[i32; OUT],
) -> [i128; OUT] {
    core::array::from_fn(|out| {
        let sum = input
            .iter()
            .zip(&weight[out])
            .fold(i128::from(bias[out]) << FRAC_BITS, |acc, (&v, &w)| {
                acc + v * i128::from(w)
            });
        round_shift(sum, FRAC_BITS)
    })
}

/// torch_geometric `Linear(bias=False)`, the `lin` of a `GCNConv`: one round half up to Q24.
fn linear_no_bias_q24<const IN: usize, const OUT: usize>(
    input: &[i128; IN],
    weight: &[[i32; IN]; OUT],
) -> [i128; OUT] {
    core::array::from_fn(|out| {
        let sum: i128 = input
            .iter()
            .zip(&weight[out])
            .map(|(&v, &w)| v * i128::from(w))
            .sum();
        round_shift(sum, FRAC_BITS)
    })
}

fn relu<const N: usize>(values: [i128; N]) -> [i128; N] {
    values.map(|v| v.max(0))
}

/// `GraphStructureLearner.forward` (gsl_gnn.py:50-94) with the original edges, then the self loops and row
/// normalisation of gsl_gnn.py:190-195. Rows are rebuilt on demand; entries are Q25.
struct LearnedGraph {
    queries: Vec<[i128; GSL_HIDDEN]>,
    keys: Vec<[i128; GSL_HIDDEN]>,
    /// `col` of every edge, grouped by `row`: `to_dense_adj` sums duplicates (_to_dense_adj.py:89-97).
    out_edges: Vec<Vec<usize>>,
    threshold_q24: i64,
}

impl LearnedGraph {
    fn new(input: &[[i128; NODE_FEATURES]], edges: &[(usize, usize)]) -> Self {
        let mut out_edges = vec![Vec::new(); input.len()];
        for &(row, col) in edges {
            out_edges[row].push(col);
        }
        LearnedGraph {
            queries: input
                .iter()
                .map(|x| linear_q24(x, &QUERY_W, &QUERY_B))
                .collect(),
            keys: input
                .iter()
                .map(|x| linear_q24(x, &KEY_W, &KEY_B))
                .collect(),
            out_edges,
            threshold_q24: threshold_attention_q24(),
        }
    }

    /// Row `i` of `0.5 * original_adj + 0.5 * sparse_attention + I` in Q25 (gsl_gnn.py:66-91, 191).
    fn row(&self, i: usize) -> Vec<i128> {
        let query = &self.queries[i];
        let mut row: Vec<i128> = self
            .keys
            .iter()
            .map(|key| half_sparse_attention_q25(query, key, self.threshold_q24))
            .collect();
        for &col in &self.out_edges[i] {
            row[col] += ADJ_ONE / 2;
        }
        row[i] += ADJ_ONE;
        row
    }

    /// `torch.matmul(learned_adj, h)` with `learned_adj` divided by its row sums (gsl_gnn.py:194-195):
    /// each output is the exact row-weighted mean, rounded half up once.
    fn propagate<const F: usize>(&self, values: &[[i128; F]]) -> Vec<[i128; F]> {
        (0..values.len())
            .map(|i| {
                let row = self.row(i);
                let degree: i128 = row.iter().sum();
                let mut sums = [0i128; F];
                for (&weight, value) in row.iter().zip(values) {
                    for (sum, &v) in sums.iter_mut().zip(value) {
                        *sum += weight * v;
                    }
                }
                sums.map(|sum| div_round(sum, degree))
            })
            .collect()
    }

    /// `encode_with_learned_graph` (gsl_gnn.py:137-154): `conv1.lin` and `conv2.lin` have no bias in
    /// torch_geometric's `GCNConv`; dropout is off in eval.
    fn encode(&self, input: &[[i128; NODE_FEATURES]]) -> Vec<[i128; HIDDEN]> {
        let hidden: Vec<[i128; HIDDEN]> = self
            .propagate(input)
            .iter()
            .map(|h| relu(linear_no_bias_q24(h, &CONV1_W)))
            .collect();
        self.propagate(&hidden)
            .iter()
            .map(|h| linear_no_bias_q24(h, &CONV2_W))
            .collect()
    }
}

/// `sigmoid(threshold)` (gsl_gnn.py:77) in Q24: floor(2^24 / (1 + e^-0.5) + 1/2) = 10443135, the value of
/// torch's float32 result 0x3f1f597f.
fn threshold_attention_q24() -> i64 {
    sigmoid_q24(i128::from(THRESHOLD) << FRAC_BITS)
}

/// `0.5 * sparse_attention[i, j]` in Q25, which is the Q24 integer of `attention = sigmoid(q . k / sqrt(32))`
/// where `attention > sigmoid(threshold)` and 0 elsewhere (gsl_gnn.py:71-80, 90). Both sides of the
/// comparison are rounded to Q24 first; in [0.5, 1), where `sigmoid(threshold)` lies, the float32 values
/// torch compares are exactly the Q24 grid.
fn half_sparse_attention_q25(
    query: &[i128; GSL_HIDDEN],
    key: &[i128; GSL_HIDDEN],
    threshold_q24: i64,
) -> i128 {
    let score_q48: i128 = query.iter().zip(key).map(|(&q, &k)| q * k).sum();
    let z_q48 = div_round(score_q48 << SQRT32_F32_SHIFT, SQRT32_F32_SIGNIFICAND);
    let attention = sigmoid_q24(z_q48);
    if attention > threshold_q24 {
        i128::from(attention)
    } else {
        0
    }
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut root = 1u128 << (128 - n.leading_zeros()).div_ceil(2);
    loop {
        let next = (root + n / root) / 2;
        if next >= root {
            return root;
        }
        root = next;
    }
}

/// round(2^32 / sqrt(source_degree * target_degree)).
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

/// `gcn_norm` (gcn_conv.py:99-113): existing self loops dropped and one added per node
/// (`add_remaining_self_loops`, weight 1), duplicates kept, degree at the target index.
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

/// torch_geometric `GCNConv.forward` (gcn_conv.py:227-268): `lin`, then the normalised sum over arcs
/// into each target, then `bias`; one rounding for `lin` and one for the output.
fn gcn_conv<const IN: usize>(
    arcs: &[Arc],
    input: &[[i128; IN]],
    weight: &[[i32; IN]; HIDDEN],
    bias: &[i32; HIDDEN],
) -> Vec<[i128; HIDDEN]> {
    let lin: Vec<[i128; HIDDEN]> = input
        .iter()
        .map(|row| linear_no_bias_q24(row, weight))
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

/// `encode_with_original_graph` (gsl_gnn.py:156-163); dropout is off in eval.
fn encode_with_original_graph(
    input: &[[i128; NODE_FEATURES]],
    edges: &[(usize, usize)],
) -> Vec<[i128; HIDDEN]> {
    let arcs = normalized_arcs(input.len(), edges);
    let conv1: Vec<[i128; HIDDEN]> = gcn_conv(&arcs, input, &CONV1_ORIG_W, &CONV1_ORIG_B)
        .into_iter()
        .map(relu)
        .collect();
    gcn_conv(&arcs, &conv1, &CONV2_ORIG_W, &CONV2_ORIG_B)
}

/// `edge_classifier` over `cat(h_learned[row], h_learned[col], h_original[row], h_original[col])`
/// (gsl_gnn.py:128-135, 204-209), then `softmax(logits)[:, 1]` (gsl_gnn.py:211-214).
struct EdgeHead {
    /// Per node, the products of its embeddings with the four 64-column blocks of `edge_classifier.0`,
    /// unrounded Q48: [learned as row, learned as col, original as row, original as col].
    terms: [Vec<[i128; HIDDEN]>; 4],
}

impl EdgeHead {
    fn new(h_learned: &[[i128; HIDDEN]], h_original: &[[i128; HIDDEN]]) -> Self {
        let block = |embeddings: &[[i128; HIDDEN]], offset: usize| -> Vec<[i128; HIDDEN]> {
            embeddings
                .iter()
                .map(|h| {
                    core::array::from_fn(|out| {
                        h.iter()
                            .zip(&CLS0_W[out][offset..offset + HIDDEN])
                            .map(|(&v, &w)| v * i128::from(w))
                            .sum()
                    })
                })
                .collect()
        };
        EdgeHead {
            terms: [
                block(h_learned, 0),
                block(h_learned, HIDDEN),
                block(h_original, 2 * HIDDEN),
                block(h_original, 3 * HIDDEN),
            ],
        }
    }

    fn score(&self, row: usize, col: usize) -> i64 {
        let [learned_row, learned_col, original_row, original_col] = &self.terms;
        let hidden1: [i128; HIDDEN] = core::array::from_fn(|out| {
            let sum = (i128::from(CLS0_B[out]) << FRAC_BITS)
                + learned_row[row][out]
                + learned_col[col][out]
                + original_row[row][out]
                + original_col[col][out];
            round_shift(sum, FRAC_BITS).max(0)
        });
        let hidden2: [i128; CLS_HIDDEN] = relu(linear_q24(&hidden1, &CLS3_W, &CLS3_B));
        let logit_q48 = |class: usize| -> i128 {
            hidden2
                .iter()
                .zip(&CLS5_W[class])
                .fold(i128::from(CLS5_B[class]) << FRAC_BITS, |acc, (&v, &w)| {
                    acc + v * i128::from(w)
                })
        };
        sigmoid_q24(logit_q48(1) - logit_q48(0))
    }
}

/// e^(-magnitude / 2^62) in Q62, range-reduced by ln 2.
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

/// floor(2^24 / (1 + e^-t) + 1/2), t = logit_q48 / 2^48; for two classes `softmax(logits)[1]` is the
/// sigmoid of `logits[1] - logits[0]`.
fn sigmoid_q24(logit_q48: i128) -> i64 {
    let magnitude = logit_q48.unsigned_abs();
    if magnitude > SIGMOID_SATURATION_Q48 {
        return if logit_q48 > 0 { ONE_Q24 } else { 0 };
    }
    let decay = exp_neg(magnitude << (EXP_BITS - 2 * FRAC_BITS));
    let denominator = ONE_EXP + decay;
    let numerator = if logit_q48 >= 0 { ONE_EXP } else { decay };
    let rounded = ((numerator << (FRAC_BITS + 1)) + denominator) / (2 * denominator);
    i64::try_from(rounded).unwrap_or(ONE_Q24)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATH: &str = include_str!("../tests/fixtures/expected-math.hbp");
    const MODEL: &str = include_str!("../tests/fixtures/expected-model.hbp");

    fn field<'a>(line: &'a str, key: &str) -> &'a str {
        line.split('|')
            .skip(1)
            .map(|f| f.split_once('=').expect("key=value"))
            .find(|&(k, _)| k == key)
            .map(|(_, v)| v)
            .expect("field present")
    }

    fn int(line: &str, key: &str) -> i128 {
        field(line, key).parse().expect("integer")
    }

    #[test]
    fn constants_match_the_referee() {
        let row = |name: &str| {
            MATH.lines()
                .find(|l| l.starts_with(&format!("CONST|name={name}|")))
                .expect("CONST row")
        };
        assert_eq!(int(row("LN2_Q62"), "value"), LN2_EXP as i128);
        let sqrt32 = row("SQRT32_F32");
        assert_eq!(int(sqrt32, "significand"), SQRT32_F32_SIGNIFICAND);
        assert_eq!(int(sqrt32, "shift"), i128::from(SQRT32_F32_SHIFT));
    }

    /// `sigmoid(threshold)` here equals torch's float32 `torch.sigmoid(gsl.threshold)` (LAYOUT row).
    #[test]
    fn threshold_attention_is_torchs_float32_value() {
        let layout = MODEL
            .lines()
            .find(|l| l.starts_with("LAYOUT|"))
            .expect("LAYOUT row");
        let bits = u32::from_str_radix(field(layout, "sigmoid_threshold_bits"), 16).expect("hex");
        // A float32 in [0.5, 1): significand * 2^-24 exactly.
        assert_eq!(bits >> 23, 0x7e, "{bits:08x}");
        let q24 = i64::from(bits & 0x7f_ffff | 1 << 23);
        assert_eq!(threshold_attention_q24(), q24);
        assert_eq!(q24, 10_443_135);
    }

    #[test]
    fn sigmoid_matches_the_exact_reference() {
        let mut rows = 0;
        for line in MATH.lines().filter(|l| l.starts_with("SIGMOID|")) {
            let want = int(line, "q24");
            assert_eq!(
                i128::from(sigmoid_q24(int(line, "logit_q48"))),
                want,
                "{line}"
            );
            rows += 1;
        }
        assert!(rows >= 1000, "{rows} sigmoid rows");
    }

    #[test]
    fn sigmoid_is_monotone_and_symmetric() {
        let mut previous = 0;
        let step = 1i128 << 40;
        let mut logit = -(20i128 << 48);
        while logit <= 20i128 << 48 {
            let p = sigmoid_q24(logit);
            assert!(p >= previous, "logit {logit}");
            let sum = p + sigmoid_q24(-logit);
            assert!(sum == ONE_Q24 || sum == ONE_Q24 + 1, "logit {logit}: {sum}");
            previous = p;
            logit += step;
        }
        assert_eq!(sigmoid_q24(0), ONE_Q24 / 2);
        assert_eq!(sigmoid_q24(i128::MAX), ONE_Q24);
        assert_eq!(sigmoid_q24(i128::MIN), 0);
    }

    #[test]
    fn coefficients_are_rounded_inverse_square_roots() {
        assert_eq!(gcn_coefficient(1, 1), 1 << 32);
        assert_eq!(gcn_coefficient(1, 4), 1 << 31);
        assert_eq!(gcn_coefficient(2, 2), 1 << 31);
        // round(2^32 / sqrt(2)) and round(2^32 / sqrt(3)), from integer isqrt and a half test.
        assert_eq!(gcn_coefficient(1, 2), 3_037_000_500);
        assert_eq!(gcn_coefficient(3, 1), 2_479_700_525);
        let d = MAX_EDGES as u64 + 1;
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

    #[test]
    fn div_round_is_round_half_up() {
        for (n, d, want) in [
            (5, 2, 3),
            (-5, 2, -2),
            (7, 2, 4),
            (-7, 2, -3),
            (1, 3, 0),
            (-2, 3, -1),
        ] {
            assert_eq!(div_round(n, d), want, "{n}/{d}");
        }
    }

    fn mask(line: &str) -> String {
        let x: Vec<[i128; NODE_FEATURES]> = field(line, "x")
            .split(';')
            .map(|row| {
                let row: Vec<i128> = row.split(',').map(|q| q.parse().expect("q24")).collect();
                row.try_into().expect("six features")
            })
            .collect();
        let graph = LearnedGraph::new(&x, &[]);
        let mut mask = String::new();
        for i in 0..x.len() {
            for (j, value) in graph.row(i).into_iter().enumerate() {
                let attention = value - if i == j { ADJ_ONE } else { 0 };
                mask.push(if attention > 0 { '1' } else { '0' });
            }
        }
        mask
    }

    /// The attention mask torch computed (`attention > sigmoid(threshold)`, row-major) for every
    /// fixture graph equals the mask decided here.
    #[test]
    fn attention_mask_matches_torch() {
        let (mut graphs, mut entries, mut set) = (0, 0, 0);
        for line in MODEL.lines().filter(|l| l.starts_with("GRAPH|")) {
            let mask = mask(line);
            assert_eq!(mask, field(line, "mask"), "{line}");
            graphs += 1;
            entries += mask.len();
            set += mask.matches('1').count();
        }
        eprintln!("MASK|graphs={graphs}|entries={entries}|set={set}");
        assert_eq!(graphs, 125);
        // Request inputs (byte / 255): no entry is set, in torch or here.
        let mut requests = 0;
        for line in MODEL.lines().filter(|l| l.starts_with("REQUEST|")) {
            assert_eq!(mask(line), "0000", "{line}");
            assert_eq!(field(line, "mask"), "0000", "{line}");
            requests += 1;
        }
        assert_eq!(requests, 64);
    }

    /// At inputs one float32 step apart across torch's crossing of entry (0, 1), the mask here and
    /// torch's can disagree where their attention values differ by rounding; the measured count is fixed here.
    #[test]
    fn attention_mask_at_the_threshold_crossings() {
        let (mut rows, mut differ) = (0, Vec::new());
        for line in MODEL.lines().filter(|l| l.starts_with("BOUNDARY|")) {
            if mask(line) != field(line, "mask") {
                differ.push(field(line, "case"));
            }
            rows += 1;
        }
        eprintln!(
            "MASK_BOUNDARY|rows={rows}|differ={}|cases={}",
            differ.len(),
            differ.join(",")
        );
        assert_eq!(rows, 80);
        // Measured: 12 rows, from 1 float32 step before torch's crossing to 4 steps after it.
        assert_eq!(differ.len(), 12);
        for case in differ {
            let (_, step) = case.split_once(':').expect("search:step");
            let step: i32 = step.parse().expect("step");
            assert!((-1..=4).contains(&step), "{case}");
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

    /// Magnitude bound of a Q24 linear layer output from a Q24 input bound, and its Q48 accumulator.
    fn linear_bound(input: u128, l1: u128, bias: u128) -> (u128, u128) {
        let sum = checked(
            input
                .checked_mul(l1)
                .and_then(|v| v.checked_add(bias << FRAC_BITS))
                .and_then(|v| v.checked_add(1 << (FRAC_BITS - 1))),
        );
        (sum, (sum >> FRAC_BITS) + 1)
    }

    #[test]
    fn no_intermediate_overflows_within_the_documented_bounds() {
        let input = MAX_INPUT_Q24 as u128;
        // Learned graph: |query|, |key|, the Q48 score and its scaled forms.
        let (_, query) = linear_bound(input, max_row_l1(&QUERY_W), max_abs(&QUERY_B));
        let (_, key) = linear_bound(input, max_row_l1(&KEY_W), max_abs(&KEY_B));
        let score = checked(
            query
                .checked_mul(key)
                .and_then(|v| v.checked_mul(GSL_HIDDEN as u128)),
        );
        checked(
            score
                .checked_mul(1 << SQRT32_F32_SHIFT)
                .and_then(|v| v.checked_mul(2)),
        );
        // Row sums: every entry is at most (edges + 1) / 2 + 1 in Q25 units of ADJ_ONE.
        let degree = checked(((MAX_NODES + MAX_EDGES) as u128 + 2).checked_mul(ADJ_ONE as u128));
        // The learned propagation is a weighted mean, so its outputs stay within its inputs' bound.
        checked(degree.checked_mul(input).and_then(|v| v.checked_mul(2)));
        let (_, hidden) = linear_bound(input, max_row_l1(&CONV1_W), 0);
        checked(degree.checked_mul(hidden).and_then(|v| v.checked_mul(2)));
        let (_, learned) = linear_bound(hidden, max_row_l1(&CONV2_W), 0);
        // Original graph: in-degree d <= MAX_EDGES + 1; coefficients into one node sum to
        // <= 2^32 * sqrt(d) + d / 2.
        let d = MAX_EDGES as u128 + 1;
        let coefficient_sum = (isqrt(d << (2 * NORM_BITS)) + 1) + d.div_ceil(2);
        let mut original = input;
        for (l1, bias) in [
            (max_row_l1(&CONV1_ORIG_W), max_abs(&CONV1_ORIG_B)),
            (max_row_l1(&CONV2_ORIG_W), max_abs(&CONV2_ORIG_B)),
        ] {
            let (_, lin) = linear_bound(original, l1, 0);
            let sum = checked(
                coefficient_sum
                    .checked_mul(lin)
                    .and_then(|v| v.checked_add(bias << NORM_BITS))
                    .and_then(|v| v.checked_add(1 << (NORM_BITS - 1))),
            );
            original = (sum >> NORM_BITS) + 1;
        }
        // Edge head: two learned and two original embeddings per edge.
        let embedding = learned.max(original);
        let (_, hidden1) = linear_bound(embedding, max_row_l1(&CLS0_W), max_abs(&CLS0_B));
        let (_, hidden2) = linear_bound(hidden1, max_row_l1(&CLS3_W), max_abs(&CLS3_B));
        let (logit, _) = linear_bound(hidden2, max_row_l1(&CLS5_W), max_abs(&CLS5_B));
        let difference = checked(logit.checked_mul(2));
        assert!(
            difference < 1 << 120,
            "logit bound 2^{}",
            128 - difference.leading_zeros()
        );
    }
}
