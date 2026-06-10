# Safety Rules

> **Status — most of this is enforced today, with specific exceptions.** The bulk of the rules below are implemented and unit-tested: the CFG-aware memory-safety checks (bounds, double-free, use-after-free, uninitialized read, leaks), ownership (you cannot free a borrowed INPUT handle, or one a live `SPAWN` still borrows), the type/size rules, `writes_output`, contract-hash verification (including dependency `REQUIRES@hash` pins), the reference/name/arity checks, and non-atomic-access-to-`SHARED`. A whole-graph pass also enforces cross-behavior rules: **transitive purity** (a `pure` behavior cannot depend, even transitively, on a non-pure one), **no circular dependencies**, **dependency-hash-pin consistency**, and the **outputs-consumed** half of port completeness (every CALL output routed, branched on, written to an OUTPUT, or explicitly `DISCARD`ed, error E0511). `all_paths_terminate` is implemented as a decidable reachability check (every block reaching an exit — not the undecidable halting property), and the `no_alloc` SCOPE bug is fixed — both were formerly listed here as broken. The things that are still **not** enforced are specific and named: `pure` does not yet track *undeclared* pattern-`MEMORY` access (Part 5; a *declared* `MEMORY` is caught); and *full* port-level input-sourcing across the wired graph (`graph/gaps.rs::find_gaps`) is built and tested but not wired on V1 — inputs are checked at the CALL-argument level instead, and the full graph check is the V2 graph-native increment. Where a rule below is one of those exceptions, it is flagged inline. [STATUS.md](../STATUS.md) is the authoritative per-feature list. "If it compiles, it is safe" is the design *goal*, not a property the current implementation delivers.

## Overview

This document defines the compile-time safety rules the language is designed around. The intent is for the compiler to be the primary guardrail. These rules target both traditional programming errors and AI-specific hallucination errors. Where a rule is not yet enforced, [STATUS.md](../STATUS.md) says so.

---

## Part 1: The Core Principle

**Compile-time verification is intended to be the primary guardrail.**

- AI will make mistakes
- Runtime might not catch everything
- The compiler should reject unsafe code
- *Goal:* if it compiles, it runs safely — an aspiration the current implementation only partially meets (see [STATUS.md](../STATUS.md))

---

## Part 2: Memory Safety

### Rule: Bounds Checking

Every memory access must be within allocation bounds.

```
LOAD(handle, size, offset):
  VERIFY: offset + size ≤ handle.allocated_size

STORE(handle, value, size, offset):
  VERIFY: offset + size ≤ handle.allocated_size
```

**Violation = Compile Error**

```
h = ALLOC 64 bytes
LOAD h 8 60        # ERROR: 60 + 8 = 68 > 64
```

### Rule: Lifetime Safety

Handles only valid within owning scope/pattern.

```
SCOPE
  h = ALLOC 64 bytes
  # h valid here
END_SCOPE
# h invalid here - compile error if used
```

**Verification:**
- Track which scope/pattern owns each handle
- Reject use outside owning scope
- Reject storing borrowed handle in pattern MEMORY (would outlive borrow)

### Rule: Initialization Safety

No reading uninitialized memory.

```
h = ALLOC 64 bytes
v = LOAD h 8           # ERROR: h not written yet
```

**Verification:**
- Data flow analysis
- Every LOAD must be preceded by STORE to that location on all paths

### Rule: Ownership Safety

Only owner can FREE.

```
BEHAVIOR foo(borrowed_handle):
  FREE borrowed_handle   # ERROR: not owner
```

**Verification:**
- Track ownership (ALLOC creates owner)
- Only owner scope/pattern can FREE
- Borrowed handles cannot be freed

### Rule: No Memory Leaks

Every allocation must be freed.

```
SCOPE
  h = ALLOC 64 bytes
  # ... use h ...
END_SCOPE              # h automatically freed

# No explicit FREE needed - scope end handles it
```

**Verification:**
- Scoped allocations freed at scope end
- Pattern MEMORY freed at pattern termination
- Every ALLOC has corresponding FREE

### Rule: No Aliasing (Unless Explicit)

Two handles cannot refer to overlapping memory unless marked SHARED.

