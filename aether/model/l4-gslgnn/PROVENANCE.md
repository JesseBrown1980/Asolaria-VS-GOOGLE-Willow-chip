# Provenance: asolaria-l4-gslgnn

This crate ports the trained L4 GSLGNN that the Asolaria sidecar serves on port 4793, with its served
checkpoint `gslgnn_w9_3_seq47_v2.pt`, and the request asolaria-score sends it, into integer-only Rust.
This file records where every upstream byte came from, which forward is ported and why, how the
parity harness runs the upstream server code, and the measured agreement and outputs.

## Lanes

| Lane | What runs there | Here |
|---|---|---|
| Hot (per request, in process) | Rust only, integers only: SHA-256 of the request strings, the input conversion, the forward, `toFixed(4)`. No Python, torch, Node, JSON or HTTP. | `src/` |
| Cold (verification, CI only) | Stdlib Python referees (`tools/quantize.py`, `tools/math_referee.py`), and torch + torch_geometric for the parity harness (`tools/model_parity.py`). | `tools/` |

The vendored Python files are imported only by the cold-lane parity harness. Nothing in the hot lane
reads the vendored checkpoint; `src/weights.rs` is generated from it without torch.

The three tools compute in integers only (Python integers and exact integer ratios; no float, no
decimal). Float runs only inside torch: on the owner's code and trained float32 checkpoint, run
unchanged (the staged server's `MODEL`; its own `gsl` module, called with no edges, for the attention
mask, gsl_gnn.py:63-77; `key_transform` and `query_transform` for the probe's attention scores;
`torch.sigmoid(threshold)` for the `LAYOUT` row, gsl_gnn.py:77), and in `torch.rand` for the probe's
uniform inputs. Every float32 the harness sends torch is built from a bit pattern rounded in integers
(to binary64, then float32, nearest with ties to even: what `torch.tensor(..., dtype=torch.float32)`
makes of a Python float, including the doubles `byte / 255` of asolaria-score.mjs:113), and every torch
output is read back by its float32 bit pattern.

## Vendored upstream files

Every file under `vendor/` is byte-identical to its upstream. `git hash-object <file>` gives the
blob id below, and CI runs `sha256sum -c UPSTREAM.sha256`.

| Vendored path | Upstream repository @ commit | Upstream path | Git blob | sha256 |
|---|---|---|---|---|
| `vendor/fnns/src/gslgnn_inference_server.py` | `JesseBrown1980/Asolaria-fnns-trained-and-reverse-gnns-many` @ `c4b2458c88358c46155d3b308c594f9991720847` | `src/gslgnn_inference_server.py` | `b31236d28cb0a5145e718c54334bb577b5f5c23f` | `8ac31eff0320a3bc0eb9a3c95ed8c271d93c5fbce907230ed2e76a390a01fc88` |
| `vendor/fnns/models/gslgnn_w9_3_seq47_v2.pt` | `JesseBrown1980/Asolaria-fnns-trained-and-reverse-gnns-many` @ `c4b2458c88358c46155d3b308c594f9991720847` | `models/gslgnn_w9_3_seq47_v2.pt` | `e3a9712b50f8e7b31590b0aa944e6a9ba6ea565b` | `9e50723b10c9de5c27baa5bc9386340bf72f76bcae2877064a47e4627a86602c` |
| `vendor/gnn-sidecar/models/gsl_gnn.py` | `JesseBrown1980/asolaria-behcs-256` @ `818cc235a6db815e3b2591339cf9b42aad3e85a0` | `services/gnn-sidecar/models/gsl_gnn.py` | `886b3b0c0cdbddba983fa8c3ae083c4520d38f0e` | `783b9b50d92ed8756270b87f5922d66a19972bfd51c1bc0d5ca0d5a483d22475` |
| `vendor/gnn-sidecar/models/__init__.py` | `JesseBrown1980/asolaria-behcs-256` @ `818cc235a6db815e3b2591339cf9b42aad3e85a0` | `services/gnn-sidecar/models/__init__.py` | `0ef9b432c6bbda74623a3bdfcdb1e22b75501c05` | `51b739fbc5851e391b90a917ee92062a97003f9428ec2167c49c7a76ff86bbc5` |

The same `gsl_gnn.py` bytes (blob `886b3b0c`) are also at Asolaria-fnns @ `c4b2458c` `src/gsl_gnn.py`.

All four files were read in full before copying. The server holds code, comments naming the fabric
ruling and the `acer-micro` voter, and its response fields; `gsl_gnn.py` holds code and docstrings;
`__init__.py` is one comment line; the checkpoint's `data.pkl` holds only the 19 state-dict keys, the
23 module paths, `cpu` and the storage records, and its other entries are float32 tensor bytes, format
markers and a numeric serialization id. None holds personal data.

