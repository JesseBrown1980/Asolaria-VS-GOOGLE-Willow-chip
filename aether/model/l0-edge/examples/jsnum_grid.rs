//! Reads `t c scale want` rows from `tools/jsnum_referee.py --grid` and checks `Math.round(t / c * scale)`.

use l0_edge::jsnum::Dbl;
use std::io::BufRead;

fn main() {
    let (mut cases, mut mismatches) = (0u64, 0u64);
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let field: Vec<u64> = line
            .split(' ')
            .map(|f| f.parse().expect("integer"))
            .collect();
        let got = (Dbl::from_ratio(field[0], field[1]) * Dbl::from_u64(field[2])).js_round();
        cases += 1;
        if got != field[3] {
            mismatches += 1;
            eprintln!("MISMATCH|row={line}|got={got}|json=0");
        }
    }
    println!("JSNUM_GRID|cases={cases}|mismatches={mismatches}|json=0");
    if mismatches > 0 || cases == 0 {
        std::process::exit(1);
    }
}
