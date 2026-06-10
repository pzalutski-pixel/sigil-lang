# Session notes — Sigil HTTP file relay (condition: sigil-with-examples)

## Task
Two Sigil components:
- **Consumer**: listen on port 9816, accept one HTTP POST, prepend a timestamp +
  `PROCESSED BY CONSUMER` line to the body, write to `output.txt`.
- **Producer**: read `input.txt` ("Hello World"), POST it to `localhost:9816`.

Acceptance: `output.txt` =
```
<YYYY-MM-DD HH:MM:SS> PROCESSED BY CONSUMER
Hello World
```

## Result: SUCCESS
`output.txt` produced byte-exact:
```
2026-06-07 03:47:26 PROCESSED BY CONSUMER
Hello World
```
(timestamp is real current UTC time from the runtime `now`+`format-time`; the
spec's `12:00:00` was illustrative.) Hex-verified: 19-char timestamp + space +
`PROCESSED BY CONSUMER` + 0x0A + `Hello World`, no trailing bytes / nulls.

## Approach / what I learned from the reference + examples
- Learned Sigil from `docs/SIGIL-LANGUAGE-REFERENCE.md`, the stdlib
  `catalog.md` / `contracts.registry`, and examples `http-server` (socket /
  bind / listen / accept / send), `file-copy` (open / read / write / close),
  and `make-addr-8080.beh` (sockaddr_in layout).
- Leaf vs composite rule: all wiring in `COMPOSITION`; all byte manipulation in
  `IMPLEMENTATION` leaves. Kept algorithmic work (body extraction, request
  assembly, sockaddr construction) in leaves.
- Output-consumption (`E0511`): every CALL output is consumed via `->`, an arg,
  a BRANCH, or `DISCARD`. Used `DISCARD` for all the status/aux outputs I ignore.
- Hashes resolved with `sigil-compiler --hash <file>` (never by hand). Stdlib
  dep hashes taken from `contracts.registry`. Leaves hashed first, then the
  composite mains (Merkle order). The three sockaddr leaves all hash to
  `00f67ec4` because the hash covers only the contract (`OUTPUT addr bytes 128`),
  which is identical across them — that is correct, not a collision bug.

## Files
- `consumer/main.sigil`, `consumer/behaviors/main.beh`
- `consumer/behaviors/make-addr-bind-9816.beh` — sockaddr_in, port 9816
  (0x2658 -> bytes 38,88), INADDR_ANY.
- `consumer/behaviors/assemble-output.beh` — finds CRLFCRLF, then writes
  `ts + " PROCESSED BY CONSUMER\n" + body` into the output buffer.
- `producer/main.sigil`, `producer/behaviors/main.beh`
- `producer/behaviors/make-addr-connect-9816.beh` — sockaddr_in for
  127.0.0.1:9816 (IP bytes 127,0,0,1 at offset 4).
- `producer/behaviors/build-post.beh` — concatenates HTTP request prefix
  (ending in CRLFCRLF) + body.
- `build.bat` — builds both exes with the prebuilt compiler + stdlib/runtime.

## Design details
- Consumer copies the body from `body_start` (after CRLFCRLF) to the end of the
  received bytes, so it does not depend on the producer's Content-Length.
- The middle literal `" PROCESSED BY CONSUMER\n"` is a string literal in the
  consumer composition, converted via `string-to-bytes` and passed into the
  leaf — avoids hand-coding 23 bytes in the leaf.
- `writes_output` (`E0604`): leaves write a default to every OUTPUT (incl. the
  bytes buffer, e.g. `STORE out 0 1 0`) before any loop/branch.

## Rework iterations (compile/fix cycles after first attempt): 2
1. `E0604` in `assemble-output`: the `out` bytes buffer wasn't written on the
   zero-iteration path. Fix: add a default `STORE out 0 1 0` at the top. (Recompile.)
2. `build.bat` used relative paths but ran with a different cwd; added
   `pushd %~dp0` / `popd`. (Recompile.)

(The producer's apparent "crash" with exit 0xC0000409 was NOT a code bug: I had
launched it without setting the working directory, so `open "input.txt"`
returned a negative fd and the runtime read/print ran off a bad buffer. Isolated
with a throwaway `diag` exe that printed "Hello World" correctly once run from
the trial dir. The real run sets `WorkingDirectory` and works.)

## How to run
```
cmd /c build.bat
# from this dir:
start build\consumer.exe        # background; listens on 9816
build\producer.exe              # reads input.txt, POSTs
type output.txt
```