```
h1 = ALLOC 64 bytes
h2 = ALLOC 64 bytes
# h1 and h2 guaranteed non-overlapping
```

**Verification:**
- Allocator guarantees non-overlapping
- Only SHARED handles can be accessed by multiple concurrent units

---

## Part 3: Type Safety

### Rule: Interpretation Matching

Operations must match handle interpretation.

```
h_int = ALLOC 8 int
h_float = ALLOC 8 float
h_bytes = ALLOC 64 bytes

IADD (LOAD h_int 8) 5 8        # OK - int op on int
FADD (LOAD h_float 8) 1.5 8    # OK - float op on float

IADD (LOAD h_float 8) 5 8      # ERROR - int op on float
FADD (LOAD h_int 8) 1.5 8      # ERROR - float op on int
IADD (LOAD h_bytes 8) 5 8      # ERROR - arithmetic on bytes
```

**Verification:**
- Track interpretation of each handle
- Verify operation family matches interpretation

### Rule: No Implicit Conversion

Type conversion must be explicit.

```
h_int = ALLOC 8 int
h_float = ALLOC 8 float

# Must use explicit conversion
int_val = LOAD h_int 8
float_val = ITOF int_val 8 8
STORE h_float float_val 8
```

**Verification:**
- No automatic int-to-float or float-to-int
- ITOF/FTOI required for conversion

### Rule: Size Consistency

Operation size must match data.

```
h = ALLOC 4 int          # 4 bytes

IADD (LOAD h 4) 5 4      # OK - 4 byte operation
IADD (LOAD h 8) 5 8      # ERROR - loading 8 bytes from 4 byte handle
```

---

## Part 4: Contract Safety

### Rule: Input Matching

Caller must provide handles matching input declarations.

```
BEHAVIOR process:
  CONTRACT:
    INPUT data bytes 1024
    INPUT count int 4

# Caller
CALL process buffer n     # buffer must be bytes/1024, n must be int/4
```

**Verification:**
- Each input handle matches declared interpretation and size
- All required inputs provided

### Rule: Output Matching

Behavior must write to outputs matching declarations.

```
BEHAVIOR compute:
  CONTRACT:
    OUTPUT result int 8

  IMPLEMENTATION:
    STORE result value 8    # Must write to declared output
```

**Verification:**
- All declared outputs written
- Written values match interpretation and size

### Rule: Requires Satisfied

All required behaviors must be available.

```
BEHAVIOR handler:
  CONTRACT:
    REQUIRES parse-http@a1b2c3 format-json@d4e5f6

  COMPOSITION:
    CALL parse-http ...     # Must exist with hash a1b2c3
    CALL format-json ...    # Must exist with hash d4e5f6
```

**Verification:**
- Required behavior exists
- Hash matches current contract

### Rule: Capabilities Declared

> **REJECTED (January 2025):** CAPABILITIES and SYSCALL were removed from the implemented grammar. System interactions use NATIVE behaviors backed by trusted C runtime instead. This rule no longer applies.

Syscall usage must be declared.

```
BEHAVIOR read-file:
  CONTRACT:
    CAPABILITIES syscall:open syscall:read syscall:close

  IMPLEMENTATION:
    SYSCALL 2 ...    # open - must have capability
    SYSCALL 0 ...    # read - must have capability
    SYSCALL 3 ...    # close - must have capability
```

**Verification:**
- Every SYSCALL covered by declared capability
- Composite behaviors: transitively check called behaviors

### Rule: Hash Integrity

Declared hash must match computed hash.

```
BEHAVIOR foo:
  CONTRACT:
    INPUT x int 4
    OUTPUT y int 4
  HASH a1b2c3d4      # Must match SHA256(CONTRACT)
```

**Verification:**
- Compute hash from contract
- Compare with declared hash
- Mismatch = compilation error

---

## Part 5: Guarantee Verification

> **Implementation note.** `writes_output` is reliably enforced, and `pure` is enforced both within a behavior **and transitively** — a `pure` behavior that (transitively) depends on a non-pure one is a compile error (a whole-graph check; see [STATUS.md](../STATUS.md)). The remaining `pure` gap is *undeclared* pattern-`MEMORY` access — a *declared* `MEMORY` is caught. `no_alloc` is enforced, and an ALLOC inside a balanced `SCOPE` (freed at `END_SCOPE`) correctly satisfies it — the former SCOPE-flagging bug is fixed.

