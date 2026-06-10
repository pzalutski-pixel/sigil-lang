# Re-run — June 2026, bug-fixed compiler: validating the four defects + onboarding/authoring token split (Claude Opus 4.8)

A validation re-run of the [stdlib re-run](../run-4/RESULTS.md). That run's real payload was four engine issues it surfaced as free QA; all four were then investigated through code and the three real ones fixed (the fourth was a misdiagnosis). **A trial whose rework was driven by a compiler defect isn't measuring "can an AI write Sigil" — it's measuring "can an AI fight a broken compiler," which the trained-language baselines never do.** So those trials are invalid data. This run repeats the *identical* protocol on the *fixed* compiler to confirm the defects are gone, and introduces a cleaner token accounting (onboarding vs authoring).

## What changed since the stdlib run

Only the compiler. Same task, same isolation, same byte-exact acceptance (53 bytes), same `RESULT SUMMARY` block, agents spawned as autonomous subagents with the `sigil-authoring` skill as a real Claude Code capability (never named in the prompt, `--hash` method never given). Conditions = grammar-only (`noex`) + grammar+examples (`ex`), **n = 3** each. Model **Claude Opus 4.8**, date **2026-06-09**. Fresh trial dirs, ports 9811–9813 / 9821–9823. All six **independently re-built from source and re-run** by the verifier (not self-report).

The four defects the prior run surfaced, and their status going into this run:

- **(a) "cross-call buffer aliasing" — was a misdiagnosis, no fix.** Disproven through code: the scope allocator is `malloc`-per-allocation, freed only at scope exit (`runtime/src/memory.c`), so distinct CALL outputs are independent regions and cannot alias; a direct probe (one CALL's output fed as another's input, the second overwriting its own output and echoing the input back) returned the input intact at 16 B and 64 KB. The prior run's noex-t2 hit real friction (8 reworks) but attributed it to a mechanism that does not exist.
- **(b) `CALL f -> out` redirect into a composition OUTPUT — fixed** (`0d45cda`): was E0604+E0511 plus codegen leaving a `bytes` OUTPUT as junk; §7.8 makes writing the call result into the composition's own OUTPUT a valid consumption, and codegen now copies into the output buffer.
- **(c) handle-vs-value, location-less codegen panic — fixed** (`30a445f`, `be628ba`): `IADD x 1` on an un-LOADed INPUT (and a handle as a dynamic LOAD/STORE offset) passed semantic then died in codegen with `Expected int immediate, got handle` and no line; now a front-end **E0104** at the source line with a LOAD hint.
- **DESCRIPTION stray-quote lex error — fixed** (`39eeeae`): a lone or line-spanning `"` in a DESCRIPTION no longer raises "unterminated string"; the block is lexed as free text.

## Results

| Trial | Condition | Built + ran | Output (independent byte check) | Rework | Onboarding (est.) | Authoring (est.) | Total tokens |
|---|---|:---:|---|:---:|---:|---:|---:|
| noex-t1 | grammar only | yes | 53 B — byte-exact | 2 | ~17.2k | ~79.3k | 96,456 |
| noex-t2 | grammar only | yes | 53 B — byte-exact | 3 | ~17.2k | ~63.8k | 80,999 |
| noex-t3 | grammar only | yes | 53 B — byte-exact | 1 | ~17.2k | ~71.1k | 88,258 |
| ex-t1 | grammar + examples | yes | 53 B — byte-exact | 3 | ~22.7k | ~72.6k | 95,316 |
| ex-t2 | grammar + examples | yes | 53 B — byte-exact | 1 | ~22.7k | ~79.8k | 102,526 |
| ex-t3 | grammar + examples | yes | 53 B — byte-exact | 2 | ~22.7k | ~69.3k | 91,982 |

Means — `noex`: rework **2.0**, authoring **~71.4k**, total **88.6k**. `ex`: rework **2.0**, authoring **~73.9k**, total **96.6k**. **6/6 byte-exact at 53 bytes by independent re-build + re-run.** Every trial used the stdlib's `socket / make-sockaddr / bind / listen / accept / receive / connect / send / close-socket / read-file-all / write-file-all / now / format-time` (+ `int-to-string` in most).

## Why we record onboarding and authoring separately

A trained language (Python, C++) carries its entire definition in the model's weights — paid once at training time, amortized across every program ever written, costing **zero tokens** at inference. Sigil has zero training data, so each run must load its grammar (and, in `ex`, the examples) into context. Reporting one conflated token total therefore taxes Sigil for re-reading its own manual and compares it against a baseline that pays nothing for the same thing. We split the cost:

