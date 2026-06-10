# sigil-noex-t2 — agent final report
Condition: grammar only. Port 9812. sigil-authoring skill available as capability. Model: Opus 4.8. Independently re-built + re-run by the verifier: output.txt = 53 bytes byte-exact.

Consumer: socket → make-sockaddr(9812) → bind → listen → accept → receive → now → format-time → `build-output` → write-file-all. Producer: read-file-all → `build-request` → socket → make-sockaddr(9812,127.0.0.1) → connect → send → close-socket. Two leaves: `build-request` (assembles a minimal HTTP/1.0 POST with decimal Content-Length) and `build-output` (scans for `\r\n\r\n`, assembles `<ts[0..19]>` + ` PROCESSED BY CONSUMER` + 0x0A + body).

Rework cycles (3): one E0303 (ALLOC leak — free at single exit), one **E0104** (`IEQ body_len 0` on a raw INPUT handle — LOAD first), and the producer dir-vs-file invocation correction. No rework after the first clean build.

Token accounting: total 80,999; onboarding ~17.2k; authoring ~63.8k.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 3
stdlib_behaviors_used: read-file-all, socket, make-sockaddr, connect, send, close-socket, bind, listen, accept, receive, now, format-time, write-file-all
hand_rolled_instead_of_stdlib: HTTP request assembly and body-extraction/framing were hand-rolled in two leaf behaviors (no stdlib HTTP exists); int-to-decimal for Content-Length was hand-rolled in the leaf rather than calling stdlib int-to-string, since a COMPOSITION can't do the byte-splicing a leaf needs anyway
main_friction: INPUT int parameters arrive as handles, not values, so every scalar input had to be LOADed before use (E0104).
```