### Pinned, not vendored

| Upstream | Size | Git blob | sha256 |
|---|---|---|---|
| Asolaria-fnns @ `c4b2458c` `models/gslgnn_w9_3_seq47.pt` (v1, not served) | 120,356 | `6cddadc91ff4e6fa2bbfc315d90fa1bab0117c6b` | `6f82384ff2e313b1f34b48cca5c1c2bb3a8f9391bba54db7afbb36c5fda91ad5` |
| Asolaria-fnns @ `c4b2458c` `manifests/gslgnn_w9_3_seq47_v2_manifest.json` | 1,520 | `f1eb3675ab5e8822d1511ce4b3062d51ee50b2dd` | `65324379834efad6f13780bf9f3f45b37f17fd4e61117367b13931116da81019` |
| Asolaria-fnns @ `c4b2458c` `manifests/gslgnn_w9_3_seq47_manifest.json` | 1,479 | `34251205696e0fb71097e21b0d7925881e975b95` | `cff92f439044267987a8addd1c94f3d8b0fc96e6380769f20f015e197477074b` |
| Asolaria-fnns @ `c4b2458c` `src/train_gslgnn_w9_3.py` (the trainer) | 358 lines | `c964b9e25b09d757a9cd3876e50d59ea55bb5a0a` | `af694c1f6d3ea13b0b64f8763d2605f679b2d542ee9892135d5dbb41ce4fcef9` |
| `JesseBrown1980/Shannon-and-the-gnns-stage` @ `32f9670a648dbbd02d64524e91150efeec8e4d2c` `pipeline/asolaria-score.mjs` (180 lines) | | `b101c0989de3af0ca41db0eac1293c758d7e5296` | `aee9c38328e75bc71f6e17a288637ddd1dbc4633576006dce9bc50c898093530` |

`asolaria-score.mjs` is vendored byte-identical in `model/l0-edge/vendor/pipeline/`; the line numbers
cited below are of that blob.

## What is ported and what is only vendored

| Upstream | Ported to | Notes |
|---|---|---|
| `gslgnn_w9_3_seq47_v2.pt` (float32 state dict, 19 tensors, 28,259 values) | `src/weights.rs` | `floor(w * 2^24 + 1/2)` per weight, written by `tools/quantize.py` without torch; the tool refuses any key list or `_metadata` module-path list other than the torch_geometric class's. |
| `GSLGNN.forward` (gsl_gnn.py:171-220) in eval mode, `num_classes=2`, as gslgnn_inference_server.py:21-36 builds it and line 54 calls it | `src/model.rs` `edge_scores` | See [Which forward is ported](#which-forward-is-ported-and-why). |
| `GraphStructureLearner.forward` (gsl_gnn.py:50-94), `encode_with_learned_graph`, `encode_with_original_graph`, `get_edge_embeddings` (lines 137-169) | `src/model.rs` | Called by the forward. |
| The L4 request of asolaria-score.mjs:112-113, 127 (`nodes` `[node(sha16(pid)), node(sha16(content))]`, `edges` `[[0, 1]]`) and lines 78, 128 (`scores[0]`, `+l4.toFixed(4)`) | `src/lib.rs` `request_nodes`, `score_request`, `byte_input_q24`; `src/model.rs` `q24_to_e4` | The input path of the :4793 call, below. |

Vendored only, not ported:

- The HTTP handler of the server (lines 39-73): the port runs the model in process.
- The fallback `GCNConv`, `to_dense_adj` and `dense_to_sparse` of gsl_gnn.py:20-32 (taken only when
  `torch_geometric` does not import); the checkpoint is saved in the torch_geometric layout.
- `return_weights=True`, `get_learned_graph` and `get_edge_importance` (gsl_gnn.py:218-240): the
  server never calls them.

## Which forward is ported, and why

asolaria-score reads its L4 signal from `POST :4793/infer` (asolaria-score.mjs:125-129, through the
`l0Infer` function of lines 73-83 with `port: 4793`). That port is served by
`gslgnn_inference_server.py`: it builds `MODEL = GSLGNN(node_input_dim=6, hidden_dim=64)`, loads the
v2 checkpoint, remaps keys between `.lin.bias` and `.bias` where the model lacks one, loads the
result (strict) and calls `eval()` (lines 21-36). The ported forward is that object's.

- **torch_geometric branch.** gsl_gnn.py:17-19 import `GCNConv`, `to_dense_adj` and
  `dense_to_sparse` from torch_geometric; only on ImportError (lines 20-32) does it define a
  `GCNConv` that is one `nn.Linear` ignoring `edge_index` and a `to_dense_adj` returning zeros. The
  checkpoint's keys (`conv*.bias`, `conv*.lin.weight`, no `conv*.lin.bias`) and module paths (an
  `aggr_module` per conv) are the torch_geometric layout, so the served forward is the
  torch_geometric branch under torch_geometric 2.8.1. The parity harness asserts that all four
  convs of the served `MODEL` are `torch_geometric.nn.conv.gcn_conv.GCNConv`, so it never compares
  against the fallback.
