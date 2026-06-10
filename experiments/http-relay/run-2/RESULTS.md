# Re-run — June 2026 (Claude Opus 4.8)

A controlled re-run of the [original experiment](../run-1/EXPERIMENT.md), prompted by a simple question: the first run used Claude Opus 4.5 (Jan 2026); current models are stronger — **does the result change?**

## Method

Same task as the original — an **HTTP file relay** (producer reads `input.txt`, POSTs it; consumer prepends `<timestamp> PROCESSED BY CONSUMER\n` and writes `output.txt`). Four conditions: Python and C++ (trained languages), and Sigil with grammar-only vs. grammar-plus-examples access.

Differences from the original — all intended as improvements:

- **n = 3** per condition (12 trials total).
- Each trial is an **autonomous agent** working in an isolated directory (`trials/<condition>-t<n>/`) on a **unique port**, so trials can't interfere.
- **Every solution was compiled and run end-to-end** and its output checked against the acceptance output byte-for-byte — *modulo the run-specific timestamp the task injects*: the body, the `PROCESSED BY CONSUMER` framing, and the byte layout must match exactly, only the timestamp differs between runs (so "byte-exact" below means "matched that acceptance check," not a single fixed golden file across conditions). This is possible for all four conditions because the Sigil toolchain now builds and links on a normal machine (see [STATUS.md](../../../docs/STATUS.md)); the original couldn't fully verify Sigil locally.
- **Tokens** are each agent's **total** consumption (reading references + writing + self-verification), measured uniformly. This is a *cleaner* figure than the original's `/context` "messages" reading — but for the same reason it is **not comparable to the original's numbers**. Only within-run ratios and the rework/bug counts compare across runs.
- Isolation was enforced per condition: grammar-only agents could read **only** the grammar + `contracts.registry`; with-examples agents could additionally read `examples/` but **not** `experiments/`. (One early with-examples trial was discarded after it found and adapted a previously-solved copy of this exact task; it was re-run under the stricter rule.)

Model: **Claude Opus 4.8** (the agents inherited the session model). Date: 2026-06-04.

## Results

| Condition | Trial tokens (t1 / t2 / t3) | Mean | Within-run ratio | Mean rework | Logic bugs | Success |
|-----------|----------------------------:|-----:|:----------------:|:-----------:|:----------:|:-------:|
| Python | 19.4k / 18.9k / 17.2k | **18.5k** | 1.0× | 0.0 | 0 | 3/3 |
| C++ | 25.7k / 21.5k / 22.8k | **23.3k** | 1.3× | 0.3 | 0 | 3/3 |
| Sigil (grammar only) | 73.1k / 100.5k / 100.3k | **91.3k** | 4.9× | 2.3 | 0 | 3/3 |
| Sigil (grammar + examples) | 89.9k / 83.9k / 86.5k | **86.8k** | 4.7× | 3.3 | 0 | 3/3 |

All 12 trials produced byte-exact output.

## Findings

1. **Reliability went from shaky to total.** The original's with-examples run shipped 3 logic bugs and needed intervention; here **all 12 trials succeeded, zero logic bugs**.

2. **Language-learning friction collapsed** but **token cost did not.** Sigil rework fell from the original's 12 cycles to ~2 (grammar-only), yet Sigil still costs **~5× Python** within the run. A stronger model writes the same volume of register-level code, so *model skill* is not the lever. But this isn't a clean "the language is 5× worse" verdict: **neither model was ever trained on Sigil** — both learned it cold from the spec, against languages the model has effectively pre-compressed through pre-training. So the ~5× is a mix of genuine register-level verbosity and the total absence of any training signal, and the experiment can't separate them. Whether training on Sigil would close the gap is untested, and is the obvious next experiment.

3. **The dominant remaining cost is a tooling gap — contract hashing — that surfaced in all 6 Sigil trials.** Four of the six independently reimplemented the SHA-256 **contract-hash** algorithm from the spec; the other two worked around the missing tool by writing placeholder hashes and copying back the compiler's reported value. The friction is real either way — there was no usable tool, and the `--fix-hashes` flag was buggy (one agent: *"--fix-hashes mangles zero placeholders"*). Contract hashing is a sound idea; computing hashes by hand is the friction. (The `--hash` tool added since formalizes the workaround the two trials improvised.)

4. **Examples don't reduce cost — confirmed cleanly.** With-examples (86.8k) ≈ grammar-only (91.3k). Examples gave socket boilerplate but couldn't touch the real costs (hashing, byte-level string building).

5. **Recurring language friction** (independently reported by multiple grammar-only agents): the IMPLEMENTATION-vs-COMPOSITION split (leaves have primitives but no CALL; compositions have CALL but no memory/arithmetic) forces all byte work into leaf behaviors; no literal-to-buffer primitive; multi-output CALLs use field access, not destructuring.

6. **Free QA — two real compiler issues surfaced:** labels declared inside a `SCOPE` block are silently unresolvable by codegen (`Undefined label`); and the `writes_output` check rejects some unconditional loop/offset-based output writes.

## Limits

Read the numbers narrowly. This is **one task** — an HTTP file relay, deliberately chosen because trained languages handle it trivially. That makes it close to a *best case* for Python/C++ and a *near-worst case* for a register-level language (raw sockets, hand-rolled HTTP framing, byte-level string building); a different task could move the ratio either way, and a second task was not run. **n = 3** is enough to show the gap is large and repeatable, not to pin it: the grammar-only trials alone ran 73.1k / 100.5k / 100.3k — a ~37% spread, with the mean carried by the two slower runs, and no dispersion is reported beyond that. And `rework` / `logic bugs` come from each trial agent's own end-of-run summary — a self-report with no formal definition of a "rework cycle." None of this overturns the headline (Sigil ~5× the token cost; a stronger model fixes correctness but not cost), but it bounds how far that headline travels: it is a result about *this task, this model family, at this scale* — not a general measurement of the language's intrinsic efficiency. The most robust thing here is not the 5×; it is the single dominant **contract-hashing** friction, which surfaced in all six Sigil trials (four hand-reimplemented SHA-256, two used a placeholder-and-read-back workaround).

## Raw data

Each trial directory `trials/<condition>-t<n>/` contains:

- **`SESSION.md`** — the faithful transcript of that trial agent, the evidence of how the run actually proceeded (rendered from the session log; tool outputs over 4,000 chars are truncated with a marker).
- **`src/`** (or the agent's chosen layout) — the source the agent produced.

Build outputs (`build/`, `*.exe`, `output.txt`) are git-ignored. The `RESULT SUMMARY` block at the end of each transcript — `compiled_or_ran`, `output_correct`, `rework_iterations`, and the main friction — is the basis of the results table above.
