//! `edge_scores` and `score_request` against the vendored :4793 server's MODEL with the v2 checkpoint
//! (tools/model_parity.py, torch 2.14.1+cpu, torch_geometric 2.8.1).

use l4_gslgnn::{
    edge_scores, q24_to_e4, request_nodes, score_request, ModelError, MAX_INPUT_Q24, NODE_FEATURES,
};
use std::collections::HashMap;

const EXPECTED: &str = include_str!("fixtures/expected-model.hbp");
const ONE_Q24: i64 = 1 << 24;
/// Largest |rust - torch| allowed for probs[:, 1], in Q24 units (2^-24), where both take the same mask.
const TOL: i64 = 16;

struct Graph {
    line: &'static str,
    x: Vec<[i64; NODE_FEATURES]>,
    edges: Vec<(usize, usize)>,
    p_q24: Vec<i64>,
}

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split('|')
        .skip(1)
        .map(|field| field.split_once('=').expect("key=value"))
        .collect()
}

fn list<T>(field: &str, parse: impl Fn(&str) -> T) -> Vec<T> {
    field.split(',').map(parse).collect()
}

fn rows(x: &str) -> Vec<[i64; NODE_FEATURES]> {
    x.split(';')
        .map(|row| {
            let row = list(row, |q| q.parse::<i64>().expect("q24"));
            row.try_into().expect("six features")
        })
        .collect()
}

/// floor(value * 2^24 + 1/2) of a non-negative float32 given by its bits, in integers.
fn float32_bits_to_q24(bits: u32) -> i64 {
    assert_eq!(bits >> 31, 0, "negative probability bits {bits:08x}");
    let exponent = i64::from((bits >> 23) & 0xff);
    let significand = i64::from(bits & 0x7f_ffff) | if exponent == 0 { 0 } else { 1 << 23 };
    let shift = exponent.max(1) - 126;
    if shift >= 0 {
        significand << shift
    } else if shift > -40 {
        (significand + (1 << (-shift - 1))) >> -shift
    } else {
        0
    }
}

fn graphs(label: &str) -> Vec<Graph> {
    EXPECTED
        .lines()
        .filter(|line| line.split('|').next() == Some(label))
        .map(|line| {
            let f = fields(line);
            let p_q24 = list(f["p_q24"], |q| q.parse().expect("q24"));
            let decoded = list(f["p_bits"], |b| {
                float32_bits_to_q24(u32::from_str_radix(b, 16).expect("hex"))
            });
            assert_eq!(decoded, p_q24, "{line}");
            Graph {
                line,
                x: rows(f["x"]),
                edges: list(f["edges"], |edge| {
                    let (s, t) = edge.split_once('>').expect("s>t");
                    (s.parse().expect("row"), t.parse().expect("col"))
                }),
                p_q24,
            }
        })
        .collect()
}

fn hex_text(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex byte"))
        .collect();
    String::from_utf8(bytes).expect("UTF-8")
}

#[test]
fn fixture_layout_is_the_torch_geometric_model() {
    let layout = EXPECTED
        .lines()
        .find(|l| l.starts_with("LAYOUT|"))
        .expect("LAYOUT row");
    let f = fields(layout);
    assert_eq!(f["conv"], "torch_geometric.nn.conv.gcn_conv.GCNConv");
    assert_eq!(f["renamed_keys"], "0");
    assert_eq!(f["checkpoint_keys"], "19");
    // float32 0.5: the threshold is the value gsl_gnn.py:48 initialises.
    assert_eq!(f["threshold_bits"], "3f000000");
    assert_eq!(l4_gslgnn::weights::THRESHOLD, 1 << 23);
}

#[test]
fn edge_scores_match_torch() {
    let (mut max_diff, mut equal, mut total) = (0i64, 0usize, 0usize);
    let all = graphs("GRAPH");
    for graph in &all {
        let rust = edge_scores(&graph.x, &graph.edges).expect("valid graph");
        assert_eq!(rust.len(), graph.p_q24.len(), "{}", graph.line);
        for (&got, &want) in rust.iter().zip(&graph.p_q24) {
            assert!((0..=ONE_Q24).contains(&got), "{}", graph.line);
            let diff = (got - want).abs();
            assert!(diff <= TOL, "{got} vs {want}: {}", graph.line);
            max_diff = max_diff.max(diff);
            equal += usize::from(diff == 0);
            total += 1;
        }
    }
    eprintln!(
        "GRAPH|graphs={}|edges={total}|equal={equal}|max_abs_diff_q24={max_diff}",
        all.len()
    );
    assert_eq!((all.len(), total), (125, 393));
}