- **Key remap.** For this checkpoint every key is already in the model, so lines 26-34 rename
  nothing; the harness asserts this (`renamed_keys=0` in the `LAYOUT` row).
- **Request.** The server reads only `nodes` and `edges` (line 46); asolaria-score.mjs:75 also sends
  `edge_features` (`edgeFeat`, line 114), which the L4 forward never sees. `nodes` arrive as JavaScript
  doubles `byte / 255` (line 113) and become float32 at server line 51.

The forward, per request graph of `n` nodes:

1. **Learned adjacency** (gsl_gnn.py:63-91). `keys = key_transform(x)`, `queries = query_transform(x)`
   (`Linear(6, 32)` each); `attention = sigmoid(queries @ keys.T / sqrt(32))`, where torch divides by
   the float32 of the Python scalar `32 ** 0.5`, 11863283 * 2^-21 (measured: 200,000 of 200,000
   quotients equal division by that value); `mask = attention > sigmoid(threshold)`;
   `combined = 0.5 * to_dense_adj(edge_index, max_num_nodes=n)[0] + 0.5 * attention * mask`.
   torch_geometric's `to_dense_adj` (utils/_to_dense_adj.py:89-97) sums duplicate edges and keeps
   self-loop edges.
2. **Self loops and row normalisation** (lines 190-195): `+ I`, then each row divided by its sum
   (`clamp(min=1)` never applies: every row sum is at least 1).
3. **Learned branch** (lines 137-154): `adj @ x`, `conv1.lin`, ReLU, `adj @ h`, `conv2.lin`. In
   torch_geometric's `GCNConv`, `lin` is `Linear(bias=False)` and the bias is the conv's own
   parameter, so this branch applies no bias; `conv1.bias` and `conv2.bias` (all zero in the
   checkpoint) are not read.
4. **Original branch** (lines 156-163): torch_geometric 2.8.1 `GCNConv` twice with ReLU between
   (gcn_conv.py:99-113 `gcn_norm`: existing self loops dropped and one added per node with weight 1,
   duplicates kept, degree at the target; lines 227-268: `lin`, propagation source to target, bias).
5. **Edge head** (lines 165-169, 204-214): per edge `(row, col)`,
   `edge_classifier(cat(h_learned[row], h_learned[col], h_original[row], h_original[col]))` =
   `Linear(256, 64)`, ReLU, `Dropout` (off in eval), `Linear(64, 32)`, ReLU, `Linear(32, 2)`; then
   `softmax(dim=-1)[:, 1]`.

Integer arithmetic: Q24 values with i128 accumulation and one round-half-up per layer output.
Adjacency entries are held in Q25, so `0.5 * count` and `0.5 * attention` are exact;
the two row-normalised products are computed as exact weighted means, each rounded once. The
attention score `q . k` stays in exact Q48 and is divided by 11863283 * 2^-21 with one rounding; the
sigmoid is the fixed-point sigmoid equal to an exact reference decided from integer bounds (1,811
rows). The mask compares the attention and `sigmoid(threshold)` each rounded to Q24; in [0.5, 1), where
`sigmoid(threshold)` = 10443135/2^24 lies, float32 values are exactly the Q24 grid, and
`sigmoid_q24(0.5)` = 10443135 is torch's float32 result 0x3f1f597f. For two classes
`softmax(logits)[1]` is the sigmoid of the exact Q48 difference `logits[1] - logits[0]`. The original
branch uses the normalisation coefficients `round(2^32 / sqrt(d_s * d_t))` of `model/l0-edge`.

Domain: `edge_scores` returns an error for no nodes or no edges (the server answers 400 for no edges,
lines 49-50), an edge index `>= n` (torch raises), a feature beyond `MAX_INPUT_Q24` = 2^40 (65536.0),
or more than 2^20 nodes or edges; within these bounds a unit test shows no intermediate can overflow
i128. The request path is always inside them.

## The input path of the :4793 call

