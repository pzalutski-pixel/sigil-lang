# Graph Format

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements.

## Overview

This document defines how behavior implementations are represented - the actual format for expressing graphs of primitives (leaf behaviors) and composition of behaviors (composite behaviors).

---

## Part 1: The Question

The implementation of a behavior is a graph:
- **Nodes** - operations (primitives or behavior calls)
- **Edges** - data flow between nodes
- **Control flow** - branching and jumping

How do we represent this textually?

---

## Part 2: Format Options Considered

### Option A: JSON

```json
{
  "nodes": [
    {"id": "n1", "op": "LOAD", "handle": "a", "size": 4},
    {"id": "n2", "op": "LOAD", "handle": "b", "size": 4},
    {"id": "n3", "op": "IADD", "a": "n1", "b": "n2", "size": 4},
    {"id": "n4", "op": "STORE", "handle": "result", "value": "n3", "size": 4}
  ]
}
```

### Option B: S-expressions (LISP-style)

```lisp
(STORE result (IADD (LOAD a 4) (LOAD b 4) 4) 4)
```

### Option C: Line-based

```
n1 = LOAD a 4
n2 = LOAD b 4
n3 = IADD n1 n2 4
STORE result n3 4
```

---

## Part 3: Analysis - Why Not JSON

JSON was designed for human-machine interchange with human readability as a goal.

### Why JSON Exists

| JSON Feature | Purpose | AI Need? |
|--------------|---------|----------|
| Key-value pairs | Humans forget what fields mean | No - AI knows the format |
| Nested braces `{ }` | Human visual organization | No - adds complexity |
| Self-describing keys | Humans can read without docs | No - AI knows the schema |
| Quotes on strings | Distinguish strings from keywords | Overhead |
| Commas between items | Human readability | Error-prone for generation |

### JSON Problems for AI

| Problem | Impact |
|---------|--------|
| High token count | Wastes context window |
| Many syntax characters | `{ } [ ] , : "` all can cause errors |
| Nesting depth | Must track state while generating |
| Modification | Change one item, fix surrounding commas |

### Token Count

JSON for simple operation: **~60 tokens**

```json
{
  "nodes": [
    {"id": "n1", "op": "LOAD", "handle": "a", "size": 4},
    {"id": "n2", "op": "LOAD", "handle": "b", "size": 4},
    {"id": "n3", "op": "IADD", "a": "n1", "b": "n2", "size": 4}
  ]
}
```

**Decision: JSON rejected.** Human-oriented, verbose, error-prone.

---

## Part 4: Analysis - What to Learn from LISP

LISP has been the "AI language" historically. What's valuable?

### LISP Concepts Evaluated

| Concept | What It Is | Take It? |
|---------|------------|----------|
| **Homoiconicity** | Code is data, data is code | Yes - graph IS data |
| **Uniform structure** | Everything is `(op args...)` | Yes - uniform patterns |
| **Minimal syntax** | Only parens, symbols, whitespace | Yes - few rules |
| **Prefix notation** | Operator first: `(+ 1 2)` | Yes - no precedence issues |
| **S-expressions** | Nested parentheses | No - error-prone |
| **Deep nesting** | Inline everything | No - hard to modify |

### S-expressions Rejected

```lisp
(STORE result (IADD (LOAD a 4) (LOAD b 4) 4) 4)
```

Problems:
- Parenthesis matching is error-prone
- Deep nesting hard to read/modify
- Change one operation = rebalance many parens
- Context needed to understand structure

### What We Keep from LISP

1. **Uniform structure** - every construct follows same pattern
2. **Prefix notation** - operator comes first
3. **Minimal syntax** - few special characters
4. **Code as data** - implementation is manipulable data structure

---

## Part 5: The Chosen Format - Line-Based

### Design Principles

| Principle | How Applied |
|-----------|-------------|
| Uniform structure | Every line is `[result =] OPERATOR args...` |
| Prefix notation | Operator always first |
| Minimal syntax | Only: newline, space, `=`, `->` |
| Flat, not nested | Explicit node IDs, no deep nesting |
| Line independence | Each line is a complete statement |

### Why Line-Based is AI-Optimal

| Factor | JSON | S-expr | Line-based |
|--------|------|--------|------------|
| Token count | High (~60) | Medium (~28) | Low (~20) |
| Syntax errors | Many | Paren matching | Few |
| Modification | Hard | Hard | Easy (one line) |
| Generation | Track nesting | Track parens | Line by line |
| Scalability | Nesting grows | Nesting grows | Stays flat |

### Token Count

Line-based for same operation: **~20 tokens**

```
n1 = LOAD a 4
n2 = LOAD b 4
n3 = IADD n1 n2 4
STORE result n3 4
```

**3x more compact than JSON.**

---

## Part 6: Format Specification

### Overall Structure

```
BEHAVIOR name
CONTRACT
  INPUT name interpretation size
  INPUT name interpretation size
  OUTPUT name interpretation size
  REQUIRES behavior@hash behavior@hash
  CAPABILITIES syscall:name syscall:name    # REJECTED - not in implemented grammar
  MEMORY name name
  GUARANTEES guarantee guarantee
HASH hexstring
IMPLEMENTATION
  ... nodes or composition ...
END
```

