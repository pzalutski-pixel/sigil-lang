# sigil-ex-t2 — agent final report
Condition: grammar + examples. Port 9822. sigil-authoring skill available as capability. Model: Opus 4.8.

consumer.sigil + consumer-main.beh (composite) + extract-body + assemble-output (leaves); producer.sigil + producer-main.beh + build-request (leaf); manifests + build.bat. Consumer wires socket/make-sockaddr/bind/listen/accept/receive, extract-body finds \r\n\r\n, now+format-time, assemble-output frames, write-file-all. Producer wires read-file-all, socket, make-sockaddr (127.0.0.1=16777343), connect, build-request prepends POST header, send/close-socket. Verified 53 bytes via od/wc; ran twice, only timestamp differs. Hashes resolved with `sigil-compiler --hash`.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 2
stdlib_behaviors_used: read-file-all, socket, make-sockaddr, connect, send, close-socket, bind, listen, accept, receive, now, format-time, write-file-all
hand_rolled_instead_of_stdlib: HTTP body extraction (find \r\n\r\n) and output framing — no stdlib HTTP parser/formatter exists; nothing stdlib-provided was hand-rolled
main_friction: the leaf-vs-value distinction — INPUT int handles must be LOADed before use in arithmetic, and every OUTPUT must be written on every control-flow path (including the not-found branch), which cost both rework cycles.
```
