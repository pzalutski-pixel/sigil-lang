# sigil-ex-t2 — agent final report
Condition: grammar + examples. Port 9822. sigil-authoring skill available as capability. Model: Opus 4.8. Independently re-built + re-run by the verifier: output.txt = 53 bytes byte-exact.

Consumer (`consumer/`): socket → make-sockaddr 9822 → bind → listen → accept → receive → `extract-body` (find `\r\n\r\n`, copy body) → now → format-time → `build-output` (`<19-char ts>` + ` PROCESSED BY CONSUMER` + 0x0A + body) → write-file-all → close-socket. Producer (`producer/`): read-file-all → int-to-string (Content-Length) → `build-request` (POST / HTTP/1.1 + headers + body) → socket → make-sockaddr 9822 (127.0.0.1=16777343) → connect → send → close-socket. 3 leaves + 2 composite mains + 2 entry points. HASHes resolved with `sigil-compiler --hash` (leaves first, then composites with pinned REQUIRES). Noted: `format-time` takes epoch ms directly, so `now`'s output feeds in with no conversion.

Rework cycles (1): **E0104** + E0604 — LOAD input int handles before comparing, and write the OUTPUT on the not-found path.

Token accounting: total 102,526; onboarding ~22.7k; authoring ~79.8k.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes — actual byte check (od -tx1 confirmed 53 bytes, last byte 0x64, no trailing newline)
output_bytes: 53
rework_iterations: 1
stdlib_behaviors_used: socket, make-sockaddr, bind, listen, accept, receive, now, format-time, write-file-all, close-socket, println, read-file-all, int-to-string, connect, send
hand_rolled_instead_of_stdlib: HTTP body extraction (extract-body leaf — no stdlib HTTP parser; find/split operate on null-terminated/byte-offset data, not raw fixed-length buffers) and the byte assembly leaves (build-output, build-request), inherently program-specific framing
main_friction: Composites can't touch memory, so every byte-level concatenation/parse had to be pushed into hand-written IMPLEMENTATION leaves, including remembering to LOAD input int handles before comparing (the one rework cycle: E0104 + E0604).
```