`model/l0-edge` builds its L0 inputs from observed graph events (`observe_pair`: `extract_node_features`
then the train_baseline.py scaling `[min(vol/100, 1), avgRisk/9, maxRisk/9, failureRate, online,
trustTier/3]`). That is not the input upstream sends to :4793. asolaria-score.mjs sends, for both :4792
(line 121) and :4793 (line 127), `nodes = [node(ph), node(ch)]` with `ph = sha16(pid)`,
`ch = sha16(content)` (line 112), `sha16(s)` the first 16 hex digits of SHA-256 of `String(s)` (line 25),
and `node(h)[i] = parseInt(h.slice(2i, 2i + 2), 16) / 255` for `i` in 0..6 (line 113). This crate ports
that path: `request_nodes(pid, content)` takes the first six bytes of SHA-256 of each UTF-8 string and
converts each byte to the Q24 value of `float32(binary64(byte / 255))` (`byte_input_q24`, equal to the
stdlib referee on all 256 bytes). `score_request` runs the forward on edge `(0, 1)` and returns
`scores[0]` in Q24 and `+scores[0].toFixed(4)` in e4 units, rounding half up on the exact Q24 value.
So aether's L0 lane (event features) and this L4 lane (request hashes) use different input
derivations; upstream used the hash derivation for both calls.

## Staging the server for the parity harness

At `c4b2458c`, `src/gslgnn_inference_server.py` cannot import where it sits: line 15 puts its own
directory on `sys.path` and line 16 runs `from models.gsl_gnn import GSLGNN`, which resolves to
`src/models/gsl_gnn.py`; that file is not in the repository (`gsl_gnn.py` is at `src/gsl_gnn.py`).
Line 18 loads `Path(__file__).with_name("gslgnn_w9_3_seq47_v2.pt")`, i.e. `src/gslgnn_w9_3_seq47_v2.pt`;
the checkpoint is at `models/`. Executed on a throwaway copy of that layout (`python -I -B`): the import
stops with `ModuleNotFoundError: No module named 'models'`, and with `src/models/` added, with
`FileNotFoundError` for `src/gslgnn_w9_3_seq47_v2.pt`. The v2 manifest records where it ran:
`"checkpoint_path": "C:\\Users\\acer\\Asolaria\\services\\gnn-sidecar\\gslgnn_w9_3_seq47_v2.pt"` and
`"model_class": "services/gnn-sidecar/models/gsl_gnn.py::GSLGNN"`, the layout of behcs-256
`services/gnn-sidecar/` (which holds `models/__init__.py` and `models/gsl_gnn.py` but no
`gslgnn_inference_server.py`).

The private mirror `asolaria-mirror.git` (read only, nothing copied from it) holds that layout at
commit `7991d355666e6e4cd2c7c51143cedfccabe95083` (2026-04-30) `services/gnn-sidecar/`: a
`gslgnn_inference_server.py` (blob `e5052afa179ec69535519e433a4de7bb954c7f4e`, 62 lines) beside
`models/__init__.py` and `models/gsl_gnn.py` (the vendored blobs `0ef9b432`, `886b3b0c`), the
checkpoint as `gslgnn-trained-2026-04-29.pt` (blob `e3a9712b`, the vendored v2 bytes) and the v2
manifest as `gslgnn-v2-manifest.json` (blob `f1eb3675`). That server version imports
`models.gsl_gnn` from its own directory, builds `GSLGNN(node_input_dim=6, hidden_dim=64)`, loads the
checkpoint without a key remap (strict), calls `eval()`, and requires `edges[*]` of length 2; its
response fields differ. Staged the same way and compared in the cold lane (`python -I -B`, the
pinned kernels): its `MODEL` has the same state dict as the vendored server's, both use
torch_geometric's `GCNConv`, and probs[:, 1] is bit-identical on all 205 `GRAPH` and `BOUNDARY`
rows of `tests/fixtures/expected-model.hbp`. The port therefore matches both server versions.

The ACER cube `9859b576` copy in this repository (`acer/Asolaria/services/gnn-sidecar/`, commit
`46cc8c5`, pinned in `acer/UPSTREAM.sha256`) holds `gslgnn_inference_server.py`, `models/__init__.py`,
`models/gsl_gnn.py` and `gslgnn_w9_3_seq47_v2.pt` beside the server, byte-identical to the four vendored
files (the same sha256 values), in exactly the layout `tools/model_parity.py` stages. It also holds the
v1 checkpoint `gslgnn_w9_3_seq47.pt` (sha256 `6f82384f…`) that `tools/model_parity.py --probe` reads.

