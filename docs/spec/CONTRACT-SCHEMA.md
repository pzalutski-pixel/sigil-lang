# Contract Schema

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements. (Contracts and hashing are implemented; not all declarable guarantees are enforced.)

> **Notation — read this first.** For readability this document writes contract fields in a *schematic* form — e.g. `name: handle(bytes, 2048)`, under `INPUT:` / `OUTPUT:` / `REQUIRES:` headers, and composition as `behavior(args) -> out`. That is **not** the surface syntax. The actual syntax the compiler accepts is flatter — `INPUT name bytes 2048`, `OUTPUT name int 4`, `REQUIRES name@hash`, and `out = CALL behavior args` — and is defined by the [Language Reference](../SIGIL-LANGUAGE-REFERENCE.md), which is authoritative for syntax. The two notations describe the same contracts; only the spelling differs. (See the hello-world example in the project README for real syntax.)

## Overview

This document defines the structure of behavior contracts - the interface that describes what a behavior does, what it needs, and what it guarantees. The contract serves as requirement, validation, and documentation in one artifact.

---

## Part 1: The Role of Contracts

From the Unified Behavior principle:

```
BEHAVIOR = REQUIREMENT + IMPLEMENTATION + VALIDATION

Contract is the requirement.
Contract is what the compiler checks the implementation against.
One artifact — requirement and check can't drift apart (though the
compiler only enforces part of the contract today; see STATUS.md).
```

The contract must contain everything needed to:
1. **Use** the behavior (what goes in, what comes out)
2. **Verify** the implementation (compiler checks against contract)
3. **Compose** with other behaviors (dependencies declared)
4. **Work across context boundaries** (self-contained, survives session loss)

---

## Part 2: Contract Sections

A contract has four sections:

```
CONTRACT:
  INPUT:       # What the behavior receives
  OUTPUT:      # What the behavior produces
  REQUIRES:    # What the behavior needs (behavior dependencies, incl. NATIVE stdlib)
  GUARANTEES:  # What the behavior promises (compiler-verified)
```

---

## Part 3: INPUT Section

### What Needs to Be Specified

For each input:
- Name (identifier)
- Interpretation (int, float, bytes)
- Size (in bytes)

### Design Decision: Everything is a Handle

**Options considered:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| Values + Handles | Small data passed directly, large by reference | Matches human intuition | Two mechanisms, more complex |
| Handles only | Everything passed via handle | One consistent model | Slight overhead for tiny values |

**Analysis from AI perspective:**

```
Human thinks: "I want to write foo(42) not foo(handle_containing_42)"
Machine reality: Both are just bytes. Register vs memory is optimization.
AI need: One consistent model. No special cases.
```

The distinction between "value" and "reference" is human mental model optimization. At machine level, everything is bytes moved between locations. Compiler can optimize small handles to registers.

**Decision:** Everything is a handle. One model.

### Input Ownership

All inputs are borrowed. The callee:
- Can READ from input handles
- Can WRITE to input handles (if semantically appropriate)
- Cannot FREE input handles
- Does not own input handles

This was established in the Memory Model. Inputs are always borrowed.

### Format

```
INPUT:
  name: handle(interpretation, size)
```

**Examples:**
```
INPUT:
  request_data: handle(bytes, 2048)
  customer_id: handle(int, 8)
  price: handle(float, 8)
```

---

## Part 4: OUTPUT Section

### What Needs to Be Specified

For each output:
- Name (identifier)
- Interpretation (int, float, bytes)
- Size (in bytes)

### Design Decision: Output Handles Only, No Return Values

**Options considered:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| Return values | Behavior returns data directly | Familiar, clean syntax | Ownership ambiguity, two output mechanisms |
| Output handles | Caller provides handles, callee writes | Clear ownership, one model | Slightly verbose |

**Analysis from AI perspective:**

```
Human thinks: "return result" feels natural
Machine reality: Both just move bytes to a destination
AI need: One model. Ownership must be unambiguous.
```

