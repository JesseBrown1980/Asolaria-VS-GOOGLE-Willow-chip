//! `event` and `features` against tools/features_referee.py, which mirrors gnnDataExport.js:25-64.

use l0_edge::event::{parse_events, Event, EventError, MAX_EVENTS, MAX_RISK_SCORE};
use l0_edge::features::{extract_node_features, model_input_q24, NodeFeatures, FRAC_BITS};

const EVENTS: &str = include_str!("fixtures/events.hbp");
const EXPECTED: &str = include_str!("fixtures/expected-features.hbp");
const ONE_Q24: i64 = 1 << 24;

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c == '%' || c == '|' || c == '=' || c < ' ' || c == '\u{7f}' {
            out.push_str(&format!("%{:02X}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

fn node_row(n: &NodeFeatures) -> String {
    format!(
        "NODE|id={}|type={}|edge_volume={}|avg_risk_centi={}|max_risk={}|failure_rate_milli={}|online={}|trust_tier={}",
        escape(&n.id),
        escape(&n.node_type),
        n.edge_volume,
        n.avg_risk_centi,
        n.max_risk,
        n.failure_rate_milli,
        n.online,
        n.trust_tier
    )
}

fn data_rows(text: &str) -> Vec<&str> {
    text.split('\n')
        .filter(|row| !row.is_empty() && !row.starts_with('#'))
        .collect()
}

fn parsed(text: &str) -> Vec<Event> {
    parse_events(text).expect("valid rows")
}

fn features_of(text: &str) -> Vec<NodeFeatures> {
    extract_node_features(&parsed(text))
}

fn node<'a>(nodes: &'a [NodeFeatures], id: &str) -> &'a NodeFeatures {
    nodes.iter().find(|n| n.id == id).expect("node present")
}

fn ids(nodes: &[NodeFeatures]) -> Vec<&str> {
    nodes.iter().map(|n| n.id.as_str()).collect()
}

fn repeated(row: &str, times: usize) -> String {
    format!("{row}\n").repeat(times)
}

fn exact_round_half_up(num: u64, den: u64) -> u64 {
    (2 * num + den) / (2 * den)
}

#[test]
fn fixture_rows_equal_the_referee_rows() {
    let events = parse_events(EVENTS).expect("fixture parses");
    assert_eq!(events.len(), 336);
    let got: Vec<String> = extract_node_features(&events)
        .iter()
        .map(node_row)
        .collect();
    let want = data_rows(EXPECTED);
    assert_eq!(got.len(), want.len());
    assert_eq!(got.len(), 42);
    for (got, want) in got.iter().zip(&want) {
        assert_eq!(got, want);
    }
}

#[test]
fn fixture_covers_the_request_pairs_and_the_23_of_40_node() {
    let events = parsed(EVENTS);
    let nodes = extract_node_features(&events);

    let benign = node(&nodes, "agent:benign");
    let ledger = node(&nodes, "svc:ledger");
    assert_eq!(
        (benign.node_type.as_str(), benign.trust_tier),
        ("runtime", 3)
    );
    assert_eq!(
        (ledger.node_type.as_str(), ledger.trust_tier),
        ("local_client", 2)
    );
    let rogue = node(&nodes, "agent:rogue");
    let vault = node(&nodes, "svc:vault");
    assert_eq!((rogue.node_type.as_str(), rogue.trust_tier), ("other", 1));
    assert_eq!((vault.node_type.as_str(), vault.trust_tier), ("other", 1));
    assert!(!ids(&nodes).contains(&"agent:ghost"));

    for event in &events {
        let actor = event.actor_id.as_deref();
        let target = event.target_id.as_deref();
        let touches = |id: &str| actor == Some(id) || target == Some(id);
        if touches("agent:benign") || touches("svc:ledger") {
            assert_eq!((actor, target), (Some("agent:benign"), Some("svc:ledger")));
            assert!(event.risk_score <= 1);
            assert_eq!(event.status.as_deref(), Some("ok"));
        }
        if touches("agent:rogue") || touches("svc:vault") {
            assert_eq!((actor, target), (Some("agent:rogue"), Some("svc:vault")));
            assert!(event.risk_score >= 6);
            assert!(matches!(
                event.status.as_deref(),
                Some("failed" | "blocked")
            ));
        }
    }

    let audit = node(&nodes, "svc:audit");
    assert_eq!(audit.edge_volume, 40);
    let audit_total: u64 = events
        .iter()
        .filter(|e| e.target_id.as_deref() == Some("svc:audit"))
        .map(|e| u64::from(e.risk_score))
        .sum();
    assert_eq!(audit_total, 23);
    assert_eq!(audit.avg_risk_centi, 57);
    assert_eq!(exact_round_half_up(2300, 40), 58);
}

#[test]
fn fixture_covers_types_statuses_risks_and_falsy_ids() {
    let events = parsed(EVENTS);
    assert!(events.len() >= 300);
    for wanted in ["runtime", "local_client", "other"] {
        assert!(events
            .iter()
            .any(|e| e.actor_type.as_deref() == Some(wanted)));
        assert!(events
            .iter()
            .any(|e| e.target_type.as_deref() == Some(wanted)));
    }
    assert!(events.iter().any(|e| e.actor_type.is_none()));
    assert!(events.iter().any(|e| e.target_type.is_none()));
    for wanted in ["failed", "error", "blocked", "denied", "ok"] {
        assert!(events.iter().any(|e| e.status.as_deref() == Some(wanted)));
    }
    assert!(events.iter().any(|e| e.status.is_none()));
    for risk in 0..=21 {
        assert!(events.iter().any(|e| e.risk_score == risk), "risk {risk}");
    }
    assert!(events.iter().all(|e| e.risk_score <= 21));
    assert!(events.iter().any(|e| e.actor_id.is_none()));
    assert!(events.iter().any(|e| e.target_id.is_none()));
    assert!(events
        .iter()
        .any(|e| e.actor_id.is_some() && e.actor_id == e.target_id));
    let raw_rows = data_rows(EVENTS);
    assert!(raw_rows.iter().any(|r| r.contains("|actor=|")));
    assert!(raw_rows.iter().any(|r| r.contains("|target=|")));
    assert!(raw_rows.iter().any(|r| !r.contains("|actor=")));
}

#[test]
fn falsy_ids_resolve_to_unknown() {
    let nodes = features_of("EVENT\nEVENT|actor=|target=|risk=4");
    assert_eq!(ids(&nodes), ["unknown"]);
    assert_eq!(nodes[0].edge_volume, 4);
    assert_eq!(nodes[0].avg_risk_centi, 200);
}

#[test]
fn type_comes_from_the_actor_only_when_the_actor_id_equals_the_node_id() {
    let absent = features_of("EVENT|actor_type=runtime|target=t|target_type=other");
    assert_eq!(ids(&absent), ["unknown", "t"]);
    assert_eq!(absent[0].node_type, "other");
    assert_eq!(absent[0].trust_tier, 1);

    let empty = features_of("EVENT|actor=|actor_type=runtime|target_type=local_client");
    assert_eq!(ids(&empty), ["unknown"]);
    assert_eq!(empty[0].node_type, "local_client");

    let literal = features_of("EVENT|actor=unknown|actor_type=runtime|target=t");
    assert_eq!(literal[0].node_type, "runtime");
    assert_eq!(literal[1].node_type, "unknown");

    let both = features_of("EVENT|actor=a|actor_type=runtime|target=b|target_type=local_client");
    assert_eq!(
        (both[0].node_type.as_str(), both[1].node_type.as_str()),
        ("runtime", "local_client")
    );
}

#[test]
fn type_is_fixed_at_first_sight() {
    let nodes = features_of(
        "EVENT|actor=b|target=a|target_type=other\n\
         EVENT|actor=a|actor_type=runtime|target=b|target_type=local_client",
    );
    assert_eq!(node(&nodes, "a").node_type, "other");
    assert_eq!(node(&nodes, "b").node_type, "unknown");
}

#[test]
fn actor_equal_to_target_is_visited_twice() {
    let nodes = features_of(
        "EVENT|actor=x|actor_type=local_client|target=x|target_type=runtime|risk=3|status=failed\n\
         EVENT|actor=x|target=y|risk=1",
    );
    let x = node(&nodes, "x");
    assert_eq!(x.node_type, "local_client");
    assert_eq!(x.edge_volume, 3);
    assert_eq!(x.avg_risk_centi, 233);
    assert_eq!(x.max_risk, 3);
    assert_eq!(x.failure_rate_milli, 667);
}

#[test]
fn failures_count_failed_and_error_only() {
    let statuses = [
        "failed", "error", "blocked", "denied", "ok", "", "FAILED", "failed ",
    ];
    let mut text = String::from("EVENT|actor=a|target=b\n");
    for status in statuses {
        text.push_str("EVENT|actor=a|target=b|status=");
        text.push_str(&escape(status));
        text.push('\n');
    }
    let a = &features_of(&text)[0];
    assert_eq!(a.edge_volume, 9);
    assert_eq!(a.failure_rate_milli, 222);
}

#[test]
fn max_risk_is_the_largest_and_average_is_per_visit() {
    let nodes = features_of(
        "EVENT|actor=a|target=b|risk=0\n\
         EVENT|actor=a|target=c|risk=5\n\
         EVENT|actor=a|risk=3\n\
         EVENT|actor=a|target=b|risk=5",
    );
    let a = node(&nodes, "a");
    assert_eq!((a.max_risk, a.avg_risk_centi), (5, 325));
    let b = node(&nodes, "b");
    assert_eq!((b.max_risk, b.avg_risk_centi), (5, 250));
    assert_eq!(node(&nodes, "unknown").avg_risk_centi, 300);
    let zero = features_of("EVENT|actor=z|risk=0");
    assert_eq!((zero[0].max_risk, zero[0].avg_risk_centi), (0, 0));
}

#[test]
fn rates_round_as_javascript_math_round() {
    let text =
        repeated("EVENT|actor=a|target=b|risk=1", 23) + &repeated("EVENT|actor=a|target=b", 17);
    let a = &features_of(&text)[0];
    assert_eq!((a.edge_volume, a.avg_risk_centi), (40, 57));
    assert_eq!(exact_round_half_up(23 * 100, 40), 58);

    let eighth = repeated("EVENT|actor=a|risk=1", 1) + &repeated("EVENT|actor=a", 7);
    assert_eq!(features_of(&eighth)[0].avg_risk_centi, 13);

    let text = repeated("EVENT|actor=a|target=b|status=error", 201)
        + &repeated("EVENT|actor=a|target=b|status=ok", 199);
    let a = &features_of(&text)[0];
    assert_eq!((a.edge_volume, a.failure_rate_milli), (400, 502));
    assert_eq!(exact_round_half_up(201 * 1000, 400), 503);
}

#[test]
fn trust_tier_follows_the_type_string() {
    let nodes = features_of(
        "EVENT|actor=r|actor_type=runtime|target=l|target_type=local_client\n\
         EVENT|actor=o|actor_type=other|target=c|target_type=Runtime\n\
         EVENT|actor=u",
    );
    let tiers: Vec<(&str, u8)> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.trust_tier))
        .collect();
    assert_eq!(
        tiers,
        [
            ("r", 3),
            ("l", 2),
            ("o", 1),
            ("c", 1),
            ("u", 1),
            ("unknown", 1)
        ]
    );
    assert!(features_of(EVENTS).iter().all(|n| n.online == 1));
}

