# Sigil HTTP File Relay — Session Notes (condition: sigil-no-examples)

Built an HTTP file relay (producer + consumer) in Sigil, learning the language
solely from `docs/SIGIL-LANGUAGE-REFERENCE.md` (grammar) and the stdlib
`contracts.registry`. No example sources were read.

## Result

End-to-end works. `output.txt` after a run:

```
2026-06-07 03:46:25 PROCESSED BY CONSUMER
Hello World
```

53 bytes = 19 (timestamp `YYYY-MM-DD HH:MM:SS`) + 23 (` PROCESSED BY CONSUMER\n`) +
11 (`Hello World`). The fixed portion matches the acceptance spec byte-for-byte.
The timestamp is the live current time (UTC, from `now` + `format-time`), which is
correct per the spec ("timestamp = current date/time"); the `2026-06-06 12:00:00`
in the prompt is an illustrative value, not a literal to hardcode.

## Architecture

Two executables (`.sigil` ENTRY = composition / wiring-only):

- **producer.sigil**: `open input.txt` (read) -> `read` -> `build-post` (wrap body in
  a minimal HTTP POST) -> `make-sockaddr 9812` -> `socket` -> `connect` -> `send` ->
  `close-socket`.
- **consumer.sigil**: `make-sockaddr 9812` -> `socket` -> `bind` -> `listen` ->
  `accept` -> `receive` -> locate `\r\n\r\n` via `find` -> `substring` to extract the
  body -> `now`/`format-time` for the timestamp -> `build-output` (assemble
  timestamp + " PROCESSED BY CONSUMER\n" + body) -> `open output.txt` (write|create,
  flags=3) -> `write` -> `close` -> close sockets. Handles one connection then exits.

Custom behaviors I had to write (the leaf/composite split forced these):

- **make-sockaddr.beh** (leaf): builds the 16-byte sockaddr_in for 127.0.0.1:port.
  Computes network (big-endian) port order with AND/SHR/SHL/OR.
- **build-post.beh** (composite): `string-to-bytes "POST / HTTP/1.1\r\n\r\n"` then
  two `copy` calls to lay header + body into the request buffer; `iadd` for total len.
- **build-output.beh** (composite): `copy` timestamp(19) + " PROCESSED BY CONSUMER\n"
  + body into the result buffer; `iadd` for offsets/total length.
- **iadd.beh / isub.beh** (pure leaves): arithmetic helpers, because a COMPOSITION
  may not compute — any derived length/offset must come from a CALL.

## Build

`build.bat` (run `cmd /c build.bat`) compiles both with the prebuilt compiler:
```
sigil-compiler.exe <entry>.sigil --link sigil-stdlib.lib --native-lib sigil_runtime.lib -o build\<name>.exe
```
Local `.beh` dependencies are auto-discovered by name from the same directory; only
the stdlib `.lib` needs `--link`. Hashes were resolved with the compiler's
`--hash <file>` flag (never hand-computed).

## Frictions / things learned the hard way

1. **LOAD/STORE offsets must be integer literals, not variables.** My first
   `build-post` was a leaf doing a runtime-offset byte-copy loop — rejected at parse
   ("Expected integer, got Identifier"). Fix: do variable-length copies via the
   stdlib `copy` (memcpy) behavior, which means the assembler has to be a
   COMPOSITION, not a leaf.

2. **Compositions can't compute** (E0509) — no ALLOC/LOAD/STORE/arithmetic. So
   every offset/length the wiring needs (header_len + body_len, index+4,
   received - bodyoff) had to be produced by a tiny pure leaf (`iadd`/`isub`).

3. **`writes_output` vs. in-place buffer writes (E0604).** When a buffer is both my
   declared OUTPUT and the `dst` I pass into `copy`, the compiler doesn't count the
   in-place mutation as "writing the OUTPUT" — it only credits SET or `->`. Dropping
   the `writes_output` guarantee from the assembler composites resolved it; the data
   is still written correctly through the handle at runtime (verified by the output).

4. **Handle vs. value in leaves.** In a leaf, an INPUT name is a *handle*; passing it
   straight to AND/STORE gave "Expected int immediate, got handle". Must `LOAD` it
   first.

5. **Output consumption (E0511 / §7.8).** Every CALL output must be consumed —
   branched, passed on, written, or `DISCARD`ed. Lots of `DISCARD status` /
   `DISCARD bytes_written` etc. for effectful calls whose results I don't use.

6. **HTTP framing kept minimal.** Producer sends `POST / HTTP/1.1\r\n\r\n` + body;
   consumer strips everything up to and including the first `\r\n\r\n`. A single
   `receive` gets the whole small request. `find`/`substring` (declared with larger
   `bytes` capacities than the literal needle) accepted the buffers without size
   complaints.

## Files created

make-sockaddr.beh, build-post.beh, build-output.beh, iadd.beh, isub.beh,
producer.sigil, consumer.sigil, build.bat, SESSION.md (+ build/producer.exe,
build/consumer.exe, and runtime output.txt).