Return values create questions:
- Who owns the returned data?
- Is it allocated by callee? (ownership transfer)
- Is it a copy? (where from?)

Output handles answer everything:
- Caller allocates, caller owns
- Callee just writes
- No ownership transfer

**Decision:** Output handles only. Caller always provides. No return values.

### Format

```
OUTPUT:
  name: handle(interpretation, size)
```

**Examples:**
```
OUTPUT:
  response: handle(bytes, 4096)
  status_code: handle(int, 4)
  total_price: handle(float, 8)
```

---

## Part 5: REQUIRES Section

### What Can Be Required

Three categories of requirements:

#### 1. Other Behaviors

```
REQUIRES:
  behaviors:
    - parse-http@a3f8c2
    - format-json@b7d4e1
    - query-db@c9f2a0
```

The `@hash` suffix is the contract hash of the dependency (see Part 7).

#### 2. System Capabilities

> **REJECTED (January 2025):** CAPABILITIES and the SYSCALL primitive were removed from the implemented grammar. With SYSCALL rejected, capability tracking became unnecessary. System interactions now go through NATIVE behaviors (e.g. file/network stdlib behaviors) backed by a trusted C runtime, and are required like any other behavior dependency. The block below is retained as a record of the rejected design — what was tried and why it was dropped.

```
# REJECTED design - not in implemented grammar:
REQUIRES:
  capabilities:
    - syscall:read
    - syscall:write
    - syscall:socket
```

The rejected design treated capabilities as permissions to use certain syscalls. In the implemented language, a behavior that needs system access instead REQUIRES the relevant NATIVE stdlib behavior (e.g. `open@<hash>`, `read@<hash>`).

#### 3. Pattern Memory

```
REQUIRES:
  memory:
    - cache
    - connection_pool
```

Access to pattern-level memory declared in the parent pattern's MEMORY section.

### Format

```
REQUIRES:
  behaviors: [name@hash, ...]   # includes NATIVE stdlib behaviors for system access
  memory: [name, ...]
```

> The `capabilities:` field was rejected and is not part of the implemented grammar. System access is requested via NATIVE behavior dependencies in `behaviors:`.

Any section can be empty or omitted if not needed.

---

## Part 6: GUARANTEES Section

### Design Decision: Only Verifiable Guarantees

**Options considered:**

| Guarantee | What It Means | Compiler Can Verify? |
|-----------|---------------|---------------------|
| `total` | Always terminates, produces output | No (halting problem) |
| `pure` | No side effects, deterministic | Yes |
| `no_alloc` | No memory allocation in implementation | Yes |
| `writes_output` | All outputs written on all paths | Yes |
| `bounded_time(n)` | Runs in O(n) time | No (undecidable) |

**Analysis from AI perspective:**

```
Human wants: "I promise this is fast and always terminates"
Compiler reality: Can only check what's statically analyzable
AI need: Guarantees only matter if enforced. Unverifiable = useless.
```

Unverifiable guarantees are just comments. They can't be enforced. They provide false confidence.

**Decision:** Only include guarantees the compiler can verify.

### Verifiable Guarantees

> **Current enforcement (see [STATUS.md](../STATUS.md)):** `writes_output` is reliably enforced, and `pure` is now enforced both within a behavior **and transitively** through CALL chains (a `pure` behavior that depends on a non-pure one is a compile error); the remaining gap is *undeclared* pattern-`MEMORY` tracking. `no_alloc` is enforced — an ALLOC inside a balanced `SCOPE` is freed at `END_SCOPE` and satisfies it; only an allocation that escapes is rejected. Treat the "Compiler verifies" lists below as the intended design.

#### `pure`

No observable side effects. Deterministic output for same input.

Compiler verifies:
- No writes except to OUTPUT handles
- No reads/writes to pattern MEMORY
- No CHANNEL operations (and `pure` is now enforced transitively — a pure behavior cannot depend on an impure one)