#[test]
fn nodes_keep_first_sight_order() {
    let nodes =
        features_of("EVENT|actor=c|target=a\nEVENT|actor=b|target=c\nEVENT|actor=a|target=d");
    assert_eq!(ids(&nodes), ["c", "a", "b", "d"]);
    assert!(extract_node_features(&[]).is_empty());
}

#[test]
fn javascript_sums_stay_exact_within_the_parser_limits() {
    let max_visits = 2 * MAX_EVENTS as u64;
    assert!(max_visits * u64::from(MAX_RISK_SCORE) < 1 << 53);
}

#[test]
fn model_input_scales_as_train_baseline_in_q24() {
    assert_eq!(FRAC_BITS, 24);
    let benign = NodeFeatures {
        id: "agent:benign".into(),
        node_type: "runtime".into(),
        edge_volume: 30,
        avg_risk_centi: 50,
        max_risk: 1,
        failure_rate_milli: 0,
        online: 1,
        trust_tier: 3,
    };
    assert_eq!(
        model_input_q24(&benign),
        [5033165, 932068, 1864135, 0, ONE_Q24, ONE_Q24]
    );
    let audit = NodeFeatures {
        edge_volume: 140,
        avg_risk_centi: 57,
        max_risk: 2,
        failure_rate_milli: 400,
        trust_tier: 2,
        ..benign.clone()
    };
    assert_eq!(
        model_input_q24(&audit),
        [ONE_Q24, 1062557, 3728270, 6710886, ONE_Q24, 11184811]
    );
    let low_tier = NodeFeatures {
        trust_tier: 1,
        ..benign.clone()
    };
    assert_eq!(model_input_q24(&low_tier)[5], 5592405);
    let limits = NodeFeatures {
        edge_volume: 2 * MAX_EVENTS as u64,
        avg_risk_centi: 100 * u64::from(MAX_RISK_SCORE),
        max_risk: MAX_RISK_SCORE,
        failure_rate_milli: 1000,
        ..benign
    };
    assert_eq!(
        model_input_q24(&limits),
        [
            ONE_Q24,
            1864135111111,
            1864135111111,
            ONE_Q24,
            ONE_Q24,
            ONE_Q24
        ]
    );
}

