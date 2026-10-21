//! `jsnum` against the vectors tools/jsnum_referee.py writes from Python's binary64 arithmetic.

use l0_edge::jsnum::Dbl;

const VECTORS: &str = include_str!("fixtures/jsnum-vectors.hbp");

fn ratio(field: &str) -> Dbl {
    let (num, den) = field.split_once('/').expect("num/den");
    Dbl::from_ratio(num.parse().expect("num"), den.parse().expect("den"))
}

#[test]
fn matches_the_referee_vectors() {
    let mut cases = 0;
    let mut extractor_cases = 0;
    for line in VECTORS.lines().filter(|l| !l.starts_with('#')) {
        let mut op = "";
        let (mut a, mut b, mut want) = ("", "", 0u64);
        for field in line.split('|').skip(1) {
            let (key, value) = field.split_once('=').expect("key=value");
            match key {
                "op" => op = value,
                "a" => a = value,
                "b" => b = value,
                "want" => want = value.parse().expect("want"),
                _ => panic!("unknown key {key}"),
            }
        }
        let (a, b_dbl) = (ratio(a), ratio(b));
        let got = match op {
            "ratio" => a.to_bits(),
            "mul" => (a * b_dbl).to_bits(),
            "add" => (a + b_dbl).to_bits(),
            "div" => (a / b_dbl).to_bits(),
            "round" => (a * b_dbl).js_round(),
            "fixed4" => (a * b_dbl).to_fixed_units(4),
            _ => panic!("unknown op {op}"),
        };
        assert_eq!(got, want, "{line}");
        cases += 1;
        if op == "round" && (b == "100/1" || b == "1000/1") {
            extractor_cases += 1;
        }
    }
    assert!(cases > 20_000, "{cases} cases");
    // 574 risk averages and 2 failure rates in gnnDataExport.js's grids round as JavaScript does.
    assert_eq!(extractor_cases, 576);
}