```
// Pure behavior - only transforms input to output
GUARANTEES:
  - pure
```

#### `no_alloc`

No memory allocation in implementation (except scoped temporaries that are freed).

Compiler verifies:
- No ALLOC that escapes scope
- All ALLOC matched by FREE at scope end

```
// Useful for real-time or memory-constrained contexts
GUARANTEES:
  - no_alloc
```

> **REMOVED (January 2025):** A `no_syscall` guarantee previously existed here. It was removed from the language: with the SYSCALL primitive rejected, all system interactions go through NATIVE stdlib behaviors rather than user code, so the guarantee became meaningless.

#### `writes_output`

All OUTPUT handles are written on all execution paths.

Compiler verifies:
- Data flow analysis
- Every path through implementation writes to every output

```
// Caller guaranteed to get valid output
GUARANTEES:
  - writes_output
```

### Format

```
GUARANTEES:
  - pure
  - no_alloc
  - writes_output
```

Any guarantee not listed is not promised.

---

## Part 7: Contract Hash

### The Problem

AI has limited context. Sessions are stateless. If AI modifies behavior A, how does behavior B know A's contract changed?

### Solution: Contract Hash

The contract has a hash computed from its contents:
- INPUT definitions
- OUTPUT definitions
- REQUIRES declarations
- GUARANTEES list

```
BEHAVIOR parse-http:
  CONTRACT:
    INPUT: ...
    OUTPUT: ...
    REQUIRES: ...
    GUARANTEES: ...

  HASH: a3f8c2...  # Computed from CONTRACT contents
```

### How It's Used

Dependencies reference specific contract versions:

```
BEHAVIOR handle-request:
  CONTRACT:
    REQUIRES:
      behaviors:
        - parse-http@a3f8c2    # This specific contract
        - format-json@b7d4e1
```

If `parse-http`'s contract changes:
1. Its hash changes (e.g., to `d5e9f1`)
2. `handle-request` still references `@a3f8c2`
3. Compiler sees mismatch
4. Forces revalidation of `handle-request`

### Why This Is AI Need, Not Human Convenience

```
Human coordination: "Let's all use version 2.1"
AI need: "Has this dependency changed since I understood it?"

Without hash:
  - AI modifies parse-http
  - AI works on handle-request in new session
  - AI doesn't know parse-http changed
  - Bug: handle-request uses wrong assumptions

With hash:
  - Compiler catches: "parse-http is now @d5e9f1, you declared @a3f8c2"
  - AI forced to reconcile the change
```

**Decision:** Contract hash solves context-limit problem. Required.

---

## Part 8: Complete Contract Example

### Composite Behavior Using NATIVE Stdlib (system access)

System access is not done with SYSCALL primitives. Instead a behavior REQUIRES the
relevant NATIVE stdlib behaviors (file open/read/close) and composes them.

```
BEHAVIOR read-file:
  CONTRACT:
    INPUT:
      path: handle(bytes, 256)

    OUTPUT:
      content: handle(bytes, 65536)
      bytes_read: handle(int, 8)

    REQUIRES:
      behaviors:
        - open@a1b2c3      # NATIVE stdlib - opens a file, backed by C runtime
        - read@d4e5f6      # NATIVE stdlib - reads bytes from a file handle
        - close@a7b8c9     # NATIVE stdlib - closes a file handle
      memory: []

    GUARANTEES:
      - writes_output

  HASH: f4a7b2c8...

  COMPOSITION:
    fd = CALL open path
    CALL read fd content 65536 -> bytes_read
    CALL close fd
```

### Composite Behavior (wires other behaviors)

