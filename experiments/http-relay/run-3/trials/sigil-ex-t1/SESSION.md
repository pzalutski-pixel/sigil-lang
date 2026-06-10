# Sigil HTTP File Relay — Session Notes (condition: sigil-with-examples)

## Goal
Two Sigil programs:
- **Consumer**: listen on TCP port 9814, accept one HTTP POST, prepend
  `<timestamp> PROCESSED BY CONSUMER\n` to the POST body, write `output.txt`.
- **Producer**: read `input.txt` ("Hello World"), POST it to `localhost:9814`.

Acceptance `output.txt`:
```
<YYYY-MM-DD HH:MM:SS> PROCESSED BY CONSUMER
Hello World
```

## Outcome
Both compiled on the **first attempt** (0 rework iterations). Ran end-to-end and
produced byte-correct output:
```
2026-06-07 03:45:20 PROCESSED BY CONSUMER
Hello World
```
(trailing newline present). Timestamp is current UTC from `now`/`format-time`;
the `2026-06-06 12:00:00` in the spec is an illustrative placeholder.

## How I learned the language
- Read `docs/SIGIL-LANGUAGE-REFERENCE.md` (grammar, contracts, HASH algorithm,
  leaf/composite split, output-consumption rule).
- Read `lib/release/.generated/contracts.registry` — full stdlib. Everything
  needed was already there: socket/bind/listen/accept/receive/send/connect,
  open/read/write/close, now, format-time, println.
- Read examples: `examples/http-server/*` (socket server + sockaddr_in +
  string-to-bytes + send + DISCARD pattern) and `examples/file-copy/*`
  (open/read/write/close + BRANCH error handling + dynamic-offset byte copy
  loops). These were the templates I mirrored.

## Architecture
Each program is an `EXECUTABLE` (`main.sigil`) whose `ENTRY` calls a `main`
composite. Behaviors live in `behaviors/` beside the `.sigil`; the compiler
auto-discovers `behaviors/<name>.beh` for each `REQUIRES name@hash`. Library
behaviors resolve from the linked stdlib.

### Consumer
- `consumer/main.sigil` — entry.
- `behaviors/main.beh` (composite) — socket(2,1) → bind → listen → println →
  accept → receive → now → format-time → build-output → open("output.txt",3) →
  write → close → close-socket. All effect-only outputs DISCARDed.
- `behaviors/make-addr-9814.beh` (leaf) — sockaddr_in for 0.0.0.0:9814.
  Port 9814 = 0x2656 big-endian → byte2=38, byte3=86.
- `behaviors/build-output.beh` (leaf) — scans the received request for the
  CRLFCRLF header terminator, takes everything after it as the body, then emits
  `<ts> PROCESSED BY CONSUMER\n<body>\n` into the output buffer.

### Producer
- `producer/main.sigil` — entry.
- `behaviors/main.beh` (composite) — open("input.txt",0) → read → close →
  build-request → socket → make-addr-localhost-9814 → connect → println →
  send → close-socket.
- `behaviors/make-addr-localhost-9814.beh` (leaf) — sockaddr_in for
  127.0.0.1:9814 (IP bytes 4-7 = 127,0,0,1; same port encoding).
- `behaviors/build-request.beh` (leaf) — wraps the file bytes in
  `POST / HTTP/1.1\r\nHost: localhost\r\n\r\n<body>`. The consumer keys only on
  the CRLFCRLF separator, so no Content-Length is needed.

## HASH workflow (the main friction)
HASHes can't be guessed; the compiler rejects wrong ones. Process:
1. Wrote contracts with placeholder `HASH 00000000`.
2. Computed leaf hashes with `sigil-compiler.exe <file>.beh --hash`:
   - make-addr-9814 = `00f67ec4`
   - build-output   = `38cf68f1`
   - make-addr-localhost-9814 = `00f67ec4` (identical contract shape)
   - build-request  = `cc30d563`
3. Filled those into the leaf `HASH` lines AND into each composite's `REQUIRES`.
4. Re-ran `--hash` on the composites (their hash depends on the REQUIRES hashes):
   - consumer main = `ffbec3e7`
   - producer main = `72855a0e`
5. Filled those in. A composite hash must be computed *after* its dependencies'
   hashes are final, so ordering matters: leaves first, then composites.

## Build
`build.bat` invokes the prebuilt compiler on each `.sigil` with
`--link sigil-stdlib.lib --native-lib sigil_runtime.lib`. Run with
`cmd.exe /c <abs path>\build.bat`. Both linked cleanly. Only warnings: "STORE/LOAD
with dynamic offset … bounds cannot be verified at compile time" — expected for
position-tracked byte-copy loops; not errors.

## Verify
1. `build\consumer.exe` started in background (WorkingDirectory = trial dir).
2. `build\producer.exe` run to completion (exit 0).
3. `output.txt` inspected via Format-Hex — matched acceptance byte-for-byte
   (timestamp + " PROCESSED BY CONSUMER\n" + "Hello World" + "\n").
4. Consumer exits on its own after one connection — no cleanup needed.

## Notes / gotchas
- Consumer/producer run relative to CWD, so they must be launched with
  WorkingDirectory set to the trial dir for input.txt/output.txt to resolve.
- `open` flags: 0 = read, 3 = write+create (write=1 | create=2), per stdlib doc.
- Composites are wiring-only: no LOAD/STORE/arithmetic. All byte assembly lives
  in leaves (build-output, build-request, make-addr-*). The leaf/composite split
  was followed without hitting E0509/E0510.
- Output consumption: every CALL result is consumed (passed on, branched, SET to
  an OUTPUT, or DISCARDed). I DISCARDed all the effect-only status outputs.

## Files created
- consumer/main.sigil
- consumer/behaviors/main.beh
- consumer/behaviors/make-addr-9814.beh
- consumer/behaviors/build-output.beh
- producer/main.sigil
- producer/behaviors/main.beh
- producer/behaviors/make-addr-localhost-9814.beh
- producer/behaviors/build-request.beh
- build.bat
- SESSION.md