### Guarantee: `pure`

No side effects, deterministic.

**Compiler verifies:**
- No calls to NATIVE or otherwise-effectful behaviors
- No writes except to OUTPUT handles
- No reads/writes to pattern MEMORY
- No CHANNEL operations

```
BEHAVIOR add:
  CONTRACT:
    GUARANTEES pure

  IMPLEMENTATION:
    # Only reads inputs, writes outputs
    a = LOAD x 4
    b = LOAD y 4
    c = IADD a b 4
    STORE result c 4     # OK - result is OUTPUT
```

### Guarantee: `no_alloc`

No memory allocation escaping scope.

**Compiler verifies:**
- No ALLOC, or
- All ALLOC within SCOPE and freed at SCOPE end

```
BEHAVIOR transform:
  CONTRACT:
    GUARANTEES no_alloc

  IMPLEMENTATION:
    # No ALLOC allowed
    a = LOAD input 8
    b = IMUL a 2 8
    STORE output b 8
```

### Guarantee: `no_syscall`

> **REJECTED (January 2025):** The `no_syscall` guarantee was removed from the implemented grammar. With SYSCALL primitive rejected, this guarantee became meaningless - all system interactions go through NATIVE behaviors at stdlib level.

No system calls.

**Compiler verifies:**
- No SYSCALL primitive
- No calls to behaviors with syscall capabilities

```
BEHAVIOR compute:
  CONTRACT:
    GUARANTEES no_syscall
    REQUIRES helper@abc    # helper must also be no_syscall

  COMPOSITION:
    CALL helper input -> output
```

### Guarantee: `writes_output`

All outputs written on all paths.

**Compiler verifies:**
- Data flow analysis
- Every execution path writes to every OUTPUT

```
BEHAVIOR process:
  CONTRACT:
    OUTPUT result int 4
    OUTPUT status int 4
    GUARANTEES writes_output

  IMPLEMENTATION:
    BRANCH condition success failure

    LABEL success
      STORE result value 4
      STORE status 200 4       # Both written
      JUMP done

    LABEL failure
      STORE result 0 4
      STORE status 500 4       # Both written
      JUMP done

    LABEL done
```

---

## Part 6: Concurrency Safety

### Rule: Shared Memory Requires Atomics

Non-atomic access to SHARED handles is forbidden.

```
shared = ALLOC 64 bytes SHARED

STORE shared value 8           # ERROR - must use atomic
ATOMIC_STORE shared value 8    # OK

LOAD shared 8                  # ERROR - must use atomic
ATOMIC_LOAD shared 8           # OK
```

**Verification:**
- Track SHARED flag on handles
- Reject non-atomic LOAD/STORE on SHARED handles

### Rule: Channel Type Matching

Send/receive types must match channel declaration.

```
ch = CHANNEL int 100

CHANNEL_SEND ch int_value      # OK
CHANNEL_SEND ch float_value    # ERROR - type mismatch
```

### Rule: Ownership Across Spawn

Borrowed handles must remain valid.

```
COMPOSITION:
  buffer = ALLOC 1024 bytes
  worker = SPAWN processor buffer    # buffer borrowed by worker

  FREE buffer                        # ERROR - worker still using it

  WAIT worker
  FREE buffer                        # OK - worker done
```

**Verification:**
- Track borrows across spawn
- Owner cannot free while borrowed
- Borrowed handle valid for spawned pattern lifetime

---

## Part 7: Control Flow Safety

### Rule: Labels Exist

All BRANCH/JUMP targets must be defined labels.

```
BRANCH condition success failure

LABEL success
  ...

LABEL failure
  ...

# Both targets exist - OK
```

```
BRANCH condition success missing    # ERROR - 'missing' not defined
```

### Rule: No Duplicate Labels

Each label unique in behavior.

```
LABEL process
  ...

LABEL process      # ERROR - duplicate label
```

### Rule: Scopes Balanced

Every SCOPE has matching END_SCOPE.

```
SCOPE
  h = ALLOC 64 bytes
END_SCOPE          # OK - balanced

SCOPE
  h = ALLOC 64 bytes
# ERROR - missing END_SCOPE
```

