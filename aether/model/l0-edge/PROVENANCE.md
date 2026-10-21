# Provenance: asolaria-l0-edge

This crate ports the trained L0 EdgeLevelGNN that the Asolaria sidecar serves on port 4792, and the
graph-event feature extractor whose output it was trained on, into integer-only Rust. This file
records where every upstream byte came from, what was ported and what was only vendored, and the
measured agreement with the upstream code.

## Lanes

| Lane | What runs there | Here |
|---|---|---|
| Hot (per request, in process) | Rust only: event parsing, features, the L0 forward, the score, the gates, the CLI. HBP text, SHA-256 and integers; no Python, torch, Node, JSON or HTTP. | `src/` of this crate, `gate/observed/src/`, `cli/` |
| Cold (verification, CI only) | Stdlib Python referees, and torch + torch_geometric for the model parity harness. The tools of this crate compute in integers only; float runs only inside the owner's code that `tools/model_parity.py` runs unchanged (torch with the trained float32 checkpoint, numpy in `build_node_features`). | `tools/`, `gate/observed/tools/` |

The vendored `.js` and `.mjs` files are kept only as pinned bytes. Nothing executes them; their
numbers are reproduced by Python referees (see [Referees](#referees-in-python-instead-of-node)).
The vendored Python files are imported only by the cold-lane torch parity harness.

## Vendored upstream files

Every file under `vendor/` is byte-identical to its upstream. `git hash-object <file>` gives the
blob id below, and CI runs `sha256sum -c UPSTREAM.sha256`.

| Vendored path | Upstream repository @ commit | Upstream path | Git blob | sha256 |
|---|---|---|---|---|
| `vendor/legacy/gnnDataExport.js` | `JesseBrown1980/asolaria-behcs-256` @ `818cc235a6db815e3b2591339cf9b42aad3e85a0` | `packages-legacy-import/src/gnnDataExport.js` | `f422efea02af6eb9dea23a85c257eb90ab1532e8` | `cf45e1c91e987dabb777523b817fc7644531b00212873190b10fc6a273562bf3` |
| `vendor/gnn-sidecar/models/gnn_baseline.py` | `JesseBrown1980/asolaria-behcs-256` @ `818cc235a6db815e3b2591339cf9b42aad3e85a0` | `services/gnn-sidecar/models/gnn_baseline.py` | `510f78890ec94b113f0610afbade8bafe6ca20e0` | `1dfa3343b1b1bac6e78bb8a698e360009f7966176aa68e6c8f4c110fcfb33b34` |
| `vendor/gnn-sidecar/train_baseline.py` | `JesseBrown1980/asolaria-behcs-256` @ `818cc235a6db815e3b2591339cf9b42aad3e85a0` | `services/gnn-sidecar/train_baseline.py` | `4fefc7abd1081fcae7135d3d04d63d43786525e8` | `af23d41adb0d7041322525f9c9195a2ee0f38ff05eedbb3f4832ccd320fa297a` |
| `vendor/gnn-sidecar/inference_server.py` | `JesseBrown1980/asolaria-behcs-256` @ `818cc235a6db815e3b2591339cf9b42aad3e85a0` | `services/gnn-sidecar/inference_server.py` | `3089da1d249f0eb76bb79e03a446714636474980` | `93a9a8432073c90b12c57c375d51c73e816e5a450696f649771a5c10c0246f14` |
| `vendor/gnn-sidecar/baseline_model.pt` | `JesseBrown1980/Asolaria-fnns-trained-and-reverse-gnns-many` @ `c4b2458c88358c46155d3b308c594f9991720847` | `models/baseline_model.pt` | `26caf5216a4df3a43250e40e1dd421180ca3c901` | `a09993369d31ca73345914812e5226b513f5fb9092f43e487a2eafc8c3b621f5` |
| `vendor/pipeline/asolaria-score.mjs` | `JesseBrown1980/Shannon-and-the-gnns-stage` @ `32f9670a648dbbd02d64524e91150efeec8e4d2c` | `pipeline/asolaria-score.mjs` | `b101c0989de3af0ca41db0eac1293c758d7e5296` | `aee9c38328e75bc71f6e17a288637ddd1dbc4633576006dce9bc50c898093530` |

The same bytes are also at these upstream paths (same git blob id):

- `baseline_model.pt`: `asolaria-behcs-256` @ `818cc235` `services/gnn-sidecar/baseline_model.pt`.
- `asolaria-score.mjs`: `JesseBrown1980/bigpickle-rebuild` @ `c592599c8f3d000b07cd5f4218b7d6b24d49d7d5` `src/asolaria-score.mjs`.

## What is ported and what is only vendored

| Upstream | Ported to | Notes |
|---|---|---|
| `extractNodeFeatures` (gnnDataExport.js:25-64) | `src/features.rs` `extract_node_features` | Map insertion order; ids `event.actor?.id \|\| "unknown"`; type at first sight; `Math.round` on the binary64 quotient via `src/jsnum.rs`. |
| `build_node_features` scaling (train_baseline.py:26-39) | `src/features.rs` `model_input_q24` | `[min(vol/100,1), avgRisk/9, maxRisk/9, failureRate, online, trustTier/3]` in Q24, round half up. `build_node_features` rounds the same ratios to float32 (`np.array(..., dtype=np.float32)`, line 39); the Q24 inputs are rounded from the exact ratio instead. For example trustTier 1/3 is 11,184,811/2 Q24 units in float32 and 5,592,405 in Q24, and Q24 values of 1 or more are rounded again to float32 in the server's tensor (inference_server.py:24). `tools/model_parity.py` runs torch on both inputs; see [Measured agreement](#measured-agreement). |
| `baseline_model.pt` (float32 state dict) | `src/weights.rs` | `floor(w * 2^24 + 1/2)` per weight, written by `tools/quantize.py` without torch; `conv*.lin.bias` loads as `conv*.bias` (inference_server.py:11). |
| `EdgeLevelGNN.forward` (gnn_baseline.py:43-67) as inference_server.py:9-13 builds it and line 24 calls it | `src/model.rs` `edge_scores`, `score_pair` | See [Which forward is ported](#which-forward-is-ported-and-why). |
| JavaScript `Math.round`, `toFixed`, `+ * /` on numbers | `src/jsnum.rs` `Dbl` | Exact binary64 values and roundings, integers only. |
| The L0 request of asolaria-score.mjs:121 (`nodes` of two, `edges` `[[0, 1]]`, `scores[0]`) | `src/lib.rs` `observe_pair` | Same request shape; node 0 is the request's actor and node 1 its target, with features from the extractor (the owner's specification for feeding L0 graph observations; gnnDataExport.js:107-110 uses the same actor -> target direction). |
| `shannonSignal`, `shaBaseline`, `reverseGain`, the composite (asolaria-score.mjs:86-108, 151-163) | `gate/observed/src/score.rs` | e4 units; the composite through `Dbl` in WEIGHTS order; Shannon in binary64 with V8's `Math.log2` (ieee754.cc:1951-2050). |

Vendored only, not ported:

- The HTTP handler of `inference_server.py` (lines 16-32): the port runs the model in process. The
  handler checks `edge_features` and passes only `nodes` and `edges` to `MODEL` (lines 22-24), so the
  port takes no edge features.
- The `ImportError` fallback `GCNConv` of `gnn_baseline.py` (lines 6-16), a plain `Linear`; the
  server runs torch_geometric's `GCNConv`.
- The training loop of `train_baseline.py` and `extractEdgeFeatures` of `gnnDataExport.js`
  (lines 67-95, the edge labels used for training).
- The L4, G1, G2, G3 and G4 queries of `asolaria-score.mjs` (lines 39-70, 126-138): this stack has
  no such services, so those signals are absent, as when the services do not answer.

## Which forward is ported, and why

asolaria-score reads its L0 signal from `POST :4792/infer` (asolaria-score.mjs:73-83, 121). That
port is served by `inference_server.py`, which builds `MODEL = EdgeLevelGNN(6, 64)`, renames
`conv1.lin.bias` and `conv2.lin.bias` to `conv1.bias` and `conv2.bias`, loads the checkpoint and
calls `eval()` (lines 9-13). With torch_geometric installed, `conv1` and `conv2` are
torch_geometric 2.8.1 `GCNConv`: self loops added with fill value 1 where missing
(`add_remaining_self_loops`), duplicate edges counted, degree taken at the target index,
`norm = deg^-1/2[source] * deg^-1/2[target]`, flow source to target, bias added after
propagation. Then ReLU (dropout is off in eval), the second `GCNConv`, and the edge head
`Linear(128, 64)`, ReLU, `Linear(64, 1)`, `Sigmoid` over `cat(h[source], h[target])`.

That server forward is the one ported, because it is the one whose output reaches the score.
`src/model.rs` runs it in Q24 integers: i128 accumulation, normalisation coefficients
`round(2^32 / sqrt(d_s * d_t))`, one round-half-up per layer output, and a fixed-point sigmoid that
returns `floor(2^24 sigmoid(t) + 1/2)` exactly for every logit (see [The sigmoid](#the-sigmoid-exact-rounding-copied-from-modell1-prototype)).

## The sigmoid: exact rounding, copied from model/l1-prototype

`sigmoid_q24` first runs a range-reduced integer series: `exp_neg` gives `e^-|t|` in units of
2^-62, and the quotient `(numerator 2^25 + denominator) / (2 denominator)` rounds `2^24 sigmoid(t)`
half up. `exp_neg` is within 12 units of 2^-62 of `e^-|t|`: that is the bound `model/l1-prototype`
states (at most 18 series terms, each rounded to half a unit, and `LN2_Q62` within 53/1000 of a
unit per octave), and it reports every error it measured, over 252,154 inputs that include each
octave boundary, below 4 units. Checked here in integers: `2^62 ln 2 - LN2_Q62` lies between
-14,184,754 / 2^28 and -14,184,753 / 2^28. So the quotient is within 2^-34 of `2^24 sigmoid(t)`.
When the quotient lies within 2^-32 of a half, `round_near_half` decides the side exactly:
`exp_exceeds` compares `b e^|t|` with `a` (`a = 2 upper - 1`, `b = 2^25 - a`) from lower and upper
sums of the series of `e^|t|` (`exp_series_bounds`, every term rounded down and up, the tail
bounded by the last upper term), at 128 bits and doubling until the two sums fall on the same
side. For `t != 0`, `e^|t|` is irrational, so they always do; `t = 0` never reaches this path (its
quotient is `2^23 + 1/2` exactly, `2^23` after flooring).

Before this change the crate had the series step alone. Measured on that code (HEAD f67b300, in
a scratch copy, against the exact values of `tools/sigmoid_referee.py`):

| Logits | Series step alone: one Q24 unit from the exact rounding | With the exact decision |
|---|---|---|
| the 1,511 SIGMOID rows over [-20, 20] | 0 | 0 |
| the 736 boundary rows (logits within 2^-32 of a half, below) | 20 (for example logit_q48 `-2903085619798773` gives 556 where the exact rounding is 557) | 0 |
| a scratch hunt (not committed; rebuilt only from the seed and construction recorded here, not from a committed fixture): 60,000 Q48 logit rows on the step-1 lattice next to `ln((k + 1/2) / (2^24 - k - 1/2)) 2^48` (`floor(c / step) step` and the next lattice point, both signs) for 15,000 `k` (5,000 below 16,000, 5,000 within 16,000 of 2^24 - 1, 5,000 below 2^24; the generator of `tools/jsnum_referee.py` seeded `0x48554E54`). They hold 49,576 distinct logits: 12,394 distinct `k` once `k` and `2^24 - 1 - k` are counted as one, since both give the same four logits | 1,582 rows (1,186 distinct logits) | 0 |
| the same hunt on the step-10 lattice (60,000 rows, 49,576 distinct logits; 5,062 of them also on the step-1 list) | 248 rows (188 distinct logits) | 0 |

Every one of those 1,850 rows (1,246 distinct logits) is 1 unit off (925 rows one unit above the
exact rounding, 925 one unit below), and in every one the exact `2^24 sigmoid(t)` lies within
2^-39 of a half (the closest within 2^-50). `src/model.rs` keeps the series-only rounding as a test helper and pins the 20
(`series_rounding_alone_is_one_off_on_20_of_the_736_boundary_rows`). The L1 review measured the
same series code in `model/l1-prototype`: 3,590 of 59,992 near-tie logits (step 1) and 624 of
59,992 (step 10) one unit off, every one within 2^-39 of a tie. Those two counts are quoted from
that review; its 59,992-logit set is not in this repository or on this container, so they are not
re-measured here.

No model score in the fixtures changes: the Rust score of every PAIR, GRAPH and REQUEST row of
`expected-model.hbp` (3,273 + 3,338 = 6,611 scores) is the same before and after.

**Duplicated code.** `TIE_WINDOW_BITS` and its doc comment (lines 29-31), and `sigmoid_q24`,
`round_near_half`, `exp_exceeds`, `exp_series_bounds`, `big_scale`, `big_mul_small`, `big_add` and
`big_cmp` (lines 191-309, identical code; `sigmoid_q24`'s doc comment is this crate's), are copied
from `model/l1-prototype/src/model.rs` @ HEAD `f67b300` (git blob
`3af5631475877b168629fe8585419309db99e354`); the test
`exp_exceeds_doubles_the_precision_until_the_sums_decide` is copied from lines 364-374 of the
same file. Two doc numbers are written as integer ratios here (`53/1000` for `0.053`, and
`0 < q e - p < 17 / 10^21`, checked in integers, for `1.6e-20`). This crate takes no dependency
on `model/l1-prototype`, so the two copies are maintained separately: a change to one must be made
to the other. The boundary set in `tools/sigmoid_referee.py` is the one of
`model/l1-prototype/tools/int_referee.py:193-212` (git blob
`e6439eead7f6b2d41c8382dd8df72c007c611d93`, same seed, same `k`, same four extra logits),
computed with integer series brackets instead of Python `decimal`; its 736 rows are byte-identical
to the last 736 SIGMOID rows of `model/l1-prototype/tests/fixtures/expected-int.hbp` at HEAD.

## Measured agreement

| Check | Cases | Result |
|---|---|---|
| Rust `edge_scores` vs the vendored server `MODEL` (torch 2.14.1+cpu, torch_geometric 2.8.1, Python 3.13.16), `tests/model.rs` over `tests/fixtures/expected-model.hbp` | 3,273 scores: 174 fixture request pairs and 240 fixed-seed random graphs of 2 to 12 nodes with self loops and duplicate edges, risk up to 21, 100 and 1,000,000 | max \|rust - torch\| = 6 Q24 units (6 / 2^24); 2,331 of 3,273 equal; the test allows 32 (2^-19) |
| Rust `score_pair` vs the server `MODEL`, torch fed the Q24 inputs and the float32 inputs of `train_baseline.build_node_features` (`tests/model.rs`, REQUEST rows) | 3,338 two-node requests: 2 fixed pairs (`e4_midpoint`, and `search`, found by a local search over two-node requests) and the 3,336 of 100,000 fixed-seed request pairs whose torch score is within 32 Q24 units of a `toFixed(4)` rounding midpoint | max \|rust - torch\| = 15 Q24 units (case `search`: Rust 8,045,661, torch 8,045,646) |
| `+score.toFixed(4)` of Rust vs of the torch float32 output (gate/observed `tests/e2e.rs`) | 174 fixture request pairs, torch fed either input | 174 equal for both |
| `+score.toFixed(4)` of Rust vs of the torch float32 output (`tests/model.rs`) | 100,000 fixed-seed request pairs (edge volume 1 to 150, max risk 0 to 21, total risk and failures events can give, tier 1 to 3) | differ by one e4 unit in 34 (torch fed the Q24 inputs) and in 38 (torch fed the `build_node_features` inputs); torch's two inputs differ in 10 |
| `+f.toFixed(4)` vs `q24_to_e4` of the Q24 rounding of `f`, every float32 `f` in [2^-15, 1) (`tests/model.rs`, in integers) | all float32 values near the 10,000 midpoints | 20,454 differ, all below 1/2 (the float32 grid is finer than Q24 there); below 2^-15 both give 0 (`2 10^4 < 2^15`, and the Q24 rounding is at most 2^9, whose e4 value is 0) |
| `sigmoid_q24` vs the exact `floor(2^24 sigmoid(t) + 1/2)` of `tools/sigmoid_referee.py` (integer series brackets), `src/model.rs` tests | 2,247 logits: 1,511 over [-20, 20] (equal to the former 60-digit `decimal` rows, byte for byte) and 736 within 2^-32 of a half | all equal; the series step alone is one unit off on 20 of the 736 |
| `LN2_Q62` vs `floor(2^62 ln 2 + 1/2)` of `tools/sigmoid_referee.py` | 1 | equal |
| `extract_node_features` vs `tools/features_referee.py` | 336 events, 42 nodes | all fields equal |
| `src/jsnum.rs` vs `tools/jsnum_referee.py` vectors | 24,576 rows (ratio, `*`, `+`, `/`, `Math.round`, `toFixed(4)`) | all equal |
| `Math.round` over the extractor's full grids (`jsnum_referee.py --grid \| jsnum_grid`) | 1,765,200 | all equal |
| `weights.rs` vs `tools/quantize.py --check` | 12,929 weights | equal |

`tools/model_parity.py --check` compares the float32 bit patterns of every torch output with the
committed file, byte for byte. torch's float32 results depend on the CPU kernels it selects. The
tool therefore sets `ATEN_CPU_CAPABILITY=default`, `MKL_CBWR=COMPATIBLE`, `OMP_NUM_THREADS=1` and
`MKL_NUM_THREADS=1` before torch loads: ATen runs its scalar kernels and oneMKL a single SSE2 code
path (oneMKL Conditional Numerical Reproducibility, `COMPATIBLE`), with one thread. On the x86-64
AVX-512 host that wrote the file, `--check` passes with the caller's environment set to
`ATEN_CPU_CAPABILITY=avx2`, `avx512`, `MKL_ENABLE_INSTRUCTIONS=AVX2` or `SSE4_2`, `MKL_CBWR=AVX2`
and 2 or 3 threads. torch's default selection on that host gives different float32 outputs in 203
of the 3,273 scores of the fixture pairs and random graphs (at most 27 ulp, at most 2 Q24 units).
It has not been run on an AMD CPU here. The REQUEST rows are screened with batched graphs and then
scored one request at a time, as the server scores them; the tool stops if a batched score is
more than 24 Q24 units from a single-request score (measured: at most 7).

## JavaScript rounding, reproduced

`extractNodeFeatures` computes `Math.round(totalRisk / edgeCount * 100)` and
`Math.round(failures / edgeCount * 1000)` on binary64 values (gnnDataExport.js:58, 60). For
example `23 / 40 * 100` is the binary64 value 8,092,405,580,431,359 / 2^47 = 115/2 - 2^-47
(JavaScript prints it as `57.49999999999999`), so the JavaScript result is 57, where exact rational
rounding of 115/2 gives 58. Over `totalRisk` 0..=21·c and `edgeCount` c in 1..=400
(1,684,600 cases), the JavaScript result and exact rational rounding differ in 574 cases; over
`failures` 0..=c (80,600 cases) they differ in 2 (201/400 and 203/400). The port reproduces the
JavaScript result in every case, through `src/jsnum.rs`. The fixture's `svc:audit` node (40
events, total risk 23) carries this case: `avg_risk_centi=57`.

## The SYNTHETIC fixture

`tests/fixtures/events.hbp` is SYNTHETIC: `tools/make_fixture.py` writes it from a fixed seed, and
its header says so. It is not observed traffic. It holds 336 events over 42 nodes and covers the
runtime, local_client, other, missing and empty types; the failed, error, blocked, denied, ok,
missing and empty statuses; risk 0 to 21; a missing actor id, an empty target id, actor equal to
target, escaped values and the 23/40 case. Its header lists the request pairs the tests use:
`agent:benign -> svc:ledger` (runtime to local_client, risk 0-1, status ok),
`agent:rogue -> svc:vault` (other to other, risk >= 6, failed or blocked) and
`agent:ghost -> svc:ledger` (`agent:ghost` never observed). The numbers computed from it show
what the code computes on these events; they are not a measurement of the model on observed
traffic.

## Referees in Python instead of Node

A JavaScript number is an IEEE-754 binary64 value. The referees of this crate hold it as its 64-bit
pattern and compute in integer arithmetic only, as JavaScript does: each `+ * /` is computed
exactly on the rationals the two bit patterns stand for and rounded once to the nearest binary64,
ties to even (`tools/jsnum_referee.py` `to_binary64`, `decode`, `js_add`, `js_mul`, `js_div`). With
`num / den` the exact value of a bit pattern (`den` a power of two), the mirrors are:

| JavaScript | Python mirror (integers) |
|---|---|
| `a + b`, `a * b`, `a / b` | the exact sum, product or quotient of the two rationals, rounded to the nearest binary64, ties to even |
| `Math.round(x)` | `(2 num + den) // (2 den)` |
| `x.toFixed(d)`, 0 <= x < 1e21 | `(2 num 10^d + den) // (2 den)` units of 10^-d |
| `Math.round(t / c * s)` (gnnDataExport.js:58, :60) | `js_round_ratio_scaled(t, c, s)`: binary64 `/`, then `*`, then `Math.round`, on bit patterns |
| `Number(event.risk?.score \|\| 0)` on the HBP integer field | `int` |

Rewritten in integers on 2026-10-18 (they used Python floats before, which hold the same binary64
values); every committed file they write is reproduced byte for byte, and the `--grid` stream is
byte-identical (1,765,200 lines). `tools/make_fixture.py` had no float operation to rewrite.

`gate/observed/tools/score_referee.py` mirrors further upstream JavaScript for that crate (unary
`+s` of a `toFixed` string, `Math.log2` as V8's `log2` statement by statement: v8/v8 tag
12.4.254.21, the V8 of Node v22.22.0, `src/base/ieee754.cc:1951-2050`, git blob
`e71b63fd7c17711e5dc04d9acc040be0aa0b7c40`, without FMA contraction; `Buffer.from(String(s),
'utf8')` and `createHash('sha256')` as `s.encode('utf-8')` and `hashlib.sha256`); its arithmetic is
recorded in `gate/observed`.

| Referee (stdlib, `python3 -I`) | Mirrors | Writes; `--check` diffs |
|---|---|---|
| `tools/make_fixture.py` | (fixed-seed generator) | `tests/fixtures/events.hbp` |
| `tools/features_referee.py` | gnnDataExport.js:25-64 | `tests/fixtures/expected-features.hbp` |
| `tools/jsnum_referee.py` | `Math.round`, `toFixed`, binary64 arithmetic; `--grid` streams gnnDataExport.js:58 and :60 over the full grids | `tests/fixtures/jsnum-vectors.hbp` |
| `tools/quantize.py` | `torch.load` of the checkpoint (zip + a restricted unpickler that resolves only `collections.OrderedDict`, `torch._utils._rebuild_tensor_v2` and `torch.FloatStorage`) and inference_server.py:11; each float32 weight read as its bit pattern and rounded to Q24 in integers | `src/weights.rs` |
| `tools/sigmoid_referee.py` | the exact `floor(2^24 sigmoid(t) + 1/2)` and `floor(2^62 ln 2 + 1/2)` from integer series brackets (no `decimal`); the boundary set of `model/l1-prototype/tools/int_referee.py:193-212` | the CONST and SIGMOID rows of `tests/fixtures/expected-model.hbp` (`--check` only; `tools/model_parity.py` writes them with these functions) |
| `gate/observed/tools/score_referee.py` | asolaria-score.mjs:25, 86-108, 145, 151-163; V8 ieee754.cc:1951-2050 (`Math.log2`) | `gate/observed/tests/fixtures/expected-score.hbp` |

The one referee that is not stdlib is `tools/model_parity.py`: it imports the vendored
`inference_server.py` with torch and torch_geometric, as the deployed server does, and never binds
a port. Float runs there only inside the owner's code, unchanged: the `MODEL` forward on the
trained float32 checkpoint, and `train_baseline.build_node_features` (numpy). The tool's own
arithmetic is integer: it builds each input tensor from float32 bit patterns (`q / 2^24` rounded
to nearest, ties to even, the value `torch.tensor(..., dtype=float32)` stores for the double
`q / 2^24` the server receives), hands `build_node_features` the binary64 `avgRisk` and
`failureRate` made from bit patterns computed by `tools/jsnum_referee.py` and reads its float32
rows back by bit pattern, and reads every output by its float32 bit pattern; Q24 roundings,
midpoint distances and the batch screen are exact integer or integer-pair arithmetic on those bits.
Rewritten this way on 2026-10-18, it reproduces every PAIR, GRAPH and REQUEST row of the committed
file byte for byte. Every data file these tools write is HBP text.
