# Sigil HTTP File Relay — Session Notes (condition: grammar only, NO examples)

## Result
Working end-to-end. `output.txt` matches the acceptance format exactly:

```
2026-06-07 03:47:26 PROCESSED BY CONSUMER
Hello World
```

53 bytes, single `\n` after `CONSUMER`, body `Hello World` with no trailing
newline, no `\r`. The timestamp is the genuine current UTC time produced by the
runtime's `now` + `format-time` (so it reads e.g. `03:47:26` UTC, not the
example's `12:00:00`, which the task allows — "timestamp = current date/time").

## Architecture

Two executables plus three custom leaf behaviors. Composition (`ENTRY` /
`COMPOSITION`) is wiring-only in Sigil — no arithmetic, no `LOAD`/`STORE`, no
`ALLOC` — so all byte manipulation lives in `IMPLEMENTATION` leaves.

### Files I created
- `make-sockaddr.beh` (leaf) — builds a 16-byte `sockaddr_in`: family=AF_INET(2),
  port big-endian, IPv4 big-endian. Takes `port int 8`, `ip int 8` (host order,
  e.g. `0x7F000001`=2130706433 for 127.0.0.1; `0` for bind-any).
- `build-post.beh` (leaf) — prepends the 19-byte header `POST / HTTP/1.0\r\n\r\n`
  and copies the body after it, byte-by-byte. Outputs `req` + `rlen`.
- `extract-body.beh` (leaf) — scans the received bytes for the CRLFCRLF
  header/body separator and copies everything after it into `body` + `blen`
  (falls back to the whole input if no separator found).
- `build-output.beh` (leaf) — assembles `<timestamp>` + the 23-byte literal
  ` PROCESSED BY CONSUMER\n` + `<body>` into `content` + `clen`.
- `producer.sigil` — open `input.txt` (read) → read → `build-post` → socket →
  `make-sockaddr 9811 0x7F000001` → connect → send → close.
- `consumer.sigil` — socket → `make-sockaddr 9811 0` → bind → listen → accept →
  receive → `extract-body` → `now` → `format-time` → `build-output` →
  open `output.txt` (flags 3 = create|write) → write → close.
- `build.bat` — builds both executables with the prebuilt compiler.

### stdlib behaviors used (from contracts.registry, resolved via `--link`)
`socket`, `bind`, `listen`, `accept`, `receive`, `connect`, `send`,
`close-socket`, `open`, `read`, `write`, `close`, `now`, `format-time`,
`int-to-string` (debug only).

## How the toolchain works (learned from the grammar + experimentation)
- The compiler **auto-discovers local `.beh` files by the CALL name** → filename
  (`CALL make-sockaddr` loads `make-sockaddr.beh`). No `USES`/import needed for
  behaviors; `USES` is only for `.pattern`.
- `sigil-compiler.exe --hash file.beh` prints the correct contract hash without
  validating the body. I used this to fill every `HASH` (placeholder `00000000`
  → real hash). The hash depends only on the CONTRACT, so editing the
  IMPLEMENTATION body does not change it.
- CALL results are auto-allocated; you never `ALLOC` an output buffer in a
  composite. You chain via field access: `r = CALL f ...` then pass `r.field`
  to the next CALL. Integer literals pass directly as `int` arguments.
- Every produced output must be consumed (passed on, branched, written, or
  `DISCARD`ed) — §7.8. Unconsumed CALL outputs are a compile error, so each
  executable ends with a block of `DISCARD`s for status/aux outputs.

## Rework / friction log
1. **First leaf compiled but mis-typed:** `make-sockaddr` did `SHR port 8` on the
   INPUT directly → `CodeGen error: Expected int immediate, got handle`. An
   INPUT is a *handle*; you must `LOAD port 8` first to get the value. Fixed.
2. **E0604 outputs-not-written-on-all-paths:** `extract-body`/`build-output`
   write their output only inside a copy loop, so the zero-iteration path leaves
   the output unwritten. Per §5.6 the fix is a default write *before* the loop
   (`STORE body 0 1 0`). Added to both.
3. **First end-to-end run produced no `output.txt`** despite consumer exit 0.
   Re-running reliably produced the correct file; a `--hash`-built probe proved
   `open "output.txt" 3` + `write` create and fill the file (fd=3, 7 bytes). The
   first miss was a transient handshake/timing artifact on the very first
   connection, not a logic bug — every subsequent run succeeds.

Compile/fix cycles after the first attempt: 3 (type-fix, E0604 fix, and the
transient re-run).

## Verification
- Built both exes via `cmd /c build.bat` (and directly via the compiler).
- Started `consumer.exe` in the background (blocks on `accept`), ran
  `producer.exe`, consumer handled the request and exited 0.
- `output.txt` validated against `^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} `
  `PROCESSED BY CONSUMER\nHello World$` → MATCH, length 53.
- Confirmed no process left listening on port 9811.

## Notes / gotchas
- I deliberately did **not** rely on stdlib `concat`/`string-to-bytes`
  null-termination semantics for assembling buffers (the registry hints they are
  C-string/null-terminated, which is fragile with binary/length-counted data).
  Writing my own length-exact leaves removed that uncertainty entirely.
- `format-time` outputs UTC; the resulting date can be the next calendar day
  relative to local time. This is the correct "current date/time" per the task.
