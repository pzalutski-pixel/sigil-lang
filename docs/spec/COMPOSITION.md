# Composition

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements. Note: cross-behavior validation of the wiring *between* behaviors is now substantially enforced — **outputs-consumed** (every CALL output routed or `DISCARD`ed, E0511), **transitive purity**, **dependency cycles**, and **dependency-hash pins**. What remains is the full edge-materialized gap detection (chiefly inputs-sourced over the wired graph).

## Overview

Composition is how behaviors connect to form larger behaviors. This document defines the wiring model - how outputs flow to inputs, how control flows through composed behaviors, and how the "mind map" of the program emerges from composition.

---

## Part 1: The Core Principle

**Composition is wiring, not new logic.**

```
LEAF BEHAVIOR:
  Graph of primitives.
  Actually does computation.

COMPOSITE BEHAVIOR:
  Wiring of other behaviors.
  No primitives.
  Just connects outputs to inputs.
```

All actual work happens in leaf behaviors. Composite behaviors only describe how to connect workers.

---

## Part 2: The Hierarchy

```
Level 0: PRIMITIVES
         LOAD, STORE, IADD, BRANCH, JUMP...
         Built into compiler. Axioms.
         (SYSCALL was removed; system access is via NATIVE behaviors.)

Level 1: LEAF BEHAVIORS
         Graphs of primitives.
         Actually do work.
         Example: parse-http, format-json

Level 2: COMPOSITE BEHAVIORS
         Wire level 1 behaviors.
         No primitives, just connections.
         Example: handle-request

Level 3: COMPOSITE BEHAVIORS
         Wire level 2 behaviors.
         Example: request-pipeline

Level N: ... unlimited nesting ...
```

**Key insight:** Implementation only exists at level 1 (leaf). Every other level is just composition.

---

## Part 3: Data Flow

### Basic Connection

Output of one behavior becomes input of another.

```
COMPOSITION
  parsed = CALL parse-http request
  data = CALL query-db db_conn parsed
  CALL format-json data -> response
END
```

**Flow:**
```
request ──→ [parse-http] ──→ parsed ──→ [query-db] ──→ data ──→ [format-json] ──→ response
               │                            │
           db_conn ─────────────────────────┘
```

### Field Access

When a behavior returns structured data, access fields with dot notation.

```
COMPOSITION
  result = CALL parse-http request

  # Access fields of result
  CALL validate-method result.method
  CALL process-path result.path
  CALL check-headers result.headers
END
```

**Note:** Fields are just offsets into the output handle. AI tracks what fields exist based on the behavior's contract.

### Multiple Outputs

When a behavior has multiple outputs, use named outputs.

```
# Contract has: OUTPUT status int 4, OUTPUT body bytes 4096

COMPOSITION
  CALL process request -> status, body

  # status and body are separate handles
  CALL log-status status
  CALL send-body body
END
```

### Splitting Output

One output can flow to multiple inputs.

```
COMPOSITION
  data = CALL fetch-data source

  # data used by multiple behaviors
  CALL cache-data cache data
  CALL transform-data data -> transformed
  CALL log-data data
END
```

No special syntax. Just reference the same name multiple times.

---

## Part 4: Control Flow

### Branch

Conditional execution paths.

```
COMPOSITION
  result = CALL validate input
  BRANCH result.valid success failure

  LABEL success
    CALL process input -> output
    SET status 200
    JUMP done

  LABEL failure
    SET status 400
    JUMP done

  LABEL done
END
```

**Syntax:** `BRANCH condition true_label false_label`

### Jump

Unconditional jump to a label.

```
JUMP label_name
```

### Labels

Named points in the composition.

```
LABEL label_name
  ... statements ...
```

Labels define sections. Control flows through unless JUMP or BRANCH redirects.

---

## Part 5: Loops

### Design Decision: No Loop Construct

**Options considered:**

| Option | Description | Decision |
|--------|-------------|----------|
| LOOP construct | Explicit loop syntax | Rejected - unnecessary |
| BRANCH + JUMP | Use existing control flow | Accepted |
| FOR-EACH behavior | Looping as a pattern | Available as library behavior |

