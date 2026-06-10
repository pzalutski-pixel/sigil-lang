# Sigil Compiler

The Sigil compiler lowers behavior files to native executables through LLVM. It is experimental — see [`../docs/STATUS.md`](../docs/STATUS.md) for exactly what is and isn't enforced.

## What it does

- Compiles a `.sigil` entry point and the `.beh` behaviors it calls into a native x86-64 executable.
- Runs a sequence of compile-time checks (below), lowers through LLVM with optimization, and links via the platform toolchain.
- Caches compiled behaviors so unchanged ones aren't recompiled (incremental builds).

## How it's organized

The pipeline lives in `src/`: tokenizing (`lexer.rs`) → parsing into an AST (`parser.rs`, `ast.rs`) → validation (`semantic/`) → LLVM code generation (`codegen/`), with `main.rs` driving file discovery, the incremental cache, and linking. Supporting modules: `cfg.rs` (control-flow graph for path analysis), `hash.rs` (contract hashing), `cache.rs` (incremental cache), `linker.rs` (toolchain discovery + linking), `graph/` (the whole-program behavior graph and its cross-behavior checks), and a diagnostics subsystem (`diagnostic.rs`, `render.rs`, `source.rs`) that renders semantic errors with error code, source line, and hint.

## Compile-time checks

The compiler runs these passes, in order:

1. **Parsing** — syntax, unknown primitives, argument counts.
2. **Name resolution** — every reference resolves; no duplicates; labels exist.
3. **Type checking** — interpretation (`int`/`float`/`bytes`/`string`) and size consistency (a `string` is self-describing and declares no size).
4. **Memory safety & ownership (CFG-aware, all paths)** — bounds checking; use-after-free, double-free, uninitialized-read, and leak detection; borrow/lifetime rules (no freeing a borrowed INPUT or a handle a live `SPAWN` still borrows); non-atomic access to `SHARED` memory.
5. **Data flow** — `writes_output` (every output written on every path).
6. **Guarantee checks** — `pure`, `no_alloc` (see the caveat below).
7. **Contract-hash verification** — declared hashes match the computed contract hash.

Most of these are enforced and unit-tested today: the memory-safety and ownership checks (pass 4), type/size matching, `writes_output`, atomic-on-`SHARED`, contract hashing, and the reference/arity checks. A whole-graph pass (`main.rs` Phase 5.5) also enforces, across behaviors, **transitive purity**, **circular-dependency** rejection, **dependency hash-pin** consistency (`REQUIRES name@hash`), and **outputs-consumed** completeness — every CALL output in a composition must be routed, branched on, written to an OUTPUT, or explicitly `DISCARD`ed, else `E0511`. The genuine gaps are specific and named: `pure` doesn't track *undeclared* pattern-`MEMORY` access; `CALL`-argument type inference is conservative (a `LOAD` result is inferred `int`, a `CALL` result untyped); and the *full* graph-level **input-sourcing** check (`graph/gaps.rs::find_gaps`) is built and tested but not wired on V1 — inputs are checked at the CALL-argument level instead. (`all_paths_terminate` is enforced as a decidable reachability check, not a halting proof; `no_alloc` is enforced including scoped `ALLOC`.) The concurrency *runtime* works and its core safety checks (atomic-on-`SHARED`, free-while-borrowed) are enforced; what's missing there is auto-parallelization. Per-feature breakdown in [`../docs/STATUS.md`](../docs/STATUS.md).

## Building

Windows only. Requires Rust (MSVC toolchain), VS Build Tools, and the **LLVM 18.1.x development libraries**.

LLVM 18 specifically (not newer) because the Rust bindings (`inkwell` / `llvm-sys`) target it and it matches `rustc`'s own LLVM line. The full setup — the LLVM dev package, `LLVM_SYS_180_PREFIX`, and the `libxml2s.lib` stub the official LLVM package needs — is in the [Building section of the project README](../README.md#building-and-running).

```bash
compiler\build.bat             # debug build
compiler\build.bat --release   # release build
compiler\build.bat --test      # build + run the test suite
```

## Status

The compiler builds, the test suite passes, and all bundled examples compile and run. Output is native x86-64 (validated on Windows). Node-level checking is real and thorough, the concurrency runtime works, and whole-graph (inter-behavior) validation is wired — transitive purity, circular-dependency rejection, dependency hash-pins, and outputs-consumed (`E0511`). What remains is the full graph-level input-sourcing check (`find_gaps`, built but unwired on V1). See [`../docs/STATUS.md`](../docs/STATUS.md).
