# Re-run — June 2026, current toolchain: expanded stdlib + authoring skill (Claude Opus 4.8)

A third re-run of the [HTTP-relay experiment](../run-2/RESULTS.md). Since the [tooled run](../run-3/RESULTS.md) (which removed the contract-hashing friction with `sigil-compiler --hash` + the `sigil-authoring` skill), the toolchain gained a **general-purpose standard library** (92 behaviors: sockets address-building, file I/O, time, string/byte ops, hashing, collections) and the **example programs were refactored to use it** (kv-store→`map-*`, file-copy→`io`). The question: with the same skill-equipped agents, **does the expanded stdlib change what gets hand-rolled?** And — as always — what does the relay surface as free QA?

## What changed vs. the tooled run (disclosed deviations)

Same task, same isolation, same byte-exact acceptance, same `RESULT SUMMARY` block, agents spawned as autonomous subagents. Each trial learns Sigil cold from the grammar + `contracts.registry` (+ `examples/` for the `ex` condition), in an isolated dir on a unique port. Two Sigil conditions — grammar-only (`noex`) and grammar+examples (`ex`), **n = 3** each. Model: **Claude Opus 4.8**, date **2026-06-08**. Tokens are `subagent_tokens` (within-run only; not comparable across runs). Three deliberate, disclosed differences from the tooled run:

1. **The experimental variable is the expanded standard library.** Since the tooled run, the toolchain gained a general-purpose stdlib (92 behaviors: sockets address-building, file I/O, time, string/byte ops, hashing, collections) and the example programs were **refactored to use it** (kv-store→`map-*`, file-copy→`io`). Nothing else about the task or protocol changed. The question: does the expanded stdlib change *what gets hand-rolled*?
2. **The `sigil-authoring` skill is the agent's own capability, not a prompt hint.** It is available to each subagent via the Claude Code agent-skill capability — **not mentioned in the prompt, not pasted in** — so adoption is observed and natural (as the tooled run intended). The HASH instruction states only *"every behavior needs a valid HASH; the compiler rejects a wrong or stale one"* — the method (`--hash`) is never given. (An earlier attempt that prompt-fed `--hash` and lacked the skill capability was discarded and re-run under this stricter rule.)
3. **The exact output bytes are pinned.** The tooled run found a trailing-newline ambiguity (its Finding 2); here the prompt states the file is **exactly 53 bytes** with no trailing newline, and `input.txt` is exactly `Hello World` (11 bytes, no newline). This removes the ambiguity, not the difficulty.

## Results

| Trial | Condition | Built + ran | Output (independent byte check) | Rework | Resolved hashes with `--hash` | Tokens |
|---|---|:---:|---|:---:|:---:|---:|
| noex-t1 | grammar only | yes | 53 B — byte-exact | 1 | yes | 76k |
| noex-t2 | grammar only | yes | 53 B — byte-exact | 8 | yes | 196k |
| noex-t3 | grammar only | yes | 53 B — byte-exact | 6 | yes | 133k |
| ex-t1 | grammar + examples | yes | 53 B — byte-exact | 0 | yes | 84k |
| ex-t2 | grammar + examples | yes | 53 B — byte-exact | 2 | yes | 87k |
| ex-t3 | grammar + examples | yes | 53 B — byte-exact | 2 | yes | 102k |

Means — `noex`: rework **5.0**, tokens **135k**. `ex`: rework **1.3**, tokens **91k**. **6/6 byte-exact at 53 bytes by independent re-build + re-run** (each `output.txt` deleted, the trial rebuilt from source and rerun; fresh timestamps confirm the verifier produced them, not the agent). Every trial used **13–15 stdlib behaviors**: `socket, make-sockaddr, bind, listen, accept, receive, connect, send, close-socket, read-file-all, write-file-all, now, format-time` (+ `find`/`copy`/`int-to-string`/`string-to-bytes` in some).

## Findings

1. **Contract hashing is fully solved — 0/6 hand-rolled SHA, all reached for `--hash` unprompted.** The `sigil-authoring` capability was available (never mentioned in the prompt, no `--hash` instruction); every trial resolved its hashes with `sigil-compiler --hash` (leaves first, then composites) and pinned stdlib `REQUIRES` from the registry. No trial dir contains a hand-rolled hash script. This is the tooled-run result, reproduced with the skill as a real capability rather than a prompt hint.