`tools/model_parity.py` copies the four vendored files, after checking their sha256, into
one temporary directory in that layout (server, `models/__init__.py`, `models/gsl_gnn.py`, and the
checkpoint beside the server) and imports the server module there, so its lines 13-36 run unchanged
and its `MODEL` is the object compared. This staging is a cold-lane arrangement; no upstream file is
changed. The harness sets `sys.dont_write_bytecode` before any import, so no `__pycache__` is written
beside staged or vendored sources. It does not run the HTTP handler: per graph it mirrors lines 51-54
(the float32 `x` that `torch.tensor(nodes, dtype=torch.float32)` makes, built from integer-rounded bits;
`torch.tensor(edges, dtype=torch.long).t().contiguous()`; `MODEL(x, edge_index)` under `no_grad`), and
per request asolaria-score.mjs:112-113, 127-128.

The harness pins `ATEN_CPU_CAPABILITY=avx2` and `MKL_ENABLE_INSTRUCTIONS=AVX2` before importing torch
and runs one thread. Measured on an AVX-512 host: without the pin, the `BOUNDARY` rows move with the
MKL instruction set (the float32 crossing of the mask shifts), and under `ATEN_CPU_CAPABILITY=default`
16 values change by 1 Q24 unit (7 `GRAPH`, 1 `REQUEST`, 8 `BOUNDARY`); with the pin, `--check` passes under any of
`ATEN_CPU_CAPABILITY=avx512|default`, `MKL_ENABLE_INSTRUCTIONS=AVX512` and `OMP_NUM_THREADS=4`, and in
both torch environments on this host (`scratchpad/torchvenv`, `scratchpad/ci-torch-venv`; torch
2.14.1+cpu, torch_geometric 2.8.1, Python 3.13.16).

## Measured agreement

| Check | Cases | Result |
|---|---|---|
| Rust `edge_scores` vs the served `MODEL` (`tests/model.rs`) | 393 edges of 125 graphs (5 fixed, 120 seeded) | max \|rust - torch\| = 1 Q24 unit; 306 of 393 equal; the test allows 16 |
| Rust attention mask vs torch's `attention > sigmoid(threshold)` (`src/model.rs` unit test) | 3,616 entries of the 125 graphs (598 set) | all equal |
| Rust `score_request` vs the mirrored request (`tests/model.rs`) | 64 pid/content pairs (incl. empty strings, non-ASCII UTF-8) | `request_nodes` equal on all; max \|rust - torch\| = 1 Q24 unit; `toFixed(4)` equal on all 64 (all `5292`); no mask entry set, in torch or here |
| `BOUNDARY` rows: 16 inputs around each of 5 float32 crossings of the mask entry (0, 1) | 80 inputs | the mask here differs from torch's at 12 (from 1 step before to 4 steps after torch's crossing); every score within 9 Q24 units |
| Fixed-point sigmoid vs `tools/math_referee.py` (integer bounds on `e^-t`, each row decided) | 1,811 logits | all equal |
| `byte_input_q24` vs the referee's float32 of the binary64 `byte / 255` (both roundings on integers) | 256 bytes | all equal |
| `weights.rs` vs `tools/quantize.py --check` | 28,259 weights, 23 module paths | equal |

The `GRAPH` inputs are Q24 integers that the harness sends torch as the float32 of `q / 2^24`. For 78 of
the 3,696 `GRAPH` inputs of the committed file (27 graphs; the second and third features, `avgRisk/9`
and `maxRisk/9`, of the train-layout nodes, at values above 2^24 Q24 units) `q` needs more than 24
significant bits, so torch receives its float32 rounding, 1 Q24 unit away for 70 of them and 2 for 8,
while the port receives `q` as written in `x`; the 393-edge comparison above (max 1 Q24 unit) includes
them. The other 3,618 `GRAPH` inputs, all 960 `BOUNDARY` inputs and all 768 `REQUEST` inputs are float32
values exactly. The fixture's header line 2 says "GRAPH/BOUNDARY send them as float32 q / 2^24
(exact)"; the fixture is kept byte-identical here, so that line is unchanged.

Fresh-seed re-runs (`tools/model_parity.py --seed N --out FILE`, then
`cargo run --example parity -- FILE`), not committed:

| Seed | GRAPH edges, max diff | REQUEST, max diff, toFixed(4) | BOUNDARY rows, max diff |
|---|---|---|---|
| `0x1234567` | 372, 1 | 64, 1, all equal | 176, 6 |
| `0xC0FFEE` | 377, 1 | 64, 1, all equal | 192, 10 |
| `0x4793` | 353, 1 | 64, 1, all equal | 176, 5 |

