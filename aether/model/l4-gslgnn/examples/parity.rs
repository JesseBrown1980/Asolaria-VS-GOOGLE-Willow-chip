//! Compares this crate with a parity file written by `tools/model_parity.py --seed N --out FILE`.
//! Usage: cargo run --example parity -- FILE

use l4_gslgnn::{edge_scores, request_nodes, score_request, NODE_FEATURES};
use std::collections::HashMap;
use std::process::ExitCode;

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split('|')
        .skip(1)
        .filter_map(|field| field.split_once('='))
        .collect()
}

fn rows(x: &str) -> Vec<[i64; NODE_FEATURES]> {
    x.split(';')
        .map(|row| {
            let row: Vec<i64> = row.split(',').map(|q| q.parse().expect("q24")).collect();
            row.try_into().expect("six features")
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

#[derive(Default)]
struct Stats {
    values: usize,
    equal: usize,
    max_diff: i64,
    worst: String,
}

impl Stats {
    fn add(&mut self, case: &str, got: i64, want: i64) {
        let diff = (got - want).abs();
        self.values += 1;
        self.equal += usize::from(diff == 0);
        if diff > self.max_diff || self.worst.is_empty() {
            self.max_diff = self.max_diff.max(diff);
            self.worst = case.to_string();
        }
    }
}

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: parity FILE");
        return ExitCode::from(2);
    };
    let text = std::fs::read_to_string(&path).expect("readable parity file");
    let mut stats: [(&str, Stats); 3] = [
        ("GRAPH", Stats::default()),
        ("REQUEST", Stats::default()),
        ("BOUNDARY", Stats::default()),
    ];
    let mut e4_differ = 0;
    for line in text.lines() {
        let label = line.split('|').next().unwrap_or("");
        let Some(index) = stats.iter().position(|(name, _)| *name == label) else {
            continue;
        };
        let f = fields(line);
        let want: Vec<i64> = f["p_q24"]
            .split(',')
            .map(|q| q.parse().expect("q24"))
            .collect();
        let got = if label == "REQUEST" {
            let (pid, content) = (hex_text(f["pid_hex"]), hex_text(f["content_hex"]));
            assert_eq!(
                request_nodes(&pid, &content).to_vec(),
                rows(f["x"]),
                "{line}"
            );
            let reading = score_request(&pid, &content);
            e4_differ += usize::from(reading.score_e4.to_string() != f["e4"]);
            vec![reading.score_q24]
        } else {
            let edges: Vec<(usize, usize)> = f["edges"]
                .split(',')
                .map(|edge| {
                    let (s, t) = edge.split_once('>').expect("s>t");
                    (s.parse().expect("row"), t.parse().expect("col"))
                })
                .collect();
            edge_scores(&rows(f["x"]), &edges).expect("valid graph")
        };
        for (got, want) in got.into_iter().zip(want) {
            stats[index].1.add(f["case"], got, want);
        }
    }
    for (label, s) in &stats {
        println!(
            "PARITY|{label}|values={}|equal={}|max_abs_diff_q24={}|worst_case={}",
            s.values, s.equal, s.max_diff, s.worst
        );
    }
    println!("PARITY|REQUEST|e4_differ={e4_differ}");
    ExitCode::SUCCESS
}