```
BEHAVIOR process-customer-request:
  CONTRACT:
    INPUT:
      request: handle(bytes, 2048)
      db_conn: handle(bytes, 64)

    OUTPUT:
      response: handle(bytes, 4096)
      status: handle(int, 4)

    REQUIRES:
      behaviors:
        - parse-http@a3f8c2
        - query-db@c9f2a0
        - format-json@b7d4e1
      memory: []

    GUARANTEES:
      - writes_output

  HASH: e2d5f8a1...

  COMPOSITION:
    # Wiring diagram - no primitives, just behavior connections
    parse-http(request) -> parsed
    query-db(db_conn, parsed.query) -> data
    format-json(data) -> response
    set status = 200
```

### Pattern with Memory

```
PATTERN http-server:
  MEMORY:
    cache: handle(bytes, 1048576)
    pool: handle(bytes, 65536)

  ON_CREATE:
    cache = ALLOC(1048576, bytes)
    pool = ALLOC(65536, bytes)

  BEHAVIOR handle-request:
    CONTRACT:
      INPUT:
        request: handle(bytes, 2048)

      OUTPUT:
        response: handle(bytes, 4096)

      REQUIRES:
        behaviors:
          - process-customer-request@e2d5f8a1
        memory:
          - cache
          - pool

      GUARANTEES:
        - writes_output

    HASH: b1c3d5e7...

    COMPOSITION:
      # Can access cache and pool
      check-cache(cache, request) -> cached
      if cached.hit:
        copy cached.data -> response
      else:
        process-customer-request(request, ...) -> response
        update-cache(cache, request, response)
```

---

## Part 9: What We Rejected

### Value Parameters

**What:** Pass small values directly instead of via handle.

**Why rejected:** Human convenience, not AI need. One model (handles) is simpler.

### Return Values

**What:** Behaviors return data directly instead of writing to output handles.

**Why rejected:** Creates ownership ambiguity. Output handles have clear ownership.

### Unverifiable Guarantees

**What:** Promises like `total` (always terminates) or `bounded_time`.

**Why rejected:** Compiler cannot verify. Unverifiable guarantees are meaningless.

### Complex Type Declarations

**What:** Structured types, arrays, optionals in contract.

**Why rejected:** Everything is handle with interpretation. Composite structure is bytes + offsets.

---

## Part 10: Contract Schema Summary

```
BEHAVIOR name:
  CONTRACT:
    INPUT:
      name: handle(interpretation, size)
      ...

    OUTPUT:
      name: handle(interpretation, size)
      ...

    REQUIRES:
      behaviors: [name@hash, ...]   # includes NATIVE stdlib behaviors for system access
      memory: [name, ...]

    GUARANTEES:
      - pure          # No side effects (enforced within-behavior and transitively; residual gap: undeclared pattern-MEMORY)
      - no_alloc      # No escaping allocation (scoped ALLOC ok; enforced)
      - writes_output # All outputs written (reliably enforced)

  HASH: computed...
```

### Design Principles Applied

| Principle | How Applied |
|-----------|-------------|
| Everything is handle | No value vs handle distinction |
| Single ownership | Outputs are caller-provided handles |
| Compile-time safety | Only verifiable guarantees |
| Context-limit friendly | Contract hash for dependency tracking |
| Self-contained | Contract has everything needed to use behavior |

---

## Part 11: For AI Working With Contracts

When AI reads a contract:
```
INPUT tells: What handles to prepare before calling
OUTPUT tells: What handles to allocate for results
REQUIRES tells: What other behaviors are used (including NATIVE stdlib for system access)
GUARANTEES tells: What properties are enforced
HASH tells: Exact version of this contract
```

When AI writes a contract:
```
1. Declare all inputs with interpretation and size
2. Declare all outputs with interpretation and size
3. List all behavior dependencies with their hashes (including NATIVE stdlib behaviors for any system access)
4. List pattern memory accessed
5. Add guarantees that implementation will honor
6. Hash is computed automatically from above
```

Contract is complete. No external context needed. Survives session boundaries.

---

*Document created during ideation session, January 2025*
*Contract = INPUT + OUTPUT + REQUIRES + GUARANTEES + HASH*
