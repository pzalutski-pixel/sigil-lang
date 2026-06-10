# sigil-noex-t3 — agent final report
Condition: grammar only. Port 9813. sigil-authoring skill available as capability. Model: Opus 4.8.

Built producer.sigil + consumer.sigil + leaves (post-header, frame-mid, crlf-needle, add-int, sub-int) + composites (build-post, extract-body, build-output). Verified 53 bytes, reproducible.

KEY FINDINGS:
1. Dynamic LOAD/STORE offsets didn't codegen in the agent's case: compiler warns "bounds cannot be verified" then codegen fails "Expected int immediate, got handle"; forced a redesign from hand-rolled copy loops to stdlib copy/find-based compositions. (NB: other behaviors DO use dynamic offsets — needs verification; may be a specific pattern.)
2. The `CALL f -> req` redirect form silently left a bytes OUTPUT full of uninitialized pointer junk; fixed by using the result form (h = CALL f) + explicit `copy h.hdr -> req`. The single biggest difficulty.
3. `CALL f -> compositionOutput` for an int OUTPUT tripped E0511/E0604; captured to a temp and SET it.
4. A "..." inside a DESCRIPTION is lexed as a string literal (unterminated-string error); quotes removed.
HASHes resolved via `sigil-compiler --hash`.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 6
stdlib_behaviors_used: read-file-all, socket, make-sockaddr, connect, send, close-socket, bind, listen, accept, receive, find, copy, now, format-time, write-file-all
hand_rolled_instead_of_stdlib: add-int and sub-int (no general int add/sub behavior for length math composites can't do); HTTP request/response framing and CRLF-CRLF body extraction (no HTTP behaviors — expected)
main_friction: the `-> output` redirect form silently leaves a bytes OUTPUT uninitialized (pointer junk), so every buffer-producing leaf had to be captured by result and explicitly copied — compounded by dynamic LOAD/STORE offsets not codegen-ing despite the grammar/warnings suggesting they are.
```
