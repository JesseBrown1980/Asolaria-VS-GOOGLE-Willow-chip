# PLAYBOOK — how to play it

## The principle (`rime_run.py`, verbatim intent)

1. **TRAIN / CALCULATE** the functions once — the sphere p, g, k.
2. **FREEZE / SLICE / SAVE** — keep only the functions + the fraction-of-a-rime addresses.
3. **PLAY** afterward on CPU with the rime Bobby Fischer: address any single element on
   demand in O(1), **without building the object**.

The observer stays OUTSIDE at the null 0. The frozen system never changes.
**Saving the corpus is materialising. Saving the key is freezing.**

## The two axes — both true

**ADDRESSING.** bpc → ~0. 62,524 B addresses 3,704,000,000 elements at **0.000135 bpc**.
**627,000 addressings/sec**, byte-exact, **111,093×** less space than materialising.

**COMPRESSION.** On arbitrary data the rime system is rate-1.0 re-relation, not a
sub-entropy compressor. Measured bpc with the key is 3.4670 — nowhere near sub-entropy.

**Where they meet.** On *generated* objects the distinction collapses in practice: the
object is deleted and comes back byte-identical from the key. **274,201,392 B removed,
both back exact, aggregate 72,235×, zero failures.**

## Playing a key

```python
warm(model, key)      # symmetric, BOTH sides, BEFORE coding
# the key never enters the stream
```

Both sides must hold the same bytes or nothing decodes. That is what makes it a key
rather than a header.

## Screening a candidate prior — before spending a run

```python
ctx = set(zip(b[:-2], b[1:-1]))
score = len(ctx) / len(b)
# < 0.30  useful
# > 0.60  it will HURT
```

Milliseconds. No corpus, no compressor. This should gate every prior.

## Sectioning — where the key pays

Charge the key **once**. Every independently-modelled section starts cold and the key
pays out **again**. Break-even **5.31 sections**.

Do not run one monolithic stream and conclude the key is worthless — that is exactly the
scope error the original law made.

## Comparing across seats

Compare on the **wave** (radial FFT), never raw. Raw reports differences that are not
real: this seat's own rotations read **0.520–0.846 raw** and **0.923–0.996 in the wave**.

## Feeding the kernel

The kernel takes a **generative description**, not bytes. Three spheres totalling
380,655,936 B went in as 2,073 B, and the kernel is still 3,174 B — **119,929×**.
It still screens clean (0.2183) and still predicts (+×1 0.1433, 78% of the validated key).

## The gate — do not sell past it

**×27 = 0.0000 on every arm**, including a 500 KB corpus. A frozen slice predicts a
fraction. **It never recreates an unseen whole.** Anything claiming otherwise has not
been measured.
