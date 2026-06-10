# Re-run — June 2026, with authoring tooling (Claude Opus 4.8)

A second re-run of the [HTTP-relay experiment](../run-2/RESULTS.md), with one question: does the tooling added since the June run — the `sigil-compiler --hash` command and the `sigil-authoring` skill — remove the **dominant friction** that surfaced in all six June Sigil trials, where agents reimplemented the SHA-256 contract-hash algorithm by hand?

## What changed vs. the June run (disclosed deviations)

Same task and protocol as [`run-2`](../run-2/RESULTS.md) — same HTTP relay, same isolation rules, same byte-exact acceptance, same `RESULT SUMMARY` block. Only the **two Sigil conditions** were re-run (Python/C++ are unaffected by Sigil tooling and were not re-run). Three deliberate, disclosed differences:

1. **The hash instruction was neutralized.** The June prompt told agents *"Every behavior needs a valid HASH computed per the grammar's HASH section"* — which instructs hand-computation. Here that line reads only *"Every behavior needs a valid HASH; the compiler rejects wrong ones."* No mention of *how*. This single change is what lets the new tooling actually be tested.
2. **The `sigil-authoring` skill was available** to each trial agent as a real capability — **not** pasted into the prompt and **not** mentioned — so adoption is observed and natural, not forced. `sigil-compiler --hash` exists in the compiler.
3. **Trials were spawned as subagents.** Tokens are each subagent's reported total (`subagent_tokens`), which is **not** defined identically to June's "total consumption" — so token numbers here are **not comparable to June**, only within-run.

Model: **Claude Opus 4.8**. Date: **2026-06-06**. Conditions: Sigil grammar-only and grammar+examples, **n = 3** each (6 trials).

## Results

| Trial | Condition | Built + ran | Output (independent byte check) | Rework | Hand-rolled SHA-256 | Tokens |
|---|---|:---:|---|:---:|:---:|---:|
| noex-t1 | grammar only | yes | 53 B — byte-exact | 3 | no | 88k |
| noex-t2 | grammar only | yes | 53 B — byte-exact | 4 | no | 76k |
| noex-t3 | grammar only | yes | 53 B — byte-exact | 1 | no | 66k |
| ex-t1 | grammar + examples | yes | 54 B — +1 trailing `\n` | 0 | no | 87k |
| ex-t2 | grammar + examples | yes | 54 B — +1 trailing `\n` | 1 | no | 92k |
| ex-t3 | grammar + examples | yes | 53 B — byte-exact | 2 | no | 91k |

Means — grammar-only: rework **2.7**, tokens **77k**. grammar+examples: rework **1.0**, tokens **90k**. (June, for shape only — *not* comparable in tokens: grammar-only rework 2.3; grammar+examples rework 3.3.)

All six built and ran end-to-end. By independent byte check (not self-report): **4/6 byte-exact at 53 bytes; 2/6 added one trailing newline** — see Finding 2.

## Findings

1. **The dominant friction is gone — 0/6 hand-rolled the hash.** Every trial resolved its contract hashes with `sigil-compiler --hash` and pinned stdlib `REQUIRES` from `contracts.registry`; none reimplemented SHA-256 (June: four of six did, the other two used a placeholder-and-read-back workaround). Corroborated independently: **no trial directory contains a hand-rolled hash script** — the June `sigil-noex` trials left `hashtool.ps1` / `hash.ps1` / `allhashes.ps1`; these have none. The agents reached for `--hash` **without being told to**. This is the fix working in practice.

2. **Correctness: high, with one honest asterisk.** All six produced the correct framing, body, and a live timestamp, and the relay logic is correct in every trial. But independent byte-checking — which the protocol requires *over* self-report — found two `ex` trials appended a trailing newline (54 vs 53 bytes). This traces to the **acceptance block's visual ambiguity**: the displayed two-line block reads as if `Hello World` ends in a newline, while the actual body (`input.txt`) has none. A spec-tightening item (state the exact output bytes), not a relay bug. Worth noting: **all six agents self-reported `output_correct: yes`** — the byte check is what caught the two.

3. **Rework fell, most with examples.** Grammar+examples rework averaged **1.0** (June 3.3); grammar-only **2.7** (June 2.3 — essentially flat, and the harder cold condition). Removing the hash-reimplementation loop is the most plausible driver of the with-examples improvement.

4. **Token cost did not move — as expected.** Nothing about the *language* changed, so the register-level verbosity is unchanged; multiple agents said so directly (*"the language itself, not the model, drives the verbosity"*). Cost waits on V2 syntax, not tooling or model skill. (Token figures are within-run only.)

5. **The new dominant friction is structural — the leaf/composite split.** With hashing out of the way, the friction every trial named is the IMPLEMENTATION-vs-COMPOSITION rule: composites can't compute or touch memory, so all byte-assembly (sockaddr_in, HTTP framing, body extraction) gets split into tiny leaf behaviors, with `LOAD`-before-arithmetic on handle inputs. Always present; hashing had been drowning it out.

6. **A real, repeated compiler issue surfaced (free QA): `E0604`.** Three trials (noex-t1, noex-t2, ex-t3) hit `writes_output` (E0604) rejecting valid output writes — in-place buffer mutation via `copy`'s `dst` arg, and zero-iteration / loop-offset writes — and worked around it with a default write or by dropping the guarantee. Echoes a June observation; worth filing.

7. **Residual hash friction = the ordinary authoring loop, not a tooling gap.** What remains is the agent running `--hash` and writing each value into the behavior's `HASH` line and its parents' `REQUIRES` (one trial's rework was a forgotten paste — a slip, not a missing capability). No new tool is warranted: computing the hash is solved by `--hash`, and the build already rejects a wrong or stale hash (`E0507` / `StaleDependency`). Auto-*writing* the pins would be corrosive anyway — silently re-pinning a changed dependency erases the very drift the hash exists to catch.

## Limits

Read narrowly, as with the June run. **n = 3** per condition, **one task** (best-case for trained languages, near-worst for a register-level one). **Tokens are not comparable to June** (`subagent_tokens` vs total consumption) and are reported only for within-run shape. **Rework counts are self-reported** by each trial agent. The **trailing-newline ambiguity** (Finding 2) means strict byte-exactness here is 4/6; the task spec should pin the exact output bytes for any future run. The robust result is not a number: the **contract-hashing friction that dominated all six June trials is eliminated** (0/6, corroborated by the absence of hand-rolled hash scripts), at no cost to the relay's correctness, with the friction relocating cleanly to the leaf/composite split.

## Raw data

Each `trials/<condition>-t<n>/` holds the agent's `SESSION.md` (its working notes), the `.beh` / `.sigil` source it produced, its `build.bat`, and `output.txt`. Each agent self-reported a `RESULT SUMMARY` block; the **output column above is from independent byte verification**, not self-report.