**Why no loop construct:**
- BRANCH and JUMP already exist
- Loop is just: do work, check condition, jump back
- No new concepts needed
- Compiler can recognize loop patterns for optimization

### Loop via BRANCH + JUMP

```
COMPOSITION
  LABEL loop_start
    item = CALL next-item iterator
    BRANCH item.done loop_end loop_body

  LABEL loop_body
    CALL process item.value
    JUMP loop_start

  LABEL loop_end
END
```

### Loop Patterns as Behaviors

For common patterns, use library behaviors:

```
COMPOSITION
  # for-each is a library behavior
  CALL for-each items process-item

  # map is a library behavior
  results = CALL map items transform-item

  # filter is a library behavior
  filtered = CALL filter items predicate
END
```

These are just composite behaviors that internally use BRANCH + JUMP.

---

## Part 6: Parallelism

> **DESIGN INTENT ONLY — this section's *auto-inferred* parallelism is not implemented.**
> The compiler does **not** infer or emit parallel execution from data dependencies —
> independent CALLs still run sequentially. (Note: *explicit* concurrency via SPAWN/WAIT/CHANNEL
> **does** work — it runs on a green-thread runtime, see [STATUS.md](../STATUS.md) — but that is
> the programmer asking for concurrency, not the compiler inferring it, which is what this
> section is about.) The discussion below is preserved as design rationale.

### Design Decision: Compiler Infers Parallelism

**Options considered:**

| Option | Description | Decision |
|--------|-------------|----------|
| PARALLEL block | Explicit parallel construct | Rejected - unnecessary complexity |
| Compiler inference | Compiler detects independent operations | Accepted |

**Why compiler inference:**
- AI specifies data dependencies by how outputs connect to inputs
- Compiler sees which operations are independent
- Independent operations can run in parallel
- AI focuses on correctness, compiler optimizes

### How It Works

```
COMPOSITION
  # These have no dependencies on each other
  user = CALL fetch-user user_id
  orders = CALL fetch-orders user_id
  prefs = CALL fetch-preferences user_id

  # This depends on all three
  CALL combine user orders prefs -> response
END
```

**Compiler sees:**
```
user_id ──┬──→ [fetch-user] ──→ user ──────┐
          │                                 │
          ├──→ [fetch-orders] ──→ orders ──┼──→ [combine] ──→ response
          │                                 │
          └──→ [fetch-prefs] ──→ prefs ────┘
```

**Compiler infers:**
- fetch-user, fetch-orders, fetch-prefs can run in parallel
- combine waits for all three
- No annotation needed from AI

### Dependencies Prevent Parallelism

```
COMPOSITION
  a = CALL step-one input
  b = CALL step-two a          # Depends on a
  c = CALL step-three b        # Depends on b
END
```

These must run sequentially. Compiler sees the dependencies.

---

## Part 7: The Mind Map

### Composition IS the Program

The composition graph is the "mental map" of the program.

```
COMPOSITE: web-server
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  ┌──────────┐      ┌─────────────────┐      ┌─────────────┐   │
│  │  listen  │─────▶│ handle-request  │─────▶│   respond   │   │
│  └──────────┘      └────────┬────────┘      └─────────────┘   │
│        ▲                    │                      │           │
│        │                    ▼                      │           │
│        │           ┌──────────────┐                │           │
│        │           │  query-db    │                │           │
│        │           └──────────────┘                │           │
│        │                                           │           │
│        └───────────────────────────────────────────┘           │
│                         (loop)                                  │
└─────────────────────────────────────────────────────────────────┘
```

Each box is a behavior. Arrows are data/control flow. This IS the program.

### Zooming In

Each behavior can be expanded to show its internal structure.

```
COMPOSITE: handle-request (zoom in)
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  ┌─────────────┐    ┌────────────┐    ┌────────────────┐       │
│  │ parse-http  │───▶│  validate  │───▶│  route-request │       │
│  └─────────────┘    └─────┬──────┘    └───────┬────────┘       │
│                           │                    │                │
│                     ┌─────┴─────┐        ┌─────┴─────┐         │
│                     ▼           ▼        ▼           ▼         │
│                 [success]   [failure]  [api]      [static]     │
│                     │           │        │           │         │
│                     ▼           ▼        ▼           ▼         │
│                ┌─────────┐ ┌────────┐ ┌──────┐  ┌─────────┐   │
│                │query-db │ │ error  │ │ api  │  │  file   │   │
│                └────┬────┘ └───┬────┘ └──┬───┘  └────┬────┘   │
│                     │          │         │           │         │
│                     └──────────┴─────────┴───────────┘         │
│                                    │                            │
│                                    ▼                            │
│                            ┌─────────────┐                     │
│                            │ format-resp │                     │
│                            └─────────────┘                     │
└─────────────────────────────────────────────────────────────────┘
```

