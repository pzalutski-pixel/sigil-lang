# sigil-noex-t3 — agent final report
Condition: grammar only. Port 9813. sigil-authoring skill available as capability. Model: Opus 4.8. Independently re-built + re-run by the verifier: output.txt = 53 bytes byte-exact.

3 leaf behaviors in `src/*.beh` compiled into `build/local.lib` via `--lib`: `build-post-request` (fixed POST header + body), `extract-body` (scan for `\r\n\r\n`, copy after it), `assemble-record` (`<ts>` + ` PROCESSED BY CONSUMER\n` + body, byte-exact STOREs). Consumer: socket→make-sockaddr(9813)→bind→listen→accept→receive→extract-body→now→format-time→assemble-record→write-file-all. Producer: read-file-all→build-post-request→socket→make-sockaddr(127.0.0.1:9813)→connect→send→close-socket.

Rework cycles (1): three behaviors used a raw INPUT handle where a loaded value was required in `IGE`/arithmetic (**E0104**), and one `LOAD handle offset` missing its size arg — caught and fixed before the first `build.bat`, which then produced byte-exact output first try.

Token accounting: total 88,258; onboarding ~17.2k; authoring ~71.1k.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 1
stdlib_behaviors_used: read-file-all, write-file-all, socket, make-sockaddr, bind, listen, accept, receive, connect, send, close-socket, now, format-time
hand_rolled_instead_of_stdlib: none (the three custom leaves — HTTP request framing, body extraction after CRLFCRLF, and record assembly — are application logic the stdlib does not provide)
main_friction: leaf primitives need explicit LOAD of INPUT handles into values before arithmetic/comparison (handles aren't values), which is easy to miss and only surfaces as a parse/semantic error at lib-build time.
```