Search for inputs where the port and torch disagree: 600 seeded searches with the harness's
`boundary_rows` (one feature of node 1 swept over float32 values in [0.5, 8] with the other features
in [-2, 2]) found 235 crossings; at the 12 float32 inputs around each (steps -6 to +5 from torch's
crossing; 2,820 inputs) the mask here differs from torch's at 512, at steps -6 to +5, both ends of that
sampled window. The score differs by at most 1 Q24 unit where the masks agree and by at most 13 Q24
units (13/2^24) where they differ. That search's seed is not recorded; its 2,820 rows were re-run: torch
reproduces every row byte for byte and the port gives the same counts. A second search with a wider
window (seed `0x51F15E`, 600 searches, the 64 float32 inputs around each crossing, steps -32 to +31)
found 256 crossings (16,384 inputs); the mask here differs from torch's at 514, at steps -6 to +12; the
score differs by at most 1 Q24 unit where the masks agree and by at most 10 where they differ. Both are
cold-lane scratch measurements, not committed. The
width of the disagreement window follows from the Q24 rounding of the `key_transform` and `query_transform`
weights and from torch's float32 rounding of the scores. On request inputs (`byte / 255`) the
attention score `q . k / sqrt(32)` stays far below the threshold 0.5 (8,388,608 Q24 units): over the
500 probe requests of seed 4793 (`tools/model_parity.py --probe`, below), the 2,000 entries are 158,635
to 2,029,627 Q24 units (Q24 of the exact `q_i . k_j / (11863283 * 2^-21)` of the served `MODEL`'s
float32 keys and queries), and no mask entry is set; the 64 fixture requests assert no mask entry, in
torch and here. This replaces an earlier scratch range over 2,000 requests whose seed was not recorded.

## Measured outputs of the served checkpoint

Scores below are Q24 integers, `floor(f * 2^24 + 1/2)` of the float32 `f` computed in integers from its bit
pattern (scale 2^24; the measurement). A parenthesised decimal is `n/2^24` of the Q24 integer `n` before
it, rounded half up to 7 decimals, computed exactly from the integer. `toFixed(4)` values (`0.5292`,
`0.5291`, `0.0000`) are the e4 outputs. A float32 written `m * 2^e` is its exact value, with its bits in hex.

- Every request scored: over the 64 fixture requests, `scores[0]` is 8,877,767 to 8,877,785 Q24 units
  in torch (0.5291561 to 0.5291572) and 8,877,767 to 8,877,786 in Rust. The smallest, 8,877,767, is 103
  Q24 units above 8,877,664, the first Q24 integer above the `toFixed(4)` boundary 0.52915 (exact Q24
  value 8,877,663 + 529/625), below which `toFixed(4)` would give `0.5291`.
  `signals.l4` is `0.5292` on every request measured.
- The fixture graphs (inputs up to 2^29 Q24 units, i.e. 32, masks set and unset): probs[:, 1] from
  8,877,586 to 8,878,792 Q24 units (0.5291454 to 0.5292172) in torch.
- The wiring map's probe inputs, re-run in integers by
  `tools/model_parity.py --probe ../../acer/Asolaria/services/gnn-sidecar/gslgnn_w9_3_seq47.pt`. The
  probe and its map are `universe/tools/l4_probe.py` (sha256 `4885ff78…`) and
  `universe/WIRING-MAP.md:338-349` on this container's scratchpad, not in this repository; their recipe:
  `random.Random(4793)` draws 500 pid/content pairs (pid `%016x` or `BH.HOOKWALL.%016X` of 64 random
  bits, content 1 to 199 printable ASCII characters), sent as the request of asolaria-score.mjs:112-113,
  127; then per checkpoint, v2 before v1, the same generator seeds 500 `torch.rand` inputs of 2 to 11
  nodes with edge `[[0, 1]]`. v2 is the staged server's `MODEL`. v1 is not served and not vendored
  here; its bytes (sha256 `6f82384f…`) are staged under the file name server line 18 reads, so the
  server's own lines 21-36 build, remap, load and `eval()` it. On all 1,000 inputs of each checkpoint
  the scores are float32 bit-identical to those of the probe's own copy of lines 21-36 (`l4_probe.py`
  `build`), in both torch environments. `scores[0]`, with the attention mask (no entry set in any set)
  and the attention score of each entry (Q24 of the exact `q_i . k_j / (11863283 * 2^-21)` of torch's
  float32 keys and queries; the threshold 0.5 is 8,388,608):
  - v2, 500 score requests: 8,877,762 to 8,877,790 Q24 units (0.5291559 to 0.5291575), 28 distinct
    values (28 distinct float32), each `toFixed(4)` = `0.5292`; 0 of 2,000 mask entries set; attention
    scores 158,635 to 2,029,627.
  - v2, 500 uniform inputs: 8,877,763 to 8,877,788 Q24 units (0.5291559 to 0.5291574), 26 distinct
    values (26 distinct float32), each `0.5292`; 0 of 26,131 mask entries set; attention scores -85,615
    to 2,256,108.
  - v1, 500 score requests: 0 Q24 units on every one (1 distinct value), each `toFixed(4)` = `0.0000`;
    the 500 float32 scores are all distinct, from 11443580 * 2^-119 (`0fae9d7c`) to 13713152 * 2^-53
    (`30d13f00`); 0 of 2,000 mask entries set; attention scores -1,628,452 to 828,784.
  - v1, 500 uniform inputs: 0 Q24 units on every one, each `0.0000`; the 500 float32 scores are all
    distinct, from 15038789 * 2^-113 (`12e57945`) to 10143413 * 2^-53 (`309ac6b5`); 0 of 24,639 mask
    entries set; attention scores -1,645,223 to 1,029,709.

  Every v1 score is positive and below 2^-29, which is why its Q24 value and `toFixed(4)` are 0 on every
  input while its float32 values differ on every input. All of these figures are identical in both
  torch environments (`scratchpad/torchvenv`, `scratchpad/ci-torch-venv`) and with or without the
  `ATEN_CPU_CAPABILITY=avx2` / `MKL_ENABLE_INSTRUCTIONS=AVX2` pin, except the float32 bits of v1's score
  requests: without the pin they run from 11443668 * 2^-119 (`0fae9dd4`) to 13713178 * 2^-53
  (`30d13f1a`), still 500 distinct, with the same Q24 values, `toFixed(4)` values and mask.