#[test]
fn requests_match_torch() {
    let (mut max_diff, mut total) = (0i64, 0usize);
    let (mut lowest, mut highest) = (i64::MAX, i64::MIN);
    for line in EXPECTED.lines().filter(|l| l.starts_with("REQUEST|")) {
        let f = fields(line);
        let (pid, content) = (hex_text(f["pid_hex"]), hex_text(f["content_hex"]));
        assert_eq!(
            request_nodes(&pid, &content).to_vec(),
            rows(f["x"]),
            "{line}"
        );
        let reading = score_request(&pid, &content);
        let want: i64 = f["p_q24"].parse().expect("q24");
        let diff = (reading.score_q24 - want).abs();
        assert!(diff <= TOL, "{} vs {want}: {line}", reading.score_q24);
        assert_eq!(reading.score_e4.to_string(), f["e4"], "toFixed(4): {line}");
        max_diff = max_diff.max(diff);
        lowest = lowest.min(reading.score_q24);
        highest = highest.max(reading.score_q24);
        total += 1;
    }
    eprintln!(
        "REQUEST|requests={total}|max_abs_diff_q24={max_diff}|rust_min_q24={lowest}|rust_max_q24={highest}"
    );
    assert_eq!(total, 64);
    // Distance of every request score from the toFixed(4) boundaries of 0.5292, in Q24 units.
    let low_edge = (52_915 * ONE_Q24 + 99_999) / 100_000;
    let high_edge = 52_925 * ONE_Q24 / 100_000;
    eprintln!(
        "REQUEST|q24_above_0.52915={}|q24_below_0.52925={}",
        lowest - low_edge,
        high_edge - highest
    );
}

/// Inputs on both sides of torch's float32 attention-threshold crossing for entry (0, 1), including the 12
/// where the mask here differs from torch's (src/model.rs `attention_mask_at_the_threshold_crossings`):
/// every score agrees within `TOL`.
#[test]
fn boundary_inputs() {
    let all = graphs("BOUNDARY");
    let (mut agree_max, mut differ, mut differ_max) = (0i64, 0usize, 0i64);
    for graph in &all {
        let rust = edge_scores(&graph.x, &graph.edges).expect("valid graph")[0];
        let diff = (rust - graph.p_q24[0]).abs();
        if diff <= TOL {
            agree_max = agree_max.max(diff);
        } else {
            differ += 1;
            differ_max = differ_max.max(diff);
        }
    }
    eprintln!(
        "BOUNDARY|rows={}|within_tol={}|max_abs_diff_within_tol={agree_max}|beyond_tol={differ}|max_abs_diff_beyond_tol={differ_max}",
        all.len(),
        all.len() - differ
    );
    assert_eq!((all.len(), differ), (80, 0));
}

#[test]
fn rejected_inputs() {
    let x = [[0i64; NODE_FEATURES]; 2];
    assert_eq!(edge_scores(&x, &[]), Err(ModelError::EmptyGraph));
    assert_eq!(edge_scores(&[], &[(0, 0)]), Err(ModelError::EmptyGraph));
    assert_eq!(
        edge_scores(&x, &[(0, 1), (1, 2)]),
        Err(ModelError::NodeIndexOutOfRange { edge: 1 })
    );
    let mut wide = x;
    wide[1][3] = MAX_INPUT_Q24 + 1;
    assert_eq!(
        edge_scores(&wide, &[(0, 1)]),
        Err(ModelError::InputOutOfRange { node: 1 })
    );
    wide[1][3] = -MAX_INPUT_Q24;
    assert!(edge_scores(&wide, &[(0, 1)]).is_ok());
}

#[test]
fn extreme_inputs_stay_in_range() {
    for value in [-MAX_INPUT_Q24, -(1 << 30), -1, 0, 1, 1 << 30, MAX_INPUT_Q24] {
        let x = [
            [value; NODE_FEATURES],
            [0; NODE_FEATURES],
            [-value; NODE_FEATURES],
        ];
        let scores = edge_scores(&x, &[(0, 1), (1, 2), (2, 0), (1, 1)]).expect("in range");
        assert!(scores.iter().all(|p| (0..=ONE_Q24).contains(p)), "{value}");
    }
}

#[test]
fn to_fixed4_rounds_half_up_on_the_exact_value() {
    assert_eq!(q24_to_e4(0), 0);
    assert_eq!(q24_to_e4(ONE_Q24), 10_000);
    assert_eq!(q24_to_e4(ONE_Q24 / 2), 5_000);
    // 0.00005 * 2^24 = 838.8608: 838 rounds down, 839 up.
    assert_eq!(q24_to_e4(838), 0);
    assert_eq!(q24_to_e4(839), 1);
    assert_eq!(q24_to_e4(-5), 0);
    assert_eq!(q24_to_e4(ONE_Q24 + 5), 10_000);
}
