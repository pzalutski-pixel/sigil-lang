# sigil-ex-t3 — agent final report
Condition: grammar + examples. Port 9823. sigil-authoring skill available as capability. Model: Opus 4.8. Independently re-built + re-run by the verifier (LIB unset so lld-link is used): output.txt = 53 bytes byte-exact.

`consumer.sigil` / `producer.sigil` + `behaviors/`: `consumer-main` (socket/bind 9823/listen/accept/receive, now→format-time, assemble-output, write-file-all); `producer-main` (read-file-all, build-request, socket/connect 127.0.0.1:9823/send); `assemble-output` leaf (find `\r\n\r\n`, write `<timestamp> PROCESSED BY CONSUMER\n<body>`); `build-request` leaf (HTTP POST with file body, Content-Length by hand).

Rework cycles (2): both in `build-request.beh` — (1) `tmp` LOADed without a prior STORE on all paths (E0604), (2) `tmp` never ALLOC'd (E0303), for the digit-count scratch handle. The consumer compiled + ran correctly first build.

Token accounting: total 91,982; onboarding ~22.7k; authoring ~69.3k.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes — byte-exact hex dump confirmed (53 bytes, 19-char timestamp + " PROCESSED BY CONSUMER" + 0x0A + "Hello World", no trailing newline)
output_bytes: 53
rework_iterations: 2
stdlib_behaviors_used: socket, make-sockaddr, bind, listen, accept, receive, now, format-time, write-file-all, close-socket, println, read-file-all, connect, send
hand_rolled_instead_of_stdlib: int-to-decimal digit formatting inside build-request (stdlib int-to-string exists but can't be CALLed from a leaf; the request assembly itself needed a leaf, so the Content-Length number was done by hand there)
main_friction: the leaf/composite split — byte assembly (finding the body, framing the output, formatting Content-Length) must live in IMPLEMENTATION leaves that cannot CALL stdlib helpers, so number formatting and buffer building were hand-written, and both rework cycles were a forgotten ALLOC + uninitialized STORE for the digit-count scratch handle.
```