#[test]
fn model_input_rounds_half_up_for_every_fixture_node() {
    for n in features_of(EVENTS) {
        let exact = [
            (n.edge_volume.min(100), 100),
            (n.avg_risk_centi, 900),
            (u64::from(n.max_risk), 9),
            (n.failure_rate_milli, 1000),
            (u64::from(n.online), 1),
            (u64::from(n.trust_tier), 3),
        ];
        for (q, (num, den)) in model_input_q24(&n).into_iter().zip(exact) {
            let (q, twice) = (i128::from(q), 2 * i128::from(num) * i128::from(ONE_Q24));
            let den = i128::from(den);
            assert!(
                2 * q * den <= twice + den && twice + den < 2 * (q + 1) * den,
                "{n:?}"
            );
        }
    }
}

#[test]
fn parser_reads_every_key_and_decodes_escapes() {
    let events = parsed(
        "# comment\n\nEVENT|status=ok|risk=21|target_type=local_client|target=svc:a%7Cb\
         |actor_type=runtime|actor=p%25q%3Dr%09s%7F%C5%93\n",
    );
    assert_eq!(
        events,
        [Event {
            actor_id: Some("p%q=r\ts\u{7f}\u{153}".into()),
            actor_type: Some("runtime".into()),
            target_id: Some("svc:a|b".into()),
            target_type: Some("local_client".into()),
            risk_score: 21,
            status: Some("ok".into()),
        }]
    );
    let raw = parsed("EVENT|actor=n\u{153}ud=x|risk=007");
    assert_eq!(raw[0].actor_id.as_deref(), Some("n\u{153}ud=x"));
    assert_eq!(raw[0].risk_score, 7);
}

