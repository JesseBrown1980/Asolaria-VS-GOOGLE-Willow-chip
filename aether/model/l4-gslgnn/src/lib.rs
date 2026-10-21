//! Integer port of the Asolaria L4 GSLGNN as served on port 4793, with the weights of its served
//! checkpoint `gslgnn_w9_3_seq47_v2.pt`, and the request asolaria-score sends it.

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod model;
pub mod weights;

pub use model::{
    edge_scores, q24_to_e4, ModelError, FRAC_BITS, HIDDEN, MAX_EDGES, MAX_INPUT_Q24, MAX_NODES,
    NODE_FEATURES,
};

use sha2::{Digest, Sha256};

/// The L4 signal of one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L4Reading {
    /// `scores[0]` of the :4793 answer (asolaria-score.mjs:78), Q24 in 0..=2^24.
    pub score_q24: i64,
    /// `signals.l4 = +l4.toFixed(4)` (asolaria-score.mjs:128) in e4 units.
    pub score_e4: u32,
}

/// The nodes asolaria-score.mjs:112-113 builds and line 127 sends: `[node(sha16(pid)), node(sha16(content))]`,
/// each feature the float32 (gslgnn_inference_server.py:51) of `byte / 255`, in Q24.
pub fn request_nodes(pid: &str, content: &str) -> [[i64; NODE_FEATURES]; 2] {
    [node(pid), node(content)]
}

/// The L4 reading of `score(pid, content)`: nodes from [`request_nodes`], edges `[[0, 1]]`
/// (asolaria-score.mjs:127), the forward, then `+scores[0].toFixed(4)` (lines 78, 128).
pub fn score_request(pid: &str, content: &str) -> L4Reading {
    let score_q24 = match edge_scores(&request_nodes(pid, content), &[(0, 1)]).as_deref() {
        Ok([score]) => *score,
        _ => unreachable!("two in-range nodes and one edge always score"),
    };
    L4Reading {
        score_q24,
        score_e4: q24_to_e4(score_q24),
    }
}

/// `node(sha16(text))`: the first six bytes of SHA-256 of the UTF-8 text (the first twelve hex digits of
/// `sha16`, asolaria-score.mjs:25, 113).
fn node(text: &str) -> [i64; NODE_FEATURES] {
    let digest = Sha256::digest(text.as_bytes());
    core::array::from_fn(|i| byte_input_q24(digest[i]))
}

/// floor(f * 2^24 + 1/2) for f = float32(binary64(byte / 255)): asolaria-score.mjs:113 divides in
/// binary64 and gslgnn_inference_server.py:51 converts to float32, each rounding to nearest, ties to even.
pub fn byte_input_q24(byte: u8) -> i64 {
    const DOUBLE_BITS: u32 = 53;
    const FLOAT_BITS: u32 = 24;
    if byte == 0 {
        return 0;
    }
    let numerator = u128::from(byte);
    // binary64: byte / 255 = significand * 2^-shift with 2^52 <= significand < 2^53.
    let mut shift = DOUBLE_BITS - 1;
    while (numerator << shift) / 255 < 1 << (DOUBLE_BITS - 1) {
        shift += 1;
    }
    let (quotient, remainder) = ((numerator << shift) / 255, (numerator << shift) % 255);
    let double = quotient + u128::from(2 * remainder > 255);
    // float32: drop 29 bits, ties to even; a carry to 2^24 keeps the same value scale.
    let dropped = DOUBLE_BITS - FLOAT_BITS;
    let (high, low, half) = (
        double >> dropped,
        double & ((1 << dropped) - 1),
        1u128 << (dropped - 1),
    );
    let float = high + u128::from(low > half || (low == half && high % 2 == 1));
    // value * 2^24 = float * 2^(24 + dropped - shift); 24 + dropped - shift is 1 for 255 / 255 and <= 0 else.
    let q24 = match (FLOAT_BITS + dropped).checked_sub(shift) {
        Some(up) => float << up,
        None => {
            let down = shift - FLOAT_BITS - dropped;
            (float + (1 << (down - 1))) >> down
        }
    };
    q24 as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATH: &str = include_str!("../tests/fixtures/expected-math.hbp");

    #[test]
    fn byte_inputs_match_the_float32_referee() {
        let mut rows = 0;
        for line in MATH.lines().filter(|l| l.starts_with("BYTE|")) {
            let mut fields = line
                .split('|')
                .skip(1)
                .map(|f| f.split_once('=').expect("k=v"));
            let (_, byte) = fields.next().expect("byte");
            let (_, q24) = fields.next().expect("q24");
            let byte: u8 = byte.parse().expect("u8");
            assert_eq!(
                byte_input_q24(byte),
                q24.parse::<i64>().expect("q24"),
                "{line}"
            );
            rows += 1;
        }
        assert_eq!(rows, 256);
    }
}
