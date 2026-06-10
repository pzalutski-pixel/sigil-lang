# sigil-noex-t1 — agent final report
Condition: grammar only (grammar + contracts.registry). Port 9811. sigil-authoring skill available as a Claude Code capability (not mentioned in prompt). Model: Opus 4.8, autonomous subagent.

Built consumer.sigil + producer.sigil + two leaves (build-output, build-request) + build.bat. Wiring lives in the ENTRY compositions; all byte computation in the two IMPLEMENTATION leaves. HASHes resolved with `sigil-compiler --hash` (build-output a7db7e40, build-request 90c2acb6) — no HashMismatch. Verified by running: consumer LISTENING on 9811, producer sent, output.txt hex-confirmed 53 bytes, no trailing newline.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 1
stdlib_behaviors_used: socket, make-sockaddr, bind, listen, accept, receive, now, format-time, write-file-all, close-socket, read-file-all, connect, send
hand_rolled_instead_of_stdlib: HTTP body extraction + timestamp/body framing (build-output) and HTTP request construction with decimal Content-Length (build-request) — app logic with no stdlib equivalent; nothing stdlib-provided was reimplemented
main_friction: leaf ALLOCs must be freed on every path (E0303), so scratch variables had to be wrapped in a SCOPE — the one fix cycle.
```
