# sigil-ex-t1 — agent final report
Condition: grammar + examples. Port 9821. sigil-authoring skill available as capability. Model: Opus 4.8.

Split into consumer/ and producer/ subtrees (each its own entry, to avoid a `main` name collision). consumer/consumer.sigil + behaviors/{consumer-main, assemble-output}; producer/producer.sigil + behaviors/{producer-main, build-post}. Producer sends a minimal HTTP/1.1 POST (Content-Length via int-to-string) to 127.0.0.1:9821; consumer reads body after \r\n\r\n, prepends format-time timestamp + framing, writes via write-file-all. All HASH lines resolved with `sigil-compiler --hash` (leaves first, then composites) — no mismatches. Build + run succeeded first time; hex dump confirms byte-exact 53 bytes.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 0
stdlib_behaviors_used: socket, make-sockaddr, bind, listen, accept, receive, now, format-time, write-file-all, close-socket, read-file-all, int-to-string, connect, send
hand_rolled_instead_of_stdlib: none (the two custom leaves assemble-output and build-post do HTTP body framing / request assembly, which the stdlib does not provide)
main_friction: two separate processes needed two separate executables (the .sigil + behaviors/ auto-discovery is per-directory and would have collided on the `main` name), so the project was split into consumer/ and producer/ subtrees.
```