### Zooming Out

Multiple composites form larger systems.

```
SYSTEM LEVEL:
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│  web-server   │────▶│   database    │────▶│    cache      │
└───────────────┘     └───────────────┘     └───────────────┘
        │                                           │
        └───────────────────────────────────────────┘
```

### For Context-Limited AI

AI never sees the whole map at once. AI sees:
- One behavior at a time
- Its contract (inputs, outputs, requires)
- Its immediate wiring (composition)

Other behaviors are referenced by contract hash, not by implementation.

```
BEHAVIOR handle-request
CONTRACT
  ...
  REQUIRES parse-http@a1b2 query-db@c3d4 format-json@e5f6
  ...

COMPOSITION
  # AI knows parse-http exists and its contract
  # AI does NOT need to see parse-http's implementation
  parsed = CALL parse-http request
  ...
END
```

---

## Part 8: Composition Patterns

### Sequential Pipeline

```
COMPOSITION
  a = CALL step1 input
  b = CALL step2 a
  c = CALL step3 b
  CALL step4 c -> output
END
```

### Fan-Out (One to Many)

```
COMPOSITION
  data = CALL fetch source

  CALL process-a data -> result_a
  CALL process-b data -> result_b
  CALL process-c data -> result_c
END
```

### Fan-In (Many to One)

```
COMPOSITION
  a = CALL fetch-a source
  b = CALL fetch-b source
  c = CALL fetch-c source

  CALL combine a b c -> output
END
```

### Conditional (Branch)

```
COMPOSITION
  check = CALL validate input
  BRANCH check.ok success failure

  LABEL success
    CALL process input -> output
    JUMP done

  LABEL failure
    CALL handle-error input -> output
    JUMP done

  LABEL done
END
```

### Loop (Iteration)

```
COMPOSITION
  LABEL start
    next = CALL get-next iterator
    BRANCH next.done end process

  LABEL process
    CALL handle next.item
    JUMP start

  LABEL end
END
```

### Error Handling

```
COMPOSITION
  result = CALL risky-operation input
  BRANCH result.success success error

  LABEL success
    CALL continue result.value -> output
    SET status 200
    JUMP done

  LABEL error
    CALL log-error result.error
    SET status 500
    JUMP done

  LABEL done
END
```

---

## Part 9: What We Decided

### Included

| Feature | Why |
|---------|-----|
| Explicit data flow | Outputs explicitly connect to inputs by name |
| Field access | Dot notation for structured outputs |
| BRANCH/JUMP/LABEL | Minimal control flow, composes into patterns |
| Compiler-inferred parallelism | AI focuses on correctness, compiler optimizes |

### Rejected

| Feature | Why Rejected |
|---------|--------------|
| Explicit LOOP construct | BRANCH + JUMP sufficient |
| PARALLEL block | Compiler can infer from dependencies |
| Complex control flow | Keep it minimal, compose patterns |

---

## Part 10: Summary

### Composition Is

- Wiring outputs to inputs
- Control flow via BRANCH, JUMP, LABEL
- Building the "mind map" of the program
- Hierarchical: leaf behaviors do work, composites wire them

### Composition Is Not

- New logic or computation
- Primitives (those are only in leaf behaviors)
- Complex syntax (just data flow and branches)

### The Formula

```
PROGRAM = COMPOSITION OF BEHAVIORS

BEHAVIOR = CONTRACT + (IMPLEMENTATION | COMPOSITION)

LEAF: Has IMPLEMENTATION (primitives)
COMPOSITE: Has COMPOSITION (wiring)
```

---

*Document created during ideation session, January 2025*
*Composition is wiring. Leaf behaviors do the work. The composition IS the mind map.*
