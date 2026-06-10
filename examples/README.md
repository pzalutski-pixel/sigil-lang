# Examples

Working example programs written in Sigil.

## Purpose

- Validate that the language can express real programs
- Test expressiveness of the design
- Serve as test cases for the compiler
- Demonstrate language patterns

## Examples

| Example | Description | Status |
|---------|-------------|--------|
| hello-world | Simple console output | ✅ Builds & runs |
| file-copy | Copy a file from source to destination | ✅ Builds & runs |
| kv-store | In-memory key-value database | ✅ Builds & runs |
| http-server | Single-connection HTTP server | ✅ Builds & runs |
| producer-consumer | Channel-based pipeline | ✅ Builds & runs |
| spawn-test | `SPAWN`/`WAIT` pattern | ✅ Builds & runs |
| parallel-test | Multiple concurrent workers | ✅ Builds & runs |
| channel-test | Channel communication | ✅ Builds & runs |

> The four concurrency examples build and run: `SPAWN`/`WAIT`/`CHANNEL` execute on a green-thread runtime backed by real OS threads, and the compiler's concurrency-safety checks are enforced (a non-atomic access to a `SHARED` handle, or freeing a handle a live `SPAWN` borrows, is a compile error). What it does *not* do is auto-parallelize independent calls. See [`../docs/STATUS.md`](../docs/STATUS.md). These examples are, in effect, the concurrency-runtime integration tests.

There is one more directory here that is not an example: `test-lib/` is a minimal smoke test that an executable links against the built standard library (it calls stdlib `println` and nothing else). The root `build.bat` builds it alongside the examples to verify stdlib linking.

## Building

From the repository root:

```bash
# Build everything (compiler, runtime, stdlib, examples)
build.bat

# Build only the examples (requires compiler, runtime, and stdlib already built)
examples\build.bat

# Clean all build artifacts
clean.bat
```

Each example builds to its own `build/` subdirectory:
```
examples/hello-world/build/hello-world.exe
examples/kv-store/build/kv-store.exe
...
```

## Structure

Each example is a self-contained project: a `.sigil` entry point, its `behaviors/`, and a `build.bat` that produces `build/<name>.exe`.

## Running Examples

```bash
# Hello World
examples\hello-world\build\hello-world.exe

# KV Store demo
examples\kv-store\build\kv-store.exe

# File Copy (no arguments: it copies test_source.txt -> test_dest.txt,
# both relative to the example dir; test_source.txt ships with the example)
cd examples\file-copy
build\file-copy.exe

# HTTP Server (runs on port 8080)
examples\http-server\build\http-server.exe
```
