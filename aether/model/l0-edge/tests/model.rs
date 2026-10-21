//! `model` against the vendored inference_server.py MODEL (tools/model_parity.py, torch 2.14.1+cpu,
//! torch_geometric 2.8.1) and the features of tests/fixtures/events.hbp.

use l0_edge::event::{parse_events, MAX_EVENTS};
use l0_edge::features::{extract_node_features, model_input_q24, NodeFeatures};
use l0_edge::model::{edge_scores, q24_to_e4, score_pair, ModelError};
use std::collections::HashMap;

const EXPECTED: &str = include_str!("fixtures/expected-model.hbp");
const EVENTS: &str = include_str!("fixtures/events.hbp");
const ONE_Q24: i64 = 1 << 24;
/// Largest |rust - torch| allowed, in Q24 units (2^-24): 32 / 2^24 = 2^-19; the fixture's largest
/// difference is 6 over the PAIR and GRAPH rows and 15 over the REQUEST rows (case `search`).
const TOL: i64 = 32;
/// REQUEST rows: 2 fixed pairs and the random pairs with a torch score near a toFixed(4) midpoint.
const REQUEST_ROWS: usize = 3338;
/// Random REQUEST rows where `q24_to_e4` differs from `+score.toFixed(4)` of torch (Q24 inputs,
/// build_node_features inputs), and where torch's two inputs give different e4 values.
const E4_DIFFERENCES: (usize, usize, usize) = (34, 38, 10);
/// score_pair of the fixture's request pairs; torch gives the same Q24 values (float32 32701a2b, 3f7ffff6).
const BENIGN_Q24: i64 = 0;
const BENIGN_E4: u32 = 0;
const ROGUE_Q24: i64 = 16_777_206;
const ROGUE_E4: u32 = 10_000;

struct Case<'a> {
    line: &'a str,
    actor: Option<String>,
    target: Option<String>,
    x: Vec<[i64; 6]>,
    edges: Vec<(usize, usize)>,
    output_bits: Vec<u32>,
    torch_q24: Vec<i64>,
}

fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("escape");
            out.push(u8::from_str_radix(hex, 16).expect("hex"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("utf-8")
}

fn list<T>(field: &str, parse: impl Fn(&str) -> T) -> Vec<T> {
    field.split(',').map(parse).collect()
}

fn cases() -> Vec<Case<'static>> {
    let mut cases = Vec::new();
    for line in EXPECTED.lines() {
        if !(line.starts_with("PAIR|") || line.starts_with("GRAPH|")) {
            continue;
        }
        let mut case = Case {
            line,
            actor: None,
            target: None,
            x: Vec::new(),
            edges: Vec::new(),
            output_bits: Vec::new(),
            torch_q24: Vec::new(),
        };
        for field in line.split('|').skip(1) {
            let (key, value) = field.split_once('=').expect("key=value");
            match key {
                "actor" => case.actor = Some(unescape(value)),
                "target" => case.target = Some(unescape(value)),
                "case" => {}
                "x" => {
                    case.x = value
                        .split(';')
                        .map(|row| {
                            let row = list(row, |q| q.parse::<i64>().expect("q24"));
                            row.try_into().expect("six features")
                        })
                        .collect();
                }
                "edges" => {
                    case.edges = list(value, |edge| {
                        let (s, t) = edge.split_once('>').expect("s>t");
                        (s.parse().expect("source"), t.parse().expect("target"))
                    });
                }
                "out_bits" => {
                    case.output_bits = list(value, |b| u32::from_str_radix(b, 16).expect("bits"));
                }
                "q24" => case.torch_q24 = list(value, |q| q.parse().expect("q24")),
                "upstream_bits" => {}
                _ => panic!("unknown key {key}"),
            }
        }
        cases.push(case);
    }
    cases
}

/// floor(v * 2^24 + 1/2) of the float32 with these bits, in integers.
fn float32_bits_to_q24(bits: u32) -> i64 {
    let exponent = i32::try_from((bits >> 23) & 0xFF).expect("exponent");
    let fraction = i128::from(bits & 0x7F_FFFF);
    let (mantissa, scale) = if exponent == 0 {
        (fraction, -149 + 24)
    } else {
        (fraction | 1 << 23, exponent - 150 + 24)
    };
    let magnitude = if scale >= 0 {
        mantissa << scale
    } else if scale > -100 {
        (mantissa + (1 << (-scale - 1))) >> -scale
    } else {
        0
    };
    assert_eq!(bits >> 31, 0, "scores are non-negative");
    i64::try_from(magnitude).expect("q24")
}

