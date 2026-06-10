# sigil-ex-t1 — agent final report
Condition: grammar + examples. Port 9821. sigil-authoring skill available as capability. Model: Opus 4.8. Independently re-built + re-run by the verifier (LIB unset so lld-link is used, not a stray GNU `link`): output.txt = 53 bytes byte-exact.

`consumer.sigil` / `behaviors/consumer-main.beh` (listen 9821, accept one POST, receive, now+format-time, assemble, write output.txt); `producer.sigil` / `behaviors/producer-main.beh` (read input.txt, build POST, connect 9821, send). Leaves: `build-output` (scan `\r\n\r\n`, write `[19-char ts][" PROCESSED BY CONSUMER"][0x0A][body]`), `build-request` (POST with Host + Content-Length), `bind-9821`/`connect-9821` sockaddr wiring over stdlib make-sockaddr.

Rework cycles (3): **E0104** int INPUTs are handles (LOAD before IADD/ILT); the leaf/composite split forced byte-assembly into leaves; and a verifier-side note — running the build via a Bash tool let a GNU coreutils `link` shadow `lld-link`, fixed by running `build.bat` (which prepends `C:\LLVM\bin`) from a Windows shell.

Token accounting: total 95,316; onboarding (grammar+registry+examples) ~22.7k; authoring ~72.6k.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes — 53-byte hex dump matches exactly (19-char ts, space, PROCESSED BY CONSUMER, 0x0A, Hello World; no trailing newline)
output_bytes: 53
rework_iterations: 3
stdlib_behaviors_used: socket, bind, make-sockaddr, listen, accept, receive, now, format-time, write-file-all, close-socket, println, read-file-all, int-to-string, connect, send
hand_rolled_instead_of_stdlib: none (body extraction and HTTP-request framing have no stdlib equivalent; build-output/build-request are necessarily custom leaves)
main_friction: An int INPUT is a handle, not a value, so every arithmetic/comparison use needs an explicit LOAD first (E0104), which is easy to forget when wiring leaf logic.
```