#[test]
fn parser_maps_absent_and_empty_values_to_falsy() {
    assert_eq!(parsed("EVENT"), [Event::default()]);
    assert_eq!(
        parsed("EVENT|actor=|actor_type=|target=|target_type=|status="),
        [Event::default()]
    );
    assert_eq!(
        parsed(&format!("EVENT|risk={MAX_RISK_SCORE}"))[0].risk_score,
        MAX_RISK_SCORE
    );
    assert!(parsed("").is_empty());
    assert!(parsed("# only a comment\n\n").is_empty());
}

fn refusal(text: &str) -> EventError {
    parse_events(text).expect_err("refused")
}

#[test]
fn parser_refuses_rows_without_the_event_prefix() {
    for text in [
        "EVENTS|actor=a",
        "event|actor=a",
        " EVENT",
        "actor=a",
        "   ",
        "EVENT\r",
        "\tEVENT",
    ] {
        assert_eq!(
            refusal(text),
            EventError::MissingPrefix { line: 1 },
            "{text:?}"
        );
    }
}

#[test]
fn parser_refuses_fields_without_equals_unknown_and_duplicate_keys() {
    for text in ["EVENT|actor", "EVENT|", "EVENT||actor=a"] {
        assert_eq!(
            refusal(text),
            EventError::MissingEquals { line: 1 },
            "{text:?}"
        );
    }
    for key in ["actr", "Actor", "at", "id", ""] {
        let text = format!("EVENT|{key}=x");
        assert_eq!(
            refusal(&text),
            EventError::UnknownKey {
                line: 1,
                key: key.into()
            }
        );
    }
    for key in [
        "actor",
        "actor_type",
        "target",
        "target_type",
        "risk",
        "status",
    ] {
        let text = format!("EVENT|{key}=1|{key}=1");
        assert_eq!(
            refusal(&text),
            EventError::DuplicateKey {
                line: 1,
                key: key.into()
            }
        );
    }
}