> **IMPLEMENTATION NOTE (January 2025):** CAPABILITIES line was rejected - not in implemented grammar. SYSCALL primitive was also rejected. System interactions use NATIVE behaviors backed by C runtime instead.

### Contract Section

```
CONTRACT
  INPUT request bytes 2048
  INPUT db_conn bytes 64
  OUTPUT response bytes 4096
  OUTPUT status int 4
  REQUIRES parse-http@a1b2c3 query-db@d4e5f6
  MEMORY cache pool
  GUARANTEES pure no_alloc writes_output
```

Each line is space-separated tokens. Position determines meaning.

### Implementation - Leaf Behaviors (Primitives)

```
IMPLEMENTATION
  n1 = LOAD a 4
  n2 = LOAD b 4
  n3 = IADD n1 n2 4
  n4 = IMUL n3 2 4
  STORE result n4 4
END
```

**Node format:** `id = OPERATOR args...`

**Store format:** `STORE handle value size` (no result ID)

### Implementation - Memory Operations

```
# Allocation
h1 = ALLOC 64 bytes
h2 = ALLOC 8 int

# Load and Store
v1 = LOAD h1 8
v2 = LOAD h1 8 24          # With offset
STORE h2 v1 8
STORE h1 v2 8 24           # With offset

# Free
FREE h1
```

### Implementation - Arithmetic

```
# Integer
n1 = IADD a b 4
n2 = ISUB a b 4
n3 = IMUL a b 4
n4 = IDIV a b 4
n5 = IMOD a b 4
n6 = INEG a 4

# Float
f1 = FADD x y 8
f2 = FSUB x y 8
f3 = FMUL x y 8
f4 = FDIV x y 8
f5 = FNEG x 8

# Conversion
i1 = FTOI f1 8 4           # float8 to int4
f6 = ITOF i1 4 8           # int4 to float8
```

### Implementation - Comparison

```
# Integer comparison (returns 0 or 1)
c1 = IEQ a b 4
c2 = INE a b 4
c3 = ILT a b 4
c4 = IGT a b 4
c5 = ILE a b 4
c6 = IGE a b 4

# Float comparison
c7 = FEQ x y 8
c8 = FLT x y 8
```

### Implementation - Bitwise

```
b1 = AND a b 4
b2 = OR a b 4
b3 = XOR a b 4
b4 = NOT a 4
b5 = SHL a 2 4             # Shift left by 2
b6 = SHR a 2 4             # Shift right by 2
```

### Implementation - Control Flow

```
IMPLEMENTATION
  n1 = LOAD x 4
  n2 = IGT n1 0 4
  BRANCH n2 positive negative

  LABEL positive
    n3 = IADD n1 1 4
    STORE result n3 4
    JUMP done

  LABEL negative
    STORE result 0 4
    JUMP done

  LABEL done
END
```

**Branch:** `BRANCH condition true_label false_label`

**Jump:** `JUMP label`

**Label:** `LABEL name`

### Implementation - Scopes

```
IMPLEMENTATION
  SCOPE
    t1 = ALLOC 64 bytes
    t2 = ALLOC 8 int

    # ... use t1, t2 ...

  END_SCOPE                  # t1, t2 automatically freed
END
```

### Implementation - Syscalls

> **REJECTED (January 2025):** SYSCALL primitive was removed from the implemented grammar. System interactions use NATIVE behaviors backed by C runtime instead. See PRIMITIVE-LAYER.md for rationale.

```
# Raw syscall (used in leaf behaviors) - NOT IMPLEMENTED
fd = SYSCALL 2 path 0 0     # open (Linux)
n = SYSCALL 0 fd buf 1024   # read (Linux)
SYSCALL 3 fd                # close (Linux)
```

### Implementation - Atomics

```
v1 = ATOMIC_LOAD addr 8
ATOMIC_STORE addr v1 8
success = CAS addr expected new 8
old = ATOMIC_ADD addr 1 8
old = ATOMIC_SUB addr 1 8
```

### Composition - Composite Behaviors

```
COMPOSITION
  parsed = CALL parse-http request
  data = CALL query-db db_conn parsed.query
  CALL format-json data -> response
  SET status 200
END
```

**Call with result:** `result = CALL behavior args...`

**Call with output handle:** `CALL behavior args -> output_handle`

**Set constant:** `SET handle value`

### Composition - Control Flow

```
COMPOSITION
  cached = CALL check-cache cache request
  BRANCH cached.hit cache_hit cache_miss

  LABEL cache_hit
    CALL copy cached.data -> response
    JUMP done

  LABEL cache_miss
    data = CALL fetch-data db_conn request
    CALL format-response data -> response
    CALL update-cache cache request response
    JUMP done

  LABEL done
END
```

---

## Part 7: Pattern Structure