#[test]
fn edge_scores_match_the_server_model_within_tol() {
    let cases = cases();
    let (mut scores, mut max_diff, mut exact) = (0, 0, 0);
    for case in &cases {
        let rust = edge_scores(&case.x, &case.edges).expect("valid graph");
        assert_eq!(rust.len(), case.torch_q24.len(), "{}", case.line);
        for ((&got, &torch), &bits) in rust.iter().zip(&case.torch_q24).zip(&case.output_bits) {
            assert_eq!(float32_bits_to_q24(bits), torch, "{}", case.line);
            assert!((0..=ONE_Q24).contains(&got));
            let diff = (got - torch).abs();
            assert!(
                diff <= TOL,
                "|{got} - {torch}| = {diff} > {TOL}: {}",
                case.line
            );
            max_diff = max_diff.max(diff);
            exact += usize::from(diff == 0);
            scores += 1;
        }
    }
    assert!(
        cases.len() >= 400 && scores >= 3000,
        "{} cases, {scores} scores",
        cases.len()
    );
    eprintln!("max |rust - torch| = {max_diff} Q24 units; {exact} of {scores} scores equal");
    assert_eq!((max_diff, exact, scores), (6, 2331, 3273));
}

struct Request {
    line: &'static str,
    case: &'static str,
    x: [[i64; 6]; 2],
    torch_bits: u32,
    upstream_bits: u32,
}

/// model_input_q24 of `edge_volume,avg_risk_centi,max_risk,failure_rate_milli,trust_tier`.
fn request_input(text: &str) -> [i64; 6] {
    let values = list(text, |v| v.parse::<u64>().expect("feature"));
    let [edge_volume, avg_risk_centi, max_risk, failure_rate_milli, trust_tier] = values[..] else {
        panic!("five features: {text}");
    };
    model_input_q24(&NodeFeatures {
        id: String::new(),
        node_type: String::new(),
        edge_volume,
        avg_risk_centi,
        max_risk: u32::try_from(max_risk).expect("max_risk"),
        failure_rate_milli,
        online: 1,
        trust_tier: u8::try_from(trust_tier).expect("trust_tier"),
    })
}

fn requests() -> Vec<Request> {
    EXPECTED
        .lines()
        .filter(|line| line.starts_with("REQUEST|"))
        .map(|line| {
            let field = |key: &str| {
                line.split('|')
                    .skip(1)
                    .find_map(|f| f.strip_prefix(key)?.strip_prefix('='))
                    .unwrap_or_else(|| panic!("{key} in {line}"))
            };
            let (actor, target) = field("features").split_once(';').expect("two nodes");
            let bits = |key: &str| u32::from_str_radix(field(key), 16).expect("bits");
            Request {
                line,
                case: field("case"),
                x: [request_input(actor), request_input(target)],
                torch_bits: bits("out_bits"),
                upstream_bits: bits("upstream_bits"),
            }
        })
        .collect()
}

/// `+v.toFixed(4)` in e4 units of the float32 `v` (0 <= v <= 1) with these bits.
fn to_fixed4_of_float32(bits: u32) -> u32 {
    let exponent = (bits >> 23) & 0xFF;
    assert!(
        bits >> 31 == 0 && exponent <= 127,
        "score in 0..=1: {bits:08x}"
    );
    let fraction = u128::from(bits & 0x7F_FFFF);
    let (mantissa, shift) = if exponent == 0 {
        (fraction, 149)
    } else {
        (fraction | 1 << 23, 150 - exponent)
    };
    if shift >= 64 {
        return 0;
    }
    u32::try_from((mantissa * 20_000 + (1 << shift)) >> (shift + 1)).expect("e4")
}