#[test]
fn parser_refuses_bad_escapes_control_bytes_and_invalid_utf8() {
    for value in ["%7c", "%7", "%", "a%", "%G0", "%%41", "%7%41"] {
        let text = format!("EVENT|actor={value}");
        assert_eq!(
            refusal(&text),
            EventError::BadEscape { line: 1 },
            "{value:?}"
        );
    }
    for value in ["a\tb", "\u{7f}", "a\r", "\u{1b}"] {
        let text = format!("EVENT|status={value}");
        assert_eq!(
            refusal(&text),
            EventError::UnescapedControlByte { line: 1 },
            "{value:?}"
        );
    }
    assert_eq!(
        refusal("EVENT|actor=a\r\nEVENT"),
        EventError::UnescapedControlByte { line: 1 }
    );
    for value in ["%FF", "%C5", "%C0%80", "%ED%A0%80"] {
        let text = format!("EVENT|target={value}");
        assert_eq!(
            refusal(&text),
            EventError::InvalidUtf8 { line: 1 },
            "{value:?}"
        );
    }
}

#[test]
fn parser_refuses_risk_that_is_not_a_bounded_decimal() {
    for value in ["", "+1", "-1", "1.0", " 1", "1 ", "0x1", "1e3", "\u{661}"] {
        let text = format!("EVENT|risk={}", escape(value));
        assert_eq!(
            refusal(&text),
            EventError::RiskNotDecimal { line: 1 },
            "{value:?}"
        );
    }
    let above = MAX_RISK_SCORE + 1;
    for value in [
        format!("{above}"),
        "4294967296".into(),
        "99999999999999999999".into(),
    ] {
        let text = format!("EVENT|risk={value}");
        assert_eq!(
            refusal(&text),
            EventError::RiskAboveLimit { line: 1 },
            "{value:?}"
        );
    }
}

#[test]
fn parser_counts_lines_from_one_including_comments_and_blanks() {
    let text = "# header\nEVENT|actor=a\n\n# note\nEVENT|risk=x";
    assert_eq!(refusal(text), EventError::RiskNotDecimal { line: 5 });
    assert_eq!(
        refusal(text).to_string(),
        "line 5: risk must be decimal digits"
    );
    assert_eq!(
        EventError::UnknownKey {
            line: 2,
            key: "a|b".into()
        }
        .to_string(),
        "line 2: unknown key \"a|b\""
    );
}

#[test]
fn parser_refuses_more_than_max_events() {
    let text = format!("# header\n{}", repeated("EVENT", MAX_EVENTS + 1));
    assert_eq!(
        refusal(&text),
        EventError::TooManyEvents {
            line: MAX_EVENTS + 2
        }
    );
    assert_eq!(
        refusal(&text).to_string(),
        format!("line {}: more than 1000000 events", MAX_EVENTS + 2)
    );
}
