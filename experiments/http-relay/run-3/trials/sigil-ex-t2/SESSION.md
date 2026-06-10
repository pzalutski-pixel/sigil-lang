# Sigil HTTP file-relay — session notes (condition: sigil-with-examples)

## Task
Two Sigil programs:
- **Consumer**: listen on port 9815 for an HTTP POST, prepend a timestamp +
  `PROCESSED BY CONSUMER` line to the body, write to `output.txt`.
- **Producer**: read `input.txt` (`Hello World`), POST it to `localhost:9815`.

Acceptance `output.txt`:
```
<YYYY-MM-DD HH:MM:SS> PROCESSED BY CONSUMER
Hello World
```

## Result — SUCCESS
Both compiled and ran. `output.txt` produced byte-exactly:
```
2026-06-07 03:46:23 PROCESSED BY CONSUMER
Hello World
```
(timestamp is the current UTC time from stdlib `now` + `format-time`; the example's
`12:00:00` was a placeholder — the spec asks for the *current* date/time.)

Verified reproducibly: removed output.txt, started consumer (background), ran
producer, consumer wrote correct output and self-terminated after one connection.

## Design

Reference points: `examples/http-server` (socket/bind/listen/accept/send,
sockaddr_in layout, string-to-bytes, DISCARD discipline) and `examples/file-copy`
(open/read/write/close, flags=3 for write+create, flags=0 for read).

### Consumer (`consumer/`)
- `make-addr-9815.beh` (leaf) — sockaddr_in, AF_INET=2, port 9815 = 0x2657 →
  network-order bytes 0x26(38),0x57(87), INADDR_ANY=0. (Hash identical to the
  example's make-addr-8080: same contract → `00f67ec4`.)
- `bind-9815.beh` (composite) — CALL bind with addr, addr_len 16.
- `build-output.beh` (leaf) — assembles output buffer: copies the timestamp
  bytes, appends ` PROCESSED BY CONSUMER\n` (23 bytes, stored byte-by-byte),
  scans the received request for the first CRLFCRLF to find the body start,
  appends the body, then a trailing `\n`. Returns buffer + length.
- `main.beh` (composite) — socket → bind-9815 → listen → accept → receive →
  now → format-time → build-output → open(output.txt, flags 3) → write →
  close → close-socket. Handles one connection then exits.
- `main.sigil` — ENTRY calls main.

### Producer (`producer/`)
- `make-addr-local-9815.beh` (leaf) — sockaddr_in for 127.0.0.1:9815 (IP bytes
  127,0,0,1 at offset 4..7). (Contract identical to make-addr → `00f67ec4`.)
- `build-request.beh` (leaf) — hand-writes a minimal request header
  `POST / HTTP/1.1\r\nHost: localhost\r\n\r\n` (36 bytes) then appends the file
  body. Returns request bytes + length.
- `main.beh` (composite) — open(input.txt, flags 0) → read → close → build-request
  → socket → make-addr-local-9815 → connect → send → close-socket.
- `main.sigil` — ENTRY calls main.

The consumer's body extraction is robust: it locates `\r\n\r\n` and takes
everything after as the body (and falls back to whole-buffer if no terminator),
so it does not depend on Content-Length.

## Hashing workflow
Used `sigil-compiler --hash <file.beh>` for every behavior, bottom-up (leaves
first, then composites whose REQUIRES pin those hashes). Wrote each value into
the file's `HASH` line and the parent's `REQUIRES name@hash`. Stdlib hashes came
from `lib/release/.generated/behaviors.index`.

Resolved hashes:
- make-addr-9815 = 00f67ec4, bind-9815 = a3081b58, build-output = 5f56fa39,
  consumer main = e512e502
- make-addr-local-9815 = 00f67ec4, build-request = 2d0207a2, producer main = 295ceaae

## Build
`build.bat` compiles both with the prebuilt compiler:
```
sigil-compiler <dir>\<prog>\main.sigil --link <lib>\sigil-stdlib.lib --native-lib <lib>\sigil_runtime.lib -o build\<prog>.exe
```
The compiler auto-discovers each `.beh` from the executable's REQUIRES graph
(no manifest listing needed). Build emits harmless "dynamic offset bounds cannot
be verified" warnings for the byte-copy loops — expected for data-dependent
offsets; both binaries link cleanly.

## Friction / rework
- 1 real rework cycle: on first consumer build I had computed `bind-9815` and
  `build-output` hashes but forgot to write them into the leaf files' own `HASH`
  lines (left at the 00000000 placeholder), so the first build hit a
  dependency-hash-mismatch. Fixed by writing the computed hashes in; built clean.
- `build.bat` succeeded for consumer but the batch exited before producer in the
  combined run (errorlevel/endlocal quirk); compiling the producer directly with
  the documented command line worked fine and produced producer.exe. Both
  binaries are present and correct.
- The hash tooling (`--hash`) made the otherwise-painful contract-hash step
  straightforward — the main friction was just the bookkeeping of writing each
  resolved hash into two places (the file and its parents' REQUIRES).

## Files created
- input.txt
- build.bat
- consumer/main.sigil
- consumer/behaviors/make-addr-9815.beh
- consumer/behaviors/bind-9815.beh
- consumer/behaviors/build-output.beh
- consumer/behaviors/main.beh
- producer/main.sigil
- producer/behaviors/make-addr-local-9815.beh
- producer/behaviors/build-request.beh
- producer/behaviors/main.beh
- build/consumer.exe, build/producer.exe (build artifacts)
- output.txt (program output)
