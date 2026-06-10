# sigil-noex-t2 — agent final report
Condition: grammar only. Port 9812. sigil-authoring skill available as capability. Model: Opus 4.8.

Built producer.sigil + consumer.sigil with custom leaves in src/ compiled to a local lib via --lib: t2-acc-init/append/complete/finish (recv accumulator + body extractor), t2-assemble (output framing), t2-build-header. Consumer uses a recv loop accumulating chunks until the full request arrives. 3/3 runs produced 53-byte byte-exact output.

KEY FINDINGS:
- Buffer aliasing: distinct `bytes 65536` outputs from different CALLs can share the same scratch buffer, so writing one output corrupts another's data; inserting an unrelated call shifts the aliasing and silently changes results. Worked around by ordering writes so prefix bytes never overlap a still-needed source region.
- Abortive close discarded data on loopback; fixed with a request/response handshake (producer blocks on receive until consumer closes).
- No loop-carried composite values: composites can't hold/mutate a scalar across a JUMP, so the recv-loop running length lives in the accumulator buffer, managed by leaves.

```
RESULT SUMMARY
compiled_or_ran: yes
output_correct: yes
output_bytes: 53
rework_iterations: 8
stdlib_behaviors_used: read-file-all, write-file-all, socket, make-sockaddr, bind, listen, accept, connect, send, receive, close-socket, now, format-time
hand_rolled_instead_of_stdlib: HTTP header build + request-body assembly/copy loops in leaves; stdlib copy/substring/find exist but were avoided because cross-call buffer aliasing made their separate 64KB output buffers unsafe to chain
main_friction: undocumented buffer aliasing between distinct bytes-typed CALL outputs (same scratch buffer reused), which silently corrupted data and changed behavior when unrelated calls were added or reordered.
```