### Rule: All Paths Terminate

> **Enforced (structurally).** This is a decidable reachability check: every block reachable from entry must be able to reach an exit. Ordinary loops pass (they structurally connect to an exit); an exit-less loop *is* rejected. It does **not** decide semantic termination (the halting problem) — only that the control-flow graph has no dead end.

Every execution path must reach end or loop.

```
IMPLEMENTATION:
  BRANCH condition a b

  LABEL a
    ...
    JUMP done

  LABEL b
    ...
    # Falls through - ERROR or must JUMP

  LABEL done
```

---

## Part 8: Catching Invalid References and Made-Up Names

These are the ordinary checks any compiler with a symbol table and a fixed instruction set performs — undefined names, wrong argument counts, unknown operations. Nothing here is novel. They are called out separately only because the *class* of mistake they catch — referencing something that was never defined, calling a primitive that doesn't exist, inventing an identifier — is exactly the kind of thing an AI model does when it confabulates, so a strict front-end rejects a confident-but-wrong reference at compile time instead of at runtime. The mechanism is forty years old; the relevance to a machine author is the only thing worth noting.

### Rule: No Invented References

Every reference must resolve to something defined.

```
# Handles
h = ALLOC 64 bytes
LOAD h 8              # OK - h exists
LOAD x 8              # ERROR - x not defined

# Behaviors
CALL parse-http       # ERROR if parse-http doesn't exist or wrong hash

# Labels
JUMP done             # ERROR if 'done' label not defined

# Primitives
IADD a b 4            # OK - IADD exists
IMIX a b 4            # ERROR - IMIX not a primitive
```

**Verification:**
- Every name resolves to a definition
- Fixed set of 43 primitives - others rejected
- Behaviors verified by hash

### Rule: Correct Primitive Arguments

Each primitive has fixed signature.

```
IADD a b 4            # OK - 3 arguments
IADD a b c 4          # ERROR - too many arguments
IADD a 4              # ERROR - too few arguments

BRANCH cond t f       # OK - 3 arguments
BRANCH cond t         # ERROR - needs false target
```

**Verification:**
- Each primitive has defined argument count and types
- Compiler rejects wrong arity

### Rule: No Self-Reference

Node cannot use itself as input.

```
n1 = IADD n1 5 4      # ERROR - n1 references itself
```

**Verification:**
- Data flow must be acyclic (except through BRANCH/JUMP)

### Rule: Unique Identifiers

No duplicates in scope.

```
n1 = LOAD x 4
n1 = LOAD y 4         # ERROR - duplicate n1

h = ALLOC 64 bytes
h = ALLOC 32 bytes    # ERROR - duplicate h in scope
```

### Rule: Complete Implementation

All declared outputs must be produced.

```
BEHAVIOR compute:
  CONTRACT:
    OUTPUT a int 4
    OUTPUT b int 4

  IMPLEMENTATION:
    STORE a value 4
    # ERROR - 'b' never written
```

### Rule: Used Dependencies

Warn on declared but unused requirements.

```
BEHAVIOR process:
  CONTRACT:
    REQUIRES helper@abc format@def

  COMPOSITION:
    CALL helper input -> output
    # WARNING - format@def declared but never used
```

### Rule: Hash Verification

Protect against context loss across sessions.

```
# AI wrote this in session 1:
REQUIRES parse-http@a1b2c3

# In session 2, parse-http changed to hash d4e5f6
# Compiler catches: "parse-http hash mismatch: expected a1b2c3, got d4e5f6"
```

---

## Part 9: Warnings vs Errors

### Compilation Errors (Reject)

| Category | Examples |
|----------|----------|
| Memory violation | Bounds, lifetime, uninitialized |
| Type violation | Wrong interpretation, missing conversion |
| Contract violation | Missing output, wrong input type |
| Guarantee violation | pure but writes state; missing output |
| Reference error | Undefined handle, label, behavior |
| Concurrency error | Non-atomic on SHARED |
| Syntax error | Wrong arguments, missing END |
| Hash mismatch | Dependency changed |

### Compilation Warnings (Allow but Report)

