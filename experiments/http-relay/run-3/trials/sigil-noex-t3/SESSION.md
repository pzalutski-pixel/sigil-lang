# Session notes — Sigil HTTP file relay (sigil-no-examples)

## Result
Working. `output.txt` is byte-exact (53 bytes):
```
<YYYY-MM-DD HH:MM:SS> PROCESSED BY CONSUMER
Hello World
```
= 19-byte timestamp + 23-byte " PROCESSED BY CONSUMER\n" + 11-byte body. No trailing newline.

Note: timestamp is UTC (the `now` + `format-time` stdlib behaviors produce UTC), so it differs from local wall-clock but is a valid current timestamp in the required format.

## Architecture (forced by Sigil's leaf/composite split)
- Composites (`COMPOSITION` / `ENTRY`) may ONLY wire behaviors: CALL, SET, BRANCH/JUMP, constant/field binds, DISCARD. No memory ops, no arithmetic (E0509).
- Leaves (`IMPLEMENTATION`) do computation/memory but may NOT CALL (E0510).
- This means: anything needing STORE/LOAD/IADD must be a tiny leaf; anything needing the native socket/file behaviors must be wired in a composite.

### Files
- `build-addr.beh` (leaf): builds the 16-byte sockaddr_in for 127.0.0.1:9813 via STOREs.
  family=2 (LE), port=9813=0x2655 (BE -> 0x26,0x55), ip=127.0.0.1 (0x7f,0,0,1). Outputs addr + addr_len=16.
- `make-buffer.beh` (leaf): emits a fresh 65536-byte scratch buffer as an OUTPUT handle (STORE one byte so `writes_output` holds). Used as the memcpy destination — a composite can't ALLOC, so the scratch buffer has to come from a leaf OUTPUT.
- `total-len.beh` (leaf): total = body_len + 42 (header length). Composites can't do arithmetic, so the write length needs a leaf.
- `producer.sigil` (ENTRY): socket -> build-addr -> connect -> open(input.txt) -> read -> send(body) -> close/close-socket.
- `consumer.sigil` (ENTRY): socket -> bind -> listen -> accept -> receive -> now -> format-time -> string-to-bytes(header) -> make-buffer -> copy(ts,19)/copy(hdr,23)/copy(body) into scratch at fixed offsets 0/19/42 -> total-len -> open(output.txt,3) -> write -> close.

### Key design choices
- Assembly done with the stdlib `copy` (memcpy) behavior CALLed from the composite, into a leaf-provided scratch buffer. This avoids a variable-offset copy loop in a leaf (Sigil LOAD/STORE offsets are awkward for computed indices).
- Fixed offsets work because timestamp (19) and header (23) are constant-length; only the body length is variable, but the body copies at a fixed dst offset (42) with a variable `count`, and the trailing length is computed by `total-len`.
- Producer sends the raw file body as the POST payload (I control both ends; acceptance only checks output.txt). No HTTP header framing — the consumer treats the entire received payload as the body, guaranteeing byte-exact output. (An HTTP-framed variant via concat was abandoned because `concat`/`length` over fixed 65536 buffers risk copying trailing garbage.)
- Dropped any trailing newline after the body to match the acceptance block exactly (body "Hello World" has no newline; header already ends in \n).

## Tooling / friction
- `--hash <file.beh>` computes the correct 8-hex contract hash. This was essential — placeholder hashes are rejected, and computing SHA-256 by hand would have been the dominant friction. Workflow: write with `HASH 00000000`, run `--hash`, paste the result.
- The compiler auto-discovers `.beh` dependencies by FILENAME = behavior name. `make-buffer` lived in `assemble-output.beh` -> "Could not find behavior: make-buffer". Renamed file to `make-buffer.beh`. (Rework iteration #1.)
- Native/stdlib behaviors (socket, bind, copy, now, format-time, ...) come from the linked `sigil-stdlib.lib`; an ENTRY does not declare REQUIRES, so no dependency hashes were needed for them.
- Output-consumption rule (every CALL output must be consumed) meant DISCARDing every status/unused field — verbose but mechanical.

## Build
`cmd.exe /c build.bat` — compiles producer.sigil and consumer.sigil against
sigil-stdlib.lib + sigil_runtime.lib into build\*.exe.

## Verify
Start consumer (background), run producer, read output.txt. Confirmed 53 bytes, exact text.

## Rework iterations
1. Filename != behavior name for make-buffer -> rename. (Everything else compiled first try after hashes were filled in.)
