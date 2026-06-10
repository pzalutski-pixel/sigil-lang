---
name: sigil-authoring
description: >-
  Guides writing and editing Sigil behaviors (.beh / .sigil / .pattern):
  resolving contract HASH and REQUIRES@hash version pins via the compiler (never
  by hand), and applying the leaf/composite, output-consumption (DISCARD),
  writes_output, and string rules, plus discovering existing stdlib behaviors
  instead of reimplementing them. Use when creating or editing Sigil source or
  resolving its hashes.
paths: "*.beh, *.sigil, *.pattern"
---

# Writing Sigil behaviors

`docs/SIGIL-LANGUAGE-REFERENCE.md` is the grammar authority. This skill is the
procedure for writing a behavior and getting its hashes and rules right.

## Resolving contract hashes — run `--hash`, never compute by hand

**Tool:** `sigil-compiler --hash <file.beh>` computes that behavior's contract
hash and prints it (just the 8 hex characters, nothing else). It does **not**
modify the file, and it works even before a `HASH` line exists, because it
computes from the CONTRACT. **When to use it:** for **every behavior you author
or change** — the CONTRACT is the behavior's identity, so a new or edited
CONTRACT means a new `HASH`. A behavior's `HASH` and each `REQUIRES name@hash`
are SHA-256 **version pins**; you run the tool and write the value down — never
compute SHA-256 yourself.

The hash is a Merkle pin — a behavior's hash includes its dependencies' hashes —
so resolve dependencies first. For each authored or changed `.beh`:

1. **Write the behavior** — CONTRACT (with `REQUIRES` listing dependencies by
   name) and body.
2. **Pin each dependency.** For every name in `REQUIRES`, get its hash and write
   `name@hash`:
   - source you have: `sigil-compiler --hash path/to/dep.beh`
   - a stdlib behavior: `grep <name> lib/release/.generated/behaviors.index`
     (each line is `name@hash`).
3. **Compute this behavior's own `HASH`** now that its `REQUIRES` are pinned:
   `sigil-compiler --hash path/to/this.beh` → prints e.g. `0281265c`. Write
   `HASH 0281265c`.
4. **Build to confirm** — no `HashMismatch` (your own HASH) and no dependency
   hash mismatch (your REQUIRES) means the pins are correct.

If you start writing a SHA-256 implementation, stop — that urge is the bug;
`--hash` is the tool. (The compiler binary is `compiler/target/release/sigil-compiler.exe`.)

### Worked example

```
BEHAVIOR greet
CONTRACT
  OUTPUT status int 8
  REQUIRES println            # name only — pin it next
  GUARANTEES writes_output
COMPOSITION
  msg = "Hi!"
  CALL println msg
  SET status 0
END
```
- Pin the dependency: `sigil-compiler --hash stdlib/units/console/println.beh` →
  `a273ebb3`; write `REQUIRES println@a273ebb3`.
- Compute greet's hash: `sigil-compiler --hash greet.beh` → prints its hash;
  write it as `HASH <value>`.
- Build → clean.

## Discover before you write

Don't reimplement what exists. `int-to-string`, `htons`/`ntohs`, `compare`,
`concat`, `substring`, `length`, etc. are in the stdlib. Scan the catalog
(`grep -i <topic> lib/release/.generated/catalog.md` from the Sigil repo root —
one line per behavior: name, inputs → outputs, purpose) and `REQUIRES` them.

## Rules the compiler enforces — write to them (checklist)

- [ ] **Leaf vs composite.** A `COMPOSITION`/`ENTRY` only wires — `CALL`, `SET`,
  `BRANCH`/`JUMP`/`LABEL`, `DISCARD`, concurrency, constant/field bindings. No
  `ALLOC`/`LOAD`/`STORE`/arithmetic (`E0509`). A leaf (`IMPLEMENTATION`) computes
  from primitives and may not `CALL` (`E0510`). Route on a field with
  `BRANCH result.flag ...`, don't `LOAD` it.
- [ ] **Consume every output (`E0511`).** Every output a CALL produces is passed
  to a CALL, branched on, written to an OUTPUT, or `DISCARD result`/
  `DISCARD result.field`. Bare `CALL f` is valid only if `f` has no outputs.
- [ ] **`writes_output` on every path (`E0604`).** Write each OUTPUT on every path
  — including the one where a loop runs zero times or a branch isn't taken; set a
  default before the loop/branch.
- [ ] **`string` vs `bytes`.** `string` is self-describing, declares no size
  (`INPUT text string`); `bytes N` is for fixed buffers. Don't fake a string with
  oversized `bytes` + a length argument.

## Verify by running

"Compiles" ≠ "correct." Build and **run** the executable, checking actual output
— concurrency and I/O paths especially.

## Installing this skill

This is per-agent setup, not per-project. The source ships at
`skills/sigil-authoring/` in the Sigil repo; install it into your AI agent's
skills directory. For Claude Code:

```bash
mkdir -p ~/.claude/skills/sigil-authoring
cp -r skills/sigil-authoring/* ~/.claude/skills/sigil-authoring/
```

The agent then auto-invokes it when you write Sigil, or call `/sigil-authoring`.
(Other agents/tools: install per their own skill mechanism.)