| Category | Examples |
|----------|----------|
| Unused allocation | ALLOC but never used |
| Unused dependency | REQUIRES but never called |
| Unused label | LABEL but never targeted |
| Unreachable code | Code after unconditional JUMP |
| Constant condition | BRANCH that always goes one way |

---

## Part 10: Verification Summary

### Memory Safety

| Check | Prevents |
|-------|----------|
| Bounds | Buffer overflow |
| Lifetime | Use after free |
| Initialization | Reading garbage |
| Ownership | Double free, invalid free |
| No leaks | Memory leaks |
| No aliasing | Unexpected mutation |

### Type Safety

| Check | Prevents |
|-------|----------|
| Interpretation match | Wrong operations |
| Explicit conversion | Precision loss |
| Size consistency | Truncation, overflow read |

### Contract Safety

| Check | Prevents |
|-------|----------|
| Input matching | Wrong data passed |
| Output matching | Wrong data returned |
| Requires satisfied | Missing dependency |
| Hash integrity | Stale dependencies |

### Guarantee Safety

| Check | Prevents |
|-------|----------|
| pure | Side effects |
| no_alloc | Unexpected allocation |
| writes_output | Missing output |

### Concurrency Safety

| Check | Prevents |
|-------|----------|
| Atomic on SHARED | Data races |
| Channel types | Type confusion |
| Ownership across spawn | Dangling references |

### Control Flow Safety

| Check | Prevents |
|-------|----------|
| Labels exist | Jump to nowhere |
| Unique labels | Ambiguous target |
| Balanced scopes | Resource leaks |
| All paths terminate | Hanging execution |

### Reference and Name Safety (the checks that catch confabulated code)

| Check | Prevents |
|-------|----------|
| References resolve | Made-up handles/behaviors |
| Correct arguments | Wrong primitive usage |
| No self-reference | Circular data flow |
| Unique identifiers | Conflicting definitions |
| Complete implementation | Missing outputs |
| Hash verification | Context loss errors |

---

## Part 11: The Compiler Pipeline

```
SOURCE CODE
    │
    ▼
┌─────────────────────────────────────┐
│         PARSING                      │
│  - Syntax errors                     │
│  - Unknown primitives                │
│  - Argument count                    │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         NAME RESOLUTION              │
│  - All references resolve            │
│  - No duplicates                     │
│  - Labels exist                      │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         TYPE CHECKING                │
│  - Interpretation matching           │
│  - Size consistency                  │
│  - Contract matching                 │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         OWNERSHIP ANALYSIS           │
│  - Lifetime tracking                 │
│  - Borrow checking                   │
│  - Scope validation                  │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         DATA FLOW ANALYSIS           │
│  - Initialization before use         │
│  - writes_output verification        │
│  - No self-reference                 │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         GUARANTEE VERIFICATION       │
│  - pure (no side effects)            │
│  - no_alloc (no escaping alloc)      │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         CONCURRENCY CHECK            │
│  - SHARED uses atomics               │
│  - Ownership across spawn            │
│  - Channel type matching             │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         HASH VERIFICATION            │
│  - Contract hash matches             │
│  - Dependency hashes match           │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│         WARNINGS                     │
│  - Unused allocations                │
│  - Unreachable code                  │
│  - Unused dependencies               │
└─────────────────────────────────────┘
    │
    ▼
SAFE CODE (or errors reported)
```

---

## Part 12: Why This Matters

### For AI

| Benefit | How |
|---------|-----|
| Catches made-up references | References must resolve |
| Catches stale assumptions across sessions | Hash verification |
| Catches incompleteness | All outputs must be written |
| Catches inconsistency | Types must match |

### For Correctness

| Benefit | How |
|---------|-----|
| No crashes | Memory safety |
| No data corruption | Type safety, bounds |
| No races | Concurrency safety |
| Contracts honored | Guarantee verification |

### For Maintainability

| Benefit | How |
|---------|-----|
| Dependencies tracked | Hash verification |
| Changes detected | Hash changes propagate |
| Dead code found | Warnings |
| Complete contracts | Self-documenting |

---

*Document created during ideation session, January 2025*
*Design goal: if it compiles, it's safe — the target the compiler works toward. See [STATUS.md](../STATUS.md) for how much of it the current implementation actually enforces.*