#[test]
fn request_rows_agree_within_tol_and_e4_differences_are_counted() {
    let requests = requests();
    let (mut max_diff, mut rust_vs_torch, mut rust_vs_upstream, mut torch_vs_upstream) =
        (0, 0, 0, 0);
    for request in &requests {
        let rust = score_pair(&request.x[0], &request.x[1]);
        let (torch, upstream) = (
            float32_bits_to_q24(request.torch_bits),
            float32_bits_to_q24(request.upstream_bits),
        );
        let diff = (rust - torch).abs().max((rust - upstream).abs());
        assert!(diff <= TOL, "{diff} > {TOL}: {}", request.line);
        max_diff = max_diff.max(diff);
        let rust_e4 = q24_to_e4(rust);
        let torch_e4 = to_fixed4_of_float32(request.torch_bits);
        let upstream_e4 = to_fixed4_of_float32(request.upstream_bits);
        for (a, b) in [
            (rust_e4, torch_e4),
            (rust_e4, upstream_e4),
            (torch_e4, upstream_e4),
        ] {
            assert!(a.abs_diff(b) <= 1, "{}", request.line);
        }
        if request.case.parse::<u32>().is_ok() {
            rust_vs_torch += usize::from(rust_e4 != torch_e4);
            rust_vs_upstream += usize::from(rust_e4 != upstream_e4);
            torch_vs_upstream += usize::from(torch_e4 != upstream_e4);
        }
        if request.case == "e4_midpoint" {
            assert_eq!((rust, rust_e4), (16_773_022, 9998));
            assert_eq!((torch, torch_e4, upstream_e4), (16_773_021, 9997, 9997));
        }
        if request.case == "search" {
            assert_eq!((rust, torch, upstream), (8_045_661, 8_045_646, 8_045_646));
        }
    }
    eprintln!(
        "REQUEST|rows={}|max_diff={max_diff}|rust_ne_torch_e4={rust_vs_torch}|rust_ne_upstream_e4={rust_vs_upstream}|torch_ne_upstream_e4={torch_vs_upstream}",
        requests.len()
    );
    assert_eq!(
        (
            requests.len(),
            max_diff,
            (rust_vs_torch, rust_vs_upstream, torch_vs_upstream)
        ),
        (REQUEST_ROWS, 15, E4_DIFFERENCES)
    );
}

#[test]
fn pair_rows_are_the_fixture_requests_with_extracted_features() {
    let events = parse_events(EVENTS).expect("fixture parses");
    let inputs: HashMap<String, [i64; 6]> = extract_node_features(&events)
        .iter()
        .map(|f| (f.id.clone(), model_input_q24(f)))
        .collect();
    let mut pairs = 0;
    for case in cases().iter().filter(|c| c.line.starts_with("PAIR|")) {
        let (actor, target) = (
            case.actor.as_deref().expect("actor"),
            case.target.as_deref().expect("target"),
        );
        assert_eq!(
            case.x,
            vec![inputs[actor], inputs[target]],
            "{actor} -> {target}"
        );
        assert_eq!(case.edges, vec![(0, 1)]);
        let score = score_pair(&inputs[actor], &inputs[target]);
        assert_eq!(
            vec![score],
            edge_scores(&case.x, &case.edges).expect("pair")
        );
        pairs += 1;
    }
    let distinct: std::collections::HashSet<(String, String)> = events
        .iter()
        .map(|e| {
            let id = |v: &Option<String>| v.clone().unwrap_or_else(|| "unknown".to_owned());
            (id(&e.actor_id), id(&e.target_id))
        })
        .collect();
    assert_eq!(pairs, distinct.len());
}

#[test]
fn request_pairs_score_as_pinned() {
    let events = parse_events(EVENTS).expect("fixture parses");
    let features = extract_node_features(&events);
    let input =
        |id: &str| model_input_q24(features.iter().find(|f| f.id == id).expect("observed node"));
    let benign = score_pair(&input("agent:benign"), &input("svc:ledger"));
    let rogue = score_pair(&input("agent:rogue"), &input("svc:vault"));
    assert_eq!((benign, q24_to_e4(benign)), (BENIGN_Q24, BENIGN_E4));
    assert_eq!((rogue, q24_to_e4(rogue)), (ROGUE_Q24, ROGUE_E4));
    assert!(features.iter().all(|f| f.id != "agent:ghost"));
}

#[test]
fn server_refusals_are_errors() {
    let node = [0, 0, 0, 0, ONE_Q24, ONE_Q24];
    assert_eq!(edge_scores(&[], &[]), Err(ModelError::EmptyGraph));
    assert_eq!(edge_scores(&[node], &[]), Err(ModelError::EmptyGraph));
    assert_eq!(edge_scores(&[], &[(0, 0)]), Err(ModelError::EmptyGraph));
    assert_eq!(
        edge_scores(&[node, node], &[(0, 1), (1, 1), (1, 2)]),
        Err(ModelError::NodeIndexOutOfRange { edge: 2 })
    );
    assert_eq!(
        edge_scores(&[node, node], &[(2, 0)]),
        Err(ModelError::NodeIndexOutOfRange { edge: 0 })
    );
}

