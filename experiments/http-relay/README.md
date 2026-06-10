# Study: HTTP file relay

One coding task: a **producer** sends a file's contents as an HTTP POST to a
**consumer**, which prepends a timestamp + fixed framing and writes the result —
**byte-exact**. The program is written from scratch by an AI agent, and the task is
re-run as the toolchain advances. **Folder names are neutral (`run-N`); what each
run *is* lives in this file.** The narrative tying them together is
[The Sigil Experiment](../../docs/THE-SIGIL-EXPERIMENT.md).

**How a trial works — this is the whole point.** Each trial is **one independent AI
coding agent**, spawned fresh, given the relay task plus a **condition** that fixes
what it may read (a Python/C++ baseline the model already knows, Sigil with the
*grammar only*, or Sigil with *grammar + examples*) — Sigil always **cold**, zero
prior exposure. The agent **authors the entire program itself**; we then build and
run its output, check it byte-for-byte, and record rework cycles, the frictions it
hit, and tokens. `n` trials per condition. The measurement is *how an AI coder fares
under each condition* — not a demo of a finished program — which is also why these
runs surface real compiler/runtime bugs that unit tests and curated examples miss.

> Re-running a saved program later through its `build.bat` is a **regression test of
> that artifact** (did a toolchain change keep it building + byte-exact?), **not** a
> re-run of the study. Re-running the study means spawning agents again. Debug runs
> done that way are tests; their findings belong in the bug record, not a new run.

## Runs

Each run repeats the task on a more advanced toolchain. Read them in order.

| Run | Date | Model | n | Toolchain state | Result |
|---|---|---|:--:|---|---|
| **[run-1](run-1/EXPERIMENT.md)** | 2026-01 | Opus 4.5 | 1 | baseline (raw; hashes hand-computed) | suggestive only — n=1, tokens estimated after the fact |
| **[run-2](run-2/RESULTS.md)** | 2026-06 | Opus 4.8 | 3 | baseline, rigorous (isolated, compiled+run+byte-checked) | dominant friction = hand-rolled contract hashing (6/6) |
| **[run-3](run-3/RESULTS.md)** | 2026-06 | Opus 4.8 | 3 | + hash tooling (`--hash` + `sigil-authoring` skill) | 0/6 hand-rolled the hash; correctness held |
| **[run-4](run-4/RESULTS.md)** | 2026-06 | Opus 4.8 | 3 | + expanded stdlib (92 behaviors), skill as capability | 6/6 byte-exact; stdlib discovered + used; surfaced 4 engine issues as free QA |
| **[run-5](run-5/RESULTS.md)** | 2026-06 | Opus 4.8 | 3 | + bug fixes (validation of run-4's issues) | 6/6 byte-exact; all 4 defects gone; adds onboarding/authoring token split |

Notes:
- **run-1**'s relay sources have since been migrated and re-verified byte-exact on the current compiler; its token numbers remain a point-in-time snapshot (see the writeup's flags).
- **run-3** onward count tokens as `subagent_tokens` (within-run only, not comparable across runs); run-5 splits that into onboarding vs authoring (see its RESULTS.md).
- Each `run-N/trials/<condition>-t<n>/` holds the agent's source, `build.bat`, and `SESSION.md`. Build outputs and `output.txt` are git-ignored.
