# sigil-noex-t1 — agent final report
Condition: grammar only (grammar + contracts.registry). Port 9811. sigil-authoring skill available as a real Claude Code capability (not mentioned in prompt). Model: Opus 4.8, autonomous subagent. Independently re-built + re-run by the verifier: output.txt = 53 bytes byte-exact.

Built `consumer.sigil` + `producer.sigil` + two leaves (`build-consumer-output`, `build-http-post`) compiled into `build\mylib.lib` via `--lib`, then linked with the stdlib + runtime. Consumer: socket → make-sockaddr(9811) → bind → listen → accept → receive → now → format-time → build-consumer-output → write-file-all. Producer: read-file-all → int-to-string(len) → build-http-post → socket → make-sockaddr(9811,127.0.0.1) → connect → send.

Rework cycles (2): (1) leaf BEHAVIORs in the `.sigil` file → "Expected Executable, got Behavior"; leaves must be a separate `--lib`. (2) **E0104**: int INPUTs are handles, not values — `ISUB req_len ...` rejected; LOAD each int INPUT before arithmetic. (The `cmd /c build.bat` bare-name fix was shell quoting, not a Sigil fix.)

Token accounting: total `subagent_tokens` 96,456; onboarding (grammar+registry) ~17.2k; authoring ~79.3k.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 2
stdlib_behaviors_used: socket, make-sockaddr, bind, listen, accept, receive, now, format-time, write-file-all, close-socket, read-file-all, int-to-string, connect, send
hand_rolled_instead_of_stdlib: byte assembly of the output framing and the HTTP request, and the CRLFCRLF body scan, were hand-written leaves; copy/find-byte exist but a composite cannot own a destination buffer to call them into, so the assembly had to live in leaf primitives
main_friction: the executable file can't contain helper BEHAVIORs and composites can't touch memory, so all byte assembly had to be split into separate leaf .beh files compiled into their own --lib library and linked, with every int INPUT explicitly LOADed from its handle before use
```