- **Onboarding** — the one-time read of the language material: the grammar (`SIGIL-LANGUAGE-REFERENCE.md`, ~9.1k tok) + `contracts.registry` (~8.1k tok) for both conditions, plus the `examples/` (~5.5k tok) for `ex`. This is Sigil's stand-in for training data; it amortizes toward zero as more programs are written against the same loaded context. **Estimate** (token ≈ chars/4 of the material the condition loads), not a meter reading — hence "est."
- **Authoring** — `total − onboarding` — the actual per-program work (reading the task, generating behaviors, hashing, fixing, verifying). This is the figure comparable to a trained language's *total*.

**Honest caveats on the estimate.** (1) The harness reports one conflated `subagent_tokens` per trial, not an input/output/cache breakdown, so onboarding is subtracted as a one-time constant; but in a multi-turn agent the grammar sits in context every turn, so `authoring` still carries the cost of re-processing it each turn — a "not trained" cost — making `authoring` a **conservative upper bound** (it overstates Sigil's true marginal cost, never understates it). (2) `onboarding` is the material the condition *loads*; an agent may not read all of it, so it too is an upper bound. The cleanest confound-free metric — generated (output) tokens only, which never contain the grammar — requires an input/output split the harness does not currently expose; capturing it is the next instrumentation step. We do **not** retroactively apply this split to earlier runs; it starts here.

## Findings

1. **All four defects are gone.** 0/6 reported buffer aliasing (the prior run's dominant friction; noex-t2 fell from 8 reworks to 3). 0/6 hit the `-> out` redirect junk. 0/6 hit a DESCRIPTION lex error. The handle-vs-value issue now appears as a clean, located **E0104** — five of six trials named it explicitly and fixed it in a single `LOAD`, where before it was a location-less codegen panic. The error message does its job: it converts a debugging dead-end into a one-line fix.
2. **Rework collapsed on the grammar-only condition: 5.0 → 2.0.** The drop is the validation — the prior `noex` mean was carried by trials fighting (a)/(b)/(c) from first principles; with the bugs fixed and the errors now located, no trial exceeded 3 reworks. (`ex` moved 1.3 → 2.0, within noise at n=3; its remaining reworks are the sound by-design checks below, not defects.)
3. **The remaining friction is structural, not bugs.** Every trial's `main_friction` is now the same legitimate design tension: the leaf/composite split — byte assembly must live in `IMPLEMENTATION` leaves that cannot `CALL` stdlib helpers, so number formatting and buffer building get hand-written in leaves even though `int-to-string`/`copy`/`find` exist. Paired with the sound checks the trials hit and fixed quickly — E0104 (LOAD a scalar INPUT before using its value), E0303 (free every ALLOC on all paths), E0604 (write every OUTPUT on all paths) — these are the language working as designed, not engine faults.
4. **The real engine-shaped follow-ups are now clear** (carried to the dev plan, not this run): the underlying "a scalar INPUT is a handle you must LOAD" verbosity behind E0104 (the V2 *scalars-are-values* idea), and the stdlib gap of a length-carrying **binary** concat (`concat` is null-terminated, unsafe for binary), which is why the request/response framing stays hand-rolled.

## Limits

Read narrowly. **n = 3 per condition, one task** (best case for trained languages, near-worst for a register-level one). The output column is independent byte verification (re-build + re-run); rework counts are self-reported. **Token figures: `total` is the harness's conflated `subagent_tokens` (within-run only, not comparable across runs); `onboarding` is a chars/4 estimate of the loaded material; `authoring` is `total − onboarding`, a conservative upper bound (see the token note).** Isolation was enforced by instruction, not sandbox. The load-bearing result here is qualitative and is a *validation*: the four compiler defects the prior run surfaced are confirmed gone — the run that found them measured the bugs; this run, on the fixed compiler, measures the language.

## Raw data

Each `trials/<condition>-t<n>/` holds the agent's source (`*.sigil`, `*.beh`, leaf dirs), `build.bat`, `input.txt`, and `SESSION.md` (its final report incl. `RESULT SUMMARY`). Build outputs (`build/`, `*.exe`) and `output.txt` (run-specific timestamp) are git-ignored. The output column above is independent byte verification; the rest of each `RESULT SUMMARY` is the agent's self-report.