- Upstream's notes on this output: asolaria-score.mjs:28 "may still return degenerate 0.5292 score
  until corpus rebalancing — honest provenance tracked", and line 31 (`L4_STATUS.note`) "process alive,
  now queried alongside L0. May show dead-constant 0.5292 until corpus rebalance.", with
  `rebalance_when: 'retrain spread std > 0.01 on balanced corpus'` (line 32). The server's response
  carries `"disclosure": "label-rule-learnability not generalization; fit-for-body-tally-scoring"`
  (line 61).
- The v2 manifest (`manifests/gslgnn_w9_3_seq47_v2_manifest.json`, sha256 `65324379…`), quoted as
  upstream wrote it: `corpus_stats` `"totalNodes": 109`, `"totalEdges": 315249`, `"benign": 40`,
  `"suspicious": 315209`, `"labelRatio": 0.9998731161716611`; `metrics` `"test_accuracy":
  0.9991540908813477`, `"test_precision": 0.9991541194383353`, `"test_recall": 1.0`, `"test_f1":
  0.9995768807649996`, `"epochs": 30`. The v1 manifest (`manifests/gslgnn_w9_3_seq47_manifest.json`,
  sha256 `cff92f43…`): `"totalNodes": 97`, `"totalEdges": 13783`, `"benign": 13743`, `"suspicious": 40`,
  `"labelRatio": 0.0029021258071537403`; `"test_accuracy": 0.9806576371192932`, `"test_precision": 0`,
  `"test_recall": 0.0`, `"test_f1": 0`, `"epochs": 30`. Each `labelRatio` is the printed binary64 of
  suspicious / totalEdges, 315,209/315,249 and 40/13,783 (checked in integers: each printed decimal is
  within half a binary64 unit in the last place of that ratio's binary64 rounding). The corpus file
  `gslgnn_w9_3_corpus.json`: the gnn-sidecar cube `9859b576` located a build of it on ACER (sha256
  `7e7d0406…`, 68,695,303 B), transferred on 2026-10-18 to `acer/Asolaria/services/gnn-sidecar/`. That build
  differs from the v2 manifest's pinned `0f123581…` (`build_corpus()` joins time-varying orchestrator
  ticks), so the exact v2 corpus is not the cube's copy, and these metrics are not re-measured here.
- `gsl.threshold` is 0.5 exactly (float32 0x3f000000), the value gsl_gnn.py:48 initialises; it
  reaches the forward only through the mask comparison.
- gsl_gnn.py's docstring states "Accuracy: 96.66% (+4.79% over baseline) - BEST PERFORMING MODEL",
  "ROC-AUC: 99.70%", "False Positive Rate: 1.5%" (lines 7-10); Asolaria-fnns README.md:44-45 calls these
  "repository-reported training results". An evaluation run or log that produced them is not in
  Asolaria-fnns @ `c4b2458c`, asolaria-behcs-256 @ `818cc235` or the ACER cube `9859b576` copy in
  `acer/` (each searched on this container for `96.66` and `99.70`: the hits are this docstring, that
  README passage and Asolaria-fnns `docs/PRE-ASOLARIA-HEALTHCARE-GNN-PROVENANCE-AND-PATH2-2026-07-11.md:27`,
  and three behcs-256 `data/agent-index/` notes that restate the figures).