```
PATTERN http-server

MEMORY
  cache bytes 1048576
  pool bytes 65536

ON_CREATE
  cache = ALLOC 1048576 bytes
  pool = ALLOC 65536 bytes

BEHAVIOR handle-request
  CONTRACT
    INPUT request bytes 2048
    OUTPUT response bytes 4096
    REQUIRES process-request@e2d5f8
    MEMORY cache pool
    GUARANTEES writes_output
  HASH b1c3d5
  COMPOSITION
    ...
  END

BEHAVIOR health-check
  CONTRACT
    OUTPUT status int 4
    GUARANTEES pure writes_output
  HASH a9b8c7
  IMPLEMENTATION
    SET status 200
  END

ON_DESTROY
  FREE cache
  FREE pool

END_PATTERN
```

---

## Part 8: Complete Example

### Leaf Behavior

```
BEHAVIOR add-and-double
CONTRACT
  INPUT a int 4
  INPUT b int 4
  OUTPUT result int 4
  GUARANTEES pure no_alloc writes_output
HASH a1b2c3d4
IMPLEMENTATION
  v1 = LOAD a 4
  v2 = LOAD b 4
  sum = IADD v1 v2 4
  doubled = IADD sum sum 4
  STORE result doubled 4
END
```

### Composite Behavior

```
BEHAVIOR process-customer-request
CONTRACT
  INPUT request bytes 2048
  INPUT db_conn bytes 64
  OUTPUT response bytes 4096
  OUTPUT status int 4
  REQUIRES parse-http@a1b2 validate-auth@c3d4 query-db@e5f6 format-json@a7b8
  GUARANTEES writes_output
HASH a9b0c1d2
COMPOSITION
  parsed = CALL parse-http request
  BRANCH parsed.valid auth_check invalid_request

  LABEL auth_check
    auth = CALL validate-auth parsed.headers
    BRANCH auth.valid fetch_data unauthorized

  LABEL fetch_data
    data = CALL query-db db_conn parsed.query
    CALL format-json data -> response
    SET status 200
    JUMP done

  LABEL invalid_request
    SET status 400
    JUMP done

  LABEL unauthorized
    SET status 401
    JUMP done

  LABEL done
END
```

### Library File

```
LIBRARY http-utils

BEHAVIOR parse-http
CONTRACT
  INPUT request bytes 2048
  OUTPUT parsed bytes 512
  GUARANTEES pure no_alloc writes_output
HASH a1b2c3d4
IMPLEMENTATION
  ...
END

BEHAVIOR format-response
CONTRACT
  INPUT data bytes 4096
  INPUT status int 4
  OUTPUT response bytes 4096
  GUARANTEES pure no_alloc writes_output
HASH e5f6a7b8
IMPLEMENTATION
  ...
END

END_LIBRARY
```

---

## Part 9: Why This Format Works

### For AI Generation

| Property | Benefit |
|----------|---------|
| Line by line | Generate one statement at a time |
| No nesting | No bracket/paren state to track |
| Uniform structure | Same pattern for all operations |
| Prefix notation | Operator always first, predictable |
| Explicit node IDs | Clear data flow references |

### For AI Modification

| Property | Benefit |
|----------|---------|
| Line independence | Change one line without affecting others |
| Flat structure | No rebalancing needed |
| Clear boundaries | END markers for sections |
| Explicit references | Find all uses of a node by searching |

### For Context Limits

| Property | Benefit |
|----------|---------|
| Compact | 3x fewer tokens than JSON |
| Self-contained | Each behavior is complete unit |
| Contract first | Understand interface without reading implementation |
| Hash references | Don't need to load dependencies |

### For Compiler

| Property | Benefit |
|----------|---------|
| Simple parsing | Line-by-line, split by whitespace |
| Clear sections | CONTRACT, IMPLEMENTATION, COMPOSITION |
| Explicit types | Interpretation and size always specified |
| No ambiguity | Position determines meaning |

---

## Part 10: Summary

### What We Chose

**Line-based format** with:
- Uniform structure: `[result =] OPERATOR args...`
- Prefix notation: operator always first
- Minimal syntax: space, newline, `=`, `->`, labels
- Flat structure: explicit node IDs, no nesting
- Clear sections: CONTRACT, IMPLEMENTATION, COMPOSITION

### What We Rejected

| Format | Why Rejected |
|--------|--------------|
| JSON | Human-oriented, verbose, error-prone syntax |
| S-expressions | Parenthesis matching errors, deep nesting |
| Binary | AI generates text, not binary |
| Custom syntax like C | Unnecessary complexity, precedence rules |

### What We Took from LISP

| Concept | How Applied |
|---------|-------------|
| Uniform structure | Every line follows same pattern |
| Prefix notation | Operator always first |
| Minimal syntax | Few special characters |
| Code as data | Graph is manipulable data structure |

### What We Didn't Take from LISP

| Concept | Why Not |
|---------|---------|
| S-expressions | Parenthesis errors |
| Deep nesting | Hard to modify |
| Macros | We have behavior composition |

---

*Document created during ideation session, January 2025*
*Line-based format: uniform, minimal, flat, AI-optimized*
