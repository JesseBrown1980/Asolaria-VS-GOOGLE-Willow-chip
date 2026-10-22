# Rime Prism Pipeline — External Spec & Review (from Jesse's other session)

*Captured verbatim-in-substance from analysis Jesse provided from another chat, so the
combined record holds it alongside the implementation. Attribution: external reviewer
(another model/session). Included for completeness — Jesse asked for all code combined.*

## The intended deterministic flow (NOT random sampling)

```
observed black slice
  → three aligned components            (trijection: shared center + paid separations)
  → 27 prism coordinates                (nest the 3 axes → balanced-ternary RGB (r,g,b)∈{-1,0,1}^3)
  → PID / color-gradient addressing     (attach each of 27 cells to its PID/gradient identity)
  → rime-directional reprojection       (wind the address through the 27-field)
  → inverse prism                       (recombine 27 addressed channels)
  → calculated slice                    (first target: approximate white-hole counterpart)
```

Key distinction the reviewer stressed:
> The rime address changes **which projection is calculated**. It does not merely change
> the seed of a random sampler.

The 27 are **intermediate coordinates / pre-rime colors**, not 27 finished sampled documents.

## White-hole counterpart — defined by the address rule, not a metaphor

Not a literal mirror `W_white = 255 − W_black` (the repo's own experiments found literal
complement was the *worst* arm — inversion only carries information already inside it).
Instead:

```
W_white = P⁻¹( Wind_{a_white}( P(W_black) ) )
```

where `P` = exact 3→27 rime-prism decomposition, `a_white` = a precisely-defined
rime-directional address, `P⁻¹` = retranslation to the output domain. The **address rule**
defines "white", not byte inversion or generic text generation.

## Proposed MCP operations (deterministic)

```
freeze_black(corpus)          -> frozen field, center, charged residual, bank hash
split_three(frozen field)     -> three aligned directional components
prism_27(three components)    -> 27 exact pre-rime coordinates
attach_palette_pid(27 coords) -> 27 PID-addressed color/gradient channels
wind_rime(address, channels)  -> rime-directionally reprojected 27-channel field
unwind_rime(address, channels)-> inverse directional operation
inverse_prism_27(channels)    -> reconstructed / approximated output slice
verify(source, output, receipts) -> hashes, inverse closure, residual charge, controls
```

## Required controls (the honest bar)

1. **Identity address** reconstructs the original slice.
2. **Prism ∘ inverse-prism** is byte-exact.
3. The **address survives quantization** (don't collapse to the final 16-bit probability).
4. **3-channel and 27-channel** versions both emitted and measured.
5. **Literal mirror/complement** retained as a *losing* control.
6. **Random address / missing bank / shuffled PID palette** must fail or degrade.
7. **Full address vs 1/3, 1/9, 1/27 fractions** → the approximation curve.
8. White-hole output is called **approximate** until its operational definition and
   inverse receipts pass.

## How the committed implementation answers this (measured)

`rime_prism_pipeline.py` (committed) implements `prism_27`/`inverse_prism_27` as the
27-point NTT on the sphere with balanced-ternary RGB cells, and runs the controls:

- **C1 identity / C2 prism∘inverse:** byte-exact = **True** (lossless).
- **C3 approximation curve:** keeping 3/9/18/24/**26** of 27 coords recovers **0.0%** of
  bytes; only **27/27 → 100%**. The sphere-NTT **whitens** — every coordinate is
  essential, so a *fraction* of the coordinates does not reconstruct an arbitrary slice.
- **C4 losing controls:** random address, mirror-complement, and missing-bank all fail.

**Honest reading (from the controls, no opinion):** the prism is a real *lossless
addressing* transform; it does **not** reconstruct arbitrary/unknown data from a fraction.
That is what the reviewer's own required controls measure, and it is what they show.

The reviewer's separate correction is accepted: `rime_derive.py` is a **seeded generator**
(Markov sampling driven by `g^x/p`), not the prism reprojection — it is retained as the
*generation* arm (Law 18), distinct from this deterministic *addressing* pipeline.
