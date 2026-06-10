# Standard Library

The behaviors that ship with Sigil — **92 behaviors across 13 units**, byte-tested,
with a discoverable catalog. The calibration is a **low-level systems library** —
think C's libc plus Go's core, not Python's batteries — and it doubles as the proof
that Sigil can build a real library in itself.

## Architecture rule (the dividing line)

A Sigil **leaf** can do any *computation* — `LOAD`/`STORE`, arithmetic, bitwise,
shifts, `ALLOC`/`FREE`/`SCOPE`, `ITOF`/`FTOI` — but has **no syscall primitive**.
So the line is drawn at the OS, not at "leaf vs composition":

- **Native (C runtime) iff it touches the OS** — file, console, sockets, clock,
  args/env/exit.
- **Everything else is pure Sigil** — strings, conversions, math, mem, encoding,
  hashing, collections, address construction. "Pure Sigil" means a `.beh` leaf
  *or* a `COMPOSITION` (e.g. the `io` unit and `bytes-to-string` are compositions
  that wire the native edge together); neither does a syscall directly.

## Units

| Unit | # | What's in it |
|------|---|--------------|
| `collections` | 12 | Int dynamic-array (`array-*`) and hash-map (`map-*`) over caller-owned memory — pure Sigil, no native heap. |
| `console` | 4 | `print` (NATIVE), `println`, `println-bytes`, `read_line` — terminal I/O. |
| `encoding` | 4 | `base64-encode`/`-decode`, `hex-encode`/`-decode`. |
| `file` | 5 | `open`, `read`, `write`, `close`, `exists` — NATIVE file I/O. |
| `hash` | 3 | `fnv1a`, `crc32`, `sha256` (sha256 byte-exact on the FIPS vectors). |
| `io` | 2 | `read-file-all`, `write-file-all` — whole-file convenience **compositions** over `file`. |
| `json` | 2 | `parse`, `format`. |
| `math` | 11 | `abs`, `min`, `max`, `clamp`, `pow`, `floor`, `ceil`, `round`, `sqrt`, `is-negative`, `is-non-positive`. |
| `mem` | 5 | Byte-buffer ops — `fill`, `copy`, `find-byte`, `mem-compare`, `zero`. |
| `network` | 14 | Sockets (`socket`/`bind`/`listen`/`accept`/`connect`/`send`/`receive`/`close-socket`, NATIVE) plus address construction + byte-order (`make-sockaddr`, `parse-addr`, `htonl`/`htons`/`ntohl`/`ntohs` — pure Sigil). |
| `os` | 4 | `arg-count`, `arg-get`, `env-get`, `exit` — NATIVE process/environment. |
| `string` | 22 | `length`, `concat`, `compare`, `find`, `substring`, `to-upper`/`to-lower`, `starts-with`/`ends-with`, `contains`, `repeat`, `pad-right`, `replace`, and the conversions `int-to-string`, `float-to-string`, `string-to-float`, `string-to-bytes`, `bytes-to-string`, `string-pack`. |
| `time` | 4 | `now`, `sleep`, `duration` (NATIVE clock) and `format-time` (pure). |

The full per-behavior signatures and one-line purposes are in
[`../docs/STDLIB-CATALOG.md`](../docs/STDLIB-CATALOG.md); the complete contracts
(sizes, hashes, guarantees) are in `lib/release/.generated/contracts.registry`.

## Discovery

- **`docs/STDLIB-CATALOG.md`** — the committed, agent-facing catalog: one line per
  behavior (name, inputs → outputs, purpose). Regenerate it after adding behaviors.
- **`lib/release/.generated/`** — written by the `--lib` build: `catalog.md`,
  `behaviors.index` (name@hash), and `contracts.registry` (full contracts). These
  are git-ignored; the catalog above is the committed copy.

## Building

From the repository root, or this directory:

```bash
# stdlib only
stdlib\build.bat
# or everything (compiler -> runtime -> stdlib -> examples)
build.bat
```

Both run `sigil-compiler --lib stdlib/units -o lib/release/sigil-stdlib.lib`,
which compiles every `.beh` into one archive and regenerates the `.generated/`
index files. Output: `lib/release/sigil-stdlib.lib`.

## Testing discipline

Every behavior is byte-tested, not "it compiled": write the `.beh`, `--hash` it,
build it into the lib, then run a real `.sigil` that `CALL`s it and assert the
exact output bytes — plus the regression gate (suite + all 8 examples byte-exact)
after each addition.

## Known gaps

- **No HTTP framing** — request/response assembly is application logic with no
  library equivalent, by design.
- **No binary-safe concat** — `string/concat` is null-terminated (C-string style),
  so it is unsafe for length-exact binary data; a length-carrying `concat-bytes`
  is the obvious addition (callers currently hand-roll length-exact copies).

## Using in applications

Reference a stdlib behavior by `name@hash` in `REQUIRES` (hashes are
content-addressed; get the current one from `--hash` or `behaviors.index`):

```
BEHAVIOR my-app

CONTRACT
  REQUIRES println@a273ebb3 open@7d62cd96
  ...
```

The compiler links the archive when passed
`--link lib/release/sigil-stdlib.lib` (with `--native-lib lib/release/sigil_runtime.lib`
for the C runtime behind the NATIVE behaviors).