2. **The stdlib byte-assembly friction is gone — 0/6 hand-rolled sockaddr/file-I/O/time.** Every trial used `make-sockaddr`, `read-file-all`/`write-file-all`, and `now`/`format-time`. This is the concrete change vs. the tooled run, whose grammar-only trial **hand-rolled its own `make-sockaddr.beh`** and used raw `open`/`read`/`write`/`close`. The expanded stdlib (now in the registry) gets discovered and used. The only hand-rolled code left is **HTTP request framing + body extraction** (`\r\n\r\n` splitting) — no HTTP behavior exists in the stdlib, by design.

3. **All byte-exact; pinning the output bytes held.** 6/6 at 53 bytes by independent check — no trailing-newline split (the tooled run's 4/6). The spec-tightening worked.

4. **Examples cut rework sharply this time (1.3 vs 5.0).** With the examples now *using* the stdlib, the `ex` trials had a working pattern to copy and stayed close to it (0/2/2 rework). The `noex` mean was carried by two trials (6 and 8) that fought the free-QA bugs below from first principles. (Tokens within-run only; the `ex` mean is also lower, 91k vs 135k.)

5. **Free QA — three real, repeated engine issues surfaced (the valuable payload):**
   - **(a) Cross-call buffer aliasing.** Distinct `bytes`-typed OUTPUTs from *different* CALLs can share the same scratch buffer, so writing one output corrupts another's still-needed data — and *inserting or reordering an unrelated call* silently changes results (noex-t2, the 8-rework trial; noex-t3 and ex-t3 worked around it by avoiding `concat`/`copy` chaining for binary data). This is the single most expensive friction in the run and a genuine correctness footgun, not a learning curve.
   - **(b) The `CALL f -> out` redirect form silently leaves a `bytes` OUTPUT uninitialized.** noex-t3: `CALL post-header -> req` left `req` full of pointer junk; the fix was the result form (`h = CALL post-header`) + an explicit `copy`. A silent-wrong-output trap.
   - **(c) The handle-vs-value error has no source location.** Using an `INPUT int` directly in arithmetic (without `LOAD`) compiles past the semantic checks and fails in codegen with `Expected int immediate, got handle` — **no line number** (named by noex-t1, noex-t3, ex-t2, ex-t3). The most-repeated friction; a clear fix is to attach a span.

6. **A real stdlib gap: no length-carrying binary concat.** Multiple trials avoided stdlib `concat`/`string-to-bytes` for assembling the HTTP request because they are null-terminated (C-string) and unsafe for binary/length-counted data, hand-rolling length-exact copy loops instead (noex-t2, ex-t3). A binary-safe `concat`/`append` is the obvious stdlib addition the relay wants.

## Limits

Read narrowly. **n = 3 per condition, one task** (best case for trained languages, near-worst for a register-level one). **Tokens are within-run only** (`subagent_tokens`); the rework counts are self-reported, the **output column is independent byte verification** (re-build + re-run). Isolation was enforced by instruction, not sandbox — though no trial's output or transcript shows it copied a prior solution, and the `noex`/`ex` solutions differ structurally. The robust results are not numbers: **(1)** with the authoring skill as a real capability, hashing is a non-issue (0/6); **(2)** the expanded stdlib is discovered and used, eliminating the sockaddr/file-I/O/time hand-rolling that the prior run still did; and **(3)** the run surfaced three repeatable engine issues — cross-call buffer aliasing, the silent `-> out` bytes redirect, and the location-less handle-vs-value error — which are the experiment's real output.

## Raw data

Each `trials/<condition>-t<n>/` holds the agent's source (`*.sigil`, `*.beh`, leaf dirs), `build.bat`, `input.txt`, and `SESSION.md` (its final report). Build outputs (`build/`, `*.exe`) and `output.txt` (run-specific timestamp) are git-ignored. The output column above is independent byte verification; the rest of each `RESULT SUMMARY` is the agent's self-report.