#[test]
fn self_loops_are_replaced_and_duplicate_edges_counted() {
    let x = [
        [
            1_006_633, 3_113_106, 7_456_540, 5_586_813, ONE_Q24, 5_592_405,
        ],
        [
            838_861, 11_930_465, 39_146_837, 3_355_443, ONE_Q24, 16_777_216,
        ],
    ];
    let plain = edge_scores(&x, &[(0, 1)]).expect("graph")[0];
    let with_loops = edge_scores(&x, &[(0, 1), (0, 0), (1, 1), (1, 1)]).expect("graph");
    assert_eq!(with_loops[0], plain);
    let duplicated = edge_scores(&x, &[(0, 1), (0, 1)]).expect("graph");
    assert_eq!(duplicated[0], duplicated[1]);
    assert_ne!(duplicated[0], plain);
}

#[test]
fn any_i64_input_with_max_events_edges_stays_in_range() {
    let extremes = [
        [i64::MAX; 6],
        [i64::MIN; 6],
        [i64::MAX, i64::MIN, i64::MAX, i64::MIN, i64::MAX, i64::MIN],
        [i64::MIN, i64::MAX, i64::MIN, i64::MAX, i64::MIN, i64::MAX],
    ];
    for &source in &extremes {
        for &target in &extremes {
            let p = score_pair(&source, &target);
            assert!((0..=ONE_Q24).contains(&p));
        }
    }
    let mut edges = vec![(0, 1); MAX_EVENTS - 2];
    edges.extend([(1, 0), (2, 2)]);
    let scores = edge_scores(&extremes[..3], &edges).expect("graph");
    assert_eq!(scores.len(), MAX_EVENTS);
    assert!(scores.iter().all(|p| (0..=ONE_Q24).contains(p)));
}

/// Float32 values 2^-15 <= f < 2^top_binade whose `+f.toFixed(4)` differs from `q24_to_e4` of their
/// Q24 rounding, per midpoint (k + 1/2) / 10^4, in units of 2^-16 Q24 units times 20000.
fn float32_values_rounding_differently_below(top_binade: i32) -> u128 {
    let mut count = 0;
    for k in 0..10_000u128 {
        let midpoint = (2 * k + 1) << 40;
        let threshold = ((((2 * k + 1) << 24).div_ceil(20_000) * 2 - 1) * 20_000) << 15;
        let (low, high) = (midpoint.min(threshold), midpoint.max(threshold));
        for binade in -15..top_binade {
            let spacing = 20_000u128 << (binade + 17);
            let (start, end) = (20_000u128 << (binade + 40), 20_000u128 << (binade + 41));
            let (low, high) = (low.max(start), high.min(end));
            if low < high {
                count += high.div_ceil(spacing) - low.div_ceil(spacing);
            }
        }
    }
    count
}

#[test]
fn q24_rounding_changes_the_fourth_decimal_of_float32_values_below_one_half_only() {
    let below_half = float32_values_rounding_differently_below(-1);
    let below_one = float32_values_rounding_differently_below(0);
    eprintln!("FLOAT32_E4|below_one_half={below_half}|below_one={below_one}");
    assert_eq!((below_half, below_one), (20_454, 20_454));
}

#[test]
fn q24_to_e4_is_to_fixed_4_rounding_half_up() {
    assert_eq!(q24_to_e4(0), 0);
    assert_eq!(q24_to_e4(ONE_Q24), 10_000);
    assert_eq!(q24_to_e4(ONE_Q24 / 2), 5_000);
    // 2^19 / 2^24 = 0.03125 exactly: toFixed(4) picks the larger of 0.0312 and 0.0313.
    assert_eq!(q24_to_e4(1 << 19), 313);
    assert_eq!(q24_to_e4((1 << 19) - 1), 312);
    assert_eq!(q24_to_e4(-1), 0);
    assert_eq!(q24_to_e4(ONE_Q24 + 1), 10_000);
    for p in (0..=ONE_Q24).step_by(997) {
        let e4 = i64::from(q24_to_e4(p));
        assert!(2 * (e4 * ONE_Q24 - p * 10_000) <= ONE_Q24, "{p}");
        assert!(2 * (p * 10_000 - e4 * ONE_Q24) < ONE_Q24, "{p}");
    }
}
