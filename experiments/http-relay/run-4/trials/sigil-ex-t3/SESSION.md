# sigil-ex-t3 — agent final report
Condition: grammar + examples. Port 9823. sigil-authoring skill available as capability. Model: Opus 4.8.

Consumer (consumer/main.sigil + 3 behaviors): socket→bind 9823→listen→accept→receive→extract-body (scan \r\n\r\n)→now/format-time→frame-output→write-file-all. Producer (producer/main.sigil + 2 behaviors): read-file-all→string-to-bytes on a fixed POST header→concat-bytes(header+body)→socket→connect 127.0.0.1:9823→send. Hashes resolved via `sigil-compiler --hash` (extract-body 741a45ad, frame-output 8f560f79, concat-bytes fa25bca7, consumer-main 00b921fa, producer-main 9e4207c0). Hex-verified 53 bytes, reproducible.

Rework (2): (1) leaves leaked ALLOC scratch (E0303) + didn't write body/result on zero-length path (E0604) — fixed with FREEs on the converging path + default output writes; (2) codegen error from using int INPUT handles (req_len, tlen, blen, alen) directly in arithmetic instead of LOAD-ing first.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 2
stdlib_behaviors_used: socket, make-sockaddr, bind, listen, accept, receive, now, format-time, write-file-all, close-socket, read-file-all, string-to-bytes, connect, send
hand_rolled_instead_of_stdlib: concat-bytes (a length-carrying byte concatenation — stdlib `concat` exists but is null-terminated, unsafe for binary HTTP request data); extract-body and frame-output are app-specific (no stdlib equivalent)
main_friction: int INPUT parameters are handles, not values — using them directly in primitives compiles past semantic checks but fails at codegen ("expected int immediate, got handle"), so every scalar input needs an explicit LOAD first.
```
