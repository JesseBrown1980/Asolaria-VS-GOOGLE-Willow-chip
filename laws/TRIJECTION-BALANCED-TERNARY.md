# The Trijection & Balanced Ternary — sealed proof (2026-07-21)

Operator: Jesse Daniel Brown. For liris, acer, and the federated Claude clouds.
Integer-only codec → bit-identical on any CPU / Rust version (the post-1.81 float
codegen issue cannot touch this). `restore=OK` on every case (lossless bijection).

## The idea

A **bijection** is 2 points + an implied center (an involution, `I(I(z)) = z`).
The **trijection** is 3 points + an **explicit** center. For a triple `(a,b,c)`:

```
s   = a + b + c
c0  = s // 3        # the CENTER (centroid) — one value
rem = s % 3         # 2-bit remainder {0,1,2}
sa  = a - c0        # separation of point 1
sb  = b - c0        # separation of point 2
c   = c0 + rem - sa - sb     # the THIRD point is FREE — determined, not stored
```

So `(a,b,c) ⟷ (c0, sa, sb, rem)` exactly. **Centroid free, separation paid.**

Balanced ternary `{−1, 0, +1}` is the native code: the `0` digit *is* the free
center. **3 trits = 3³ = 27 = the cube:** `1 center + 6 faces + 12 edges + 8
corners`, where separation-from-center = count of nonzero trits = information paid.

## The measurement (see `trijection.py`, 300,000 B/case, order-0 + lzma, byte-exact)

**(A) WRONG use — arbitrary consecutive-byte triples (no shared center): EXPANDS.**

| source | order-0 raw | order-0 trijection | verdict |
|---|---|---|---|
| gradient | 5.8638 | 6.9516 | no save |
| chaos (random) | 7.9994 | 8.3035 | no save |
| english | 5.0404 | 6.6006 | no save |
| rustcode | 4.9768 | 6.6586 | no save |

Consecutive bytes do not share a center, so the separations get *wider* range than
the originals. But the balanced-ternary zero-trit fraction already shows the signal
is real: english 0.602, rustcode 0.609 vs random 0.460 — structured data carries a
free center; arbitrary grouping just fails to monetize it.

**(B) RIGHT use — 3 aligned vantages sharing a center: SAVES.**

```
order-0 bpc   raw = 7.4752  →  trijection = 3.8966   (−3.5786)   SAVES
lzma    bpc   raw = 0.1780  →  trijection = 0.1188   (−0.0592)   SAVES
balanced-ternary zero-trit fraction = 0.900   (free center dominates)
restore = OK
```

## The law (kept honest)

The trijection reduces cost **iff the three genuinely share a center** (aligned,
correlated vantages). Then separations collapse to mostly-zero trits and the third
point is free. With no shared center there is no win. It **never beats the joint
entropy** — it *reaches* the joint entropy by not paying for the shared center three
times. Nested 3× → the 27-jection cube, a hierarchy of free centers.

Design consequence: to make a 27-vantage cube "divide below 1," the 27 must share a
deep center (a common functional substrate — the glyph layer), NOT be 27 unrelated
universes. Shared-center diversity divides; random diversity only differs.

## Reproduce

```
python3 trijection.py        # deterministic; prints A (wrong) and B (right) cases
```

## The nested cascade — reduction law 27 → 9 → 3 → 1 (`nested_cascade.py`)

Rule of 3 nested three deep. 27 separate universes sharing ONE omniverse-center,
collapsed via free centers. All `restore=OK`, integer-only, deterministic.

**27 shared-center universes:** raw 7.4479 → cascade 2.0106 bpc = **3.70×**,
free-center zero-trit 0.900, levels 27→9→3→1.

**Scaling (reduction grows with machines added, all sharing the center):**

| machines | raw bpc | cascade bpc | reduction | free-center |
|---|---|---|---|---|
| 3  | 7.4729 | 3.8965 | 1.92× | 0.787 |
| 9  | 7.4580 | 2.5137 | 2.97× | 0.871 |
| 27 | 7.4479 | 2.0106 | 3.70× | 0.900 |
| 81 | 7.4462 | 1.8036 | 4.13× | 0.911 |

**Control — 27 INDEPENDENT universes (no shared center):** raw 7.9963 →
cascade 8.3361 = **0.96× (expands)**. No shared center → no reduction. The law
fires only on separate universes of one omniverse.

**Reading:** reduction grows ~log(N) (one free-center layer per nesting level).
The center amortizes toward zero per machine; the floor is each universe's own
separation entropy (its unique contribution). Never below the joint entropy —
it stops paying for the shared center N−1 extra times. "Spend one universe to
get the universe."

Reproduce: `python3 nested_cascade.py`

## The 27-JECTION (flat) beats the nested cascade — head to head (`njection.py`)

Direct N-jection: N universes -> 1 grand center (mean of ALL N) + N-1 separations,
the Nth FREE. Head-to-head vs nested 27->9->3->1, same universes, all restore=OK:

| machines | flat N-jection | nested cascade | winner |
|---|---|---|---|
| 3  | 1.92× | 1.92× | tie |
| 9  | 3.01× | 2.97× | flat |
| 27 | **3.92×** | 3.70× | **flat** |
| 81 | 4.34× | 4.13× | flat |
| control (independent) | 0.98× | 0.96× | neither reduces (correct) |

WHY FLAT WINS: the grand mean of all N is the lowest-variance estimate of the
shared omniverse signal, so every separation from that single center is minimal.
Nested uses local (triple) centers — noisier estimates — so its intermediate
separations cost more. One outside viewer seeing all N at once beats a hierarchy
of viewers each seeing only 3. => the canonical operation is the flat N-JECTION.

Boundary unchanged: reduction grows ~log(N) (diminishing), floor = each universe's
own separation entropy, never below the joint entropy, and the control (no shared
center) does not reduce. Reproduce: `python3 njection.py`

## The trianti — symmetry group of the 27-cube (`trianti.py`)

27 = 3^3 -> one base-27 digit = 3 balanced trits. Two inversions, by ORDER:

| operation | what | order | anti-inversion |
|---|---|---|---|
| **N** negation `t→−t` | reflect through center (binary-style) | **2** | itself (self-inverse) |
| **R** rotation `(a,b,c)→(c,a,b)` | trinary-native 3-cycle | **3** | **R² = R⁻¹ (distinct!)** |

Key result (verified byte-exact on all 27 cells): `N·N = id`, `R·R·R = id`, and
**`R ≠ R⁻¹`** — unlike binary, the trinary inversion is NOT its own anti. The
**anti-inversion is the counter-rotation R² ("trianti")**, and `R⁰ + R¹ + R²` =
1/3 + 1/3 + 1/3 → closes to unity (the **pie = 1**): `R · R⁻¹ = id`.

**Fixed points:** R fixes the main diagonal `{(−1,−1,−1), (0,0,0), (+1,+1,+1)}`
(orbit sizes {1:3, 3:8} = 3 fixed + 8 full 3-cycles = 27). The FULL group {N,R,R²}
fixes only `(0,0,0)`. **The free center is the fixed point of the entire symmetry
group** — which is exactly why it costs nothing: it moves under no inversion.

Honest line: N, R, R² are lossless permutations of the 27 cells (bijective
symmetries, rate 1.0 — re-relation, not sub-entropy). They give the trijection its
STRUCTURE (order-3 with a distinct anti), not compression below entropy. But that
structure is precisely what a 2-machine bijection cannot provide.

Reproduce: `python3 trianti.py`