## Composite weight

Upstream (blob `b101c098`), asolaria-score.mjs:152, 156 gave L4 the weight 0.15 among `l0 0.30, l4
0.15, g1 0.10, g2 0.10, g3 0.10, shannon 0.15, baseline 0.10`, normalised over the signals present
(lines 157-162). The served scorer no longer does: see the owner decision below, applied the same day.
This crate supplies the reported value, `score_request(pid, content).score_e4`, for `signals.l4`.

Owner decision 2026-10-18 (OWNER-RULES 7.a amendment): L4 is to carry weight 0 — "something that
doesn't calculate shouldn't propagate." The :4793 GSLGNN returns a near-constant 0.5292 (the same
40-vs-315,209 imbalance that benches L2), so it carries no signal into the flow; at 0.15 it only
adds a fixed bias to every score. L4 joins L1/L2 as a benched non-voter, and a class-weighted
retrain that would make it calculate again is now optional, not owed. This is not absence: the
checkpoint (9e50723b) and the `gslgnn_inference_server.py` server are present (on ACER / in cube
9859b576). To keep rule 1 (the port never self-corrects ahead of the served source), the weight-0
change is made at the source — dropping the `l4` entry in asolaria-score.mjs:156 lets the
present-signals weighted mean renormalise to the remaining 85 on its own — and this port and its
referee re-mirror it then (one line each + a fixture regen).

Applied 2026-10-18. The owner's seat made the source edit (office receipt
`FABLE5-SCORER-L4-WEIGHT0-2026-10-18.hbp`: `WEIGHTS` is now line 157 with no `l4`; working-copy sha256
`99b3d8ec…` before, `74b8e906…` after, CRLF). The served file is vendored at
`gate/observed/vendor/acer-scorer-2026-10-18/` (LF; it is upstream blob `b101c098` plus that edit, and
the referee checks both against the receipt). `gate/observed/src/score.rs` `composite_e4` has no `l4`
slot, and `tools/score_referee.py` regenerates its fixture with `l4` present in 25,010 of 50,000
composite rows, none of which it moves. With `l4` unweighted every weight sum is odd in twentieths,
so no composite is an exact half (measured: 0 of 50,000).

## Referees

| Tool | Needs | Writes; `--check` diffs |
|---|---|---|
| `tools/quantize.py` | stdlib | `src/weights.rs`, from the checkpoint read by a restricted unpickler (only `collections.OrderedDict`, `torch._utils._rebuild_tensor_v2`, `torch.FloatStorage`); each weight is decoded from its float32 bits in integers |
| `tools/math_referee.py` | stdlib | `tests/fixtures/expected-math.hbp` (`LN2_Q62`, `SQRT32_F32`, 1,811 `SIGMOID`, 256 `BYTE` rows), each decided from integer bounds or rounded on integers |
| `tools/model_parity.py` | torch 2.14.1+cpu, torch_geometric 2.8.1 | `tests/fixtures/expected-model.hbp` (`LAYOUT`, 125 `GRAPH`, 64 `REQUEST`, 80 `BOUNDARY` rows); `--probe V1_CHECKPOINT` prints the probe rows above and writes nothing |

Every data file these tools write is HBP text; no JSON is read or written.

`tools/math_referee.py` computed its references with Python `decimal` at 60 digits, `32 ** 0.5` and
`byte / 255` in binary64 until 2026-10-18; it now uses integers only: `ln 2` from the series
`sum 1/(k * 2^k)` and `e^-t` from a Taylor series and 16 squarings, each with integer lower and upper
bounds at 128 bits and more until both round to the same Q24 value (all 1,811 `SIGMOID` rows and
`LN2_Q62` are decided at 128 bits), `math.isqrt` for `sqrt(32)`, and integer round-to-nearest-even for
the binary64 and float32 roundings. Regenerated, all 2,069 data rows of `expected-math.hbp` are
byte-identical to the committed rows; the one changed line is header line 3, which named the method
("Python decimal at 60 digits", now "decided from integer bounds"). `tools/quantize.py` (float32 bits
decoded in integers) and `tools/model_parity.py` (inputs built from integer-rounded bits, outputs read
as bits, the mask read from the served `gsl` module) regenerate `src/weights.rs` and
`expected-model.hbp` byte-identical, and the harness's `--seed` output for seeds `0x1234567`,
`0xC0FFEE`, `0x4793` and `0x51F15E` is byte-identical to the earlier float-built harness's.
