# Type System

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements. (Type and size checking is implemented.)

## Overview

This document defines how data interpretation works in the AI-native language. We use the term "interpretation" rather than "type" because data is fundamentally bytes - the interpretation determines which operations are valid.

---

## Part 1: The Core Principle

**Data is bytes. Operations are typed.**

At the machine level, memory is just bytes. The value `0x40490FDB` could be:
- The float `3.14159...`
- The integer `1078530011`
- Part of a string
- Anything

The bytes don't "know" what they are. The OPERATION interprets them.

```
IADD interprets bytes as integer, performs integer addition.
FADD interprets bytes as float, performs float addition.
Same bytes, different interpretation, different results.
```

---

## Part 2: The Problem

### Without Type Information

```
h = ALLOC(8)
STORE(h, some_value, 8)

// Later...
x = LOAD(h, 8)
result = ???ADD(x, 5, 8)    // IADD or FADD?
```

How does AI know which operation to use?

### Options Considered

| Option | Description | Compile-time Safe? | Survives Context Loss? |
|--------|-------------|-------------------|----------------------|
| **A: AI remembers** | No type in language. AI tracks internally. | No | No |
| **B: Handle metadata** | Interpretation declared at allocation. | Yes | Yes |
| **C: Explicit ops only** | AI picks IADD vs FADD. No verification. | No | Partially |

### Decision: Option B

**Handles carry interpretation metadata.**

Reasoning:
- Compile-time safety is paramount (core principle)
- AI has limited context (may lose session state)
- Interpretation in handle survives context boundaries
- Compiler can verify operation/interpretation match

---

## Part 3: The Four Interpretations

### Integer (`int`)

For whole number arithmetic.

```
h = ALLOC(4, int)    // 4-byte integer

Valid operations:
  IADD, ISUB, IMUL, IDIV, IMOD, INEG
  IEQ, INE, ILT, IGT, ILE, IGE
  AND, OR, XOR, NOT, SHL, SHR, SAR

Valid sizes: 1, 2, 4, 8 bytes
```

### Float (`float`)

For decimal/fractional arithmetic.

```
h = ALLOC(8, float)    // 8-byte float (double precision)

Valid operations:
  FADD, FSUB, FMUL, FDIV, FNEG
  FEQ, FNE, FLT, FGT, FLE, FGE

Valid sizes: 4, 8 bytes
```

### Bytes (`bytes`)

For raw data with no arithmetic interpretation.

```
h = ALLOC(100, bytes)    // 100 bytes, raw

Valid operations:
  LOAD, STORE, COPY only
  No arithmetic operations

Valid sizes: Any
```

### String (`string`)

For variable-length text and other self-describing data.

```
msg = "Hi!"    // 11 bytes total: [ 3 : 8-byte LE length ][ H i ! ]

Layout:  [ len : 8-byte little-endian ][ content bytes … ]

Valid operations:
  LEN (read the length prefix in O(1))
  string behaviors — concat, substring, compare — built over LOAD/STORE/COPY

Valid sizes: none — the value is self-describing and carries its own length
```

Unlike `int` / `float` / `bytes`, a `string` declares **no size** in a contract (`INPUT text string`, not `bytes 65536` plus a separate length): it is length-prefixed, so the value carries how much of it is real. `bytes N` stays for genuinely **fixed** buffers — a record, a packet, file I/O; `string` is for variable-length text.

#### Why `string` is a type — a reconsidered decision

`string` was *not* in the original design, and leaving it out was a deliberate choice. Both that choice and its reversal are kept here, because a revised decision is part of the record.

**The original choice — string as `bytes`.** From an AI author's perspective, a string is just bytes: its length is a stored value or a null-scan, and "string operations" — concat, substring, compare — are *behaviors* built on `LOAD`/`STORE`/`COPY`, not primitive operations. Special string *syntax* is human ergonomics, which this model strips. So strings were treated as the `bytes` interpretation with string behaviors on top — not a separate type — which kept the core principle clean: data is bytes, operations are typed.

**Why it was reversed — string as a self-describing type.** That held until it collided with a principle this model values more: a contract must accurately describe the behavior, and *self-description is a correctness property, not a convenience*. Faking variable-length text as a fixed-capacity `bytes` buffer forced three pieces of syscall plumbing into every text contract:

| Hack it forced | Why it's wrong |
|---|---|
| `INPUT data bytes 65536` | arbitrary and wasteful — reserves 64 KB to hold `"Hi!"` |
| `INPUT length int 8` | the buffer doesn't carry how much is real, so you hand-count it every call |
| `OUTPUT bytes_written int 8` | leaks the syscall's return and smuggles failure in as a negative number |

None of that describes what the behavior *is*, and the hand-counting caused real bugs (a contract that passed `43` for 40 real bytes). The original analysis was right that string *operations* are behaviors and string *syntax* is sugar — but it missed that *variable-length-ness itself* needs a type to stay safe: a length-prefixed value carries its own size, so nothing runs off the end, which is exactly the guarantee the type system exists to provide.

So `string` became a first-class, self-describing, length-prefixed interpretation (`[ len : 8-byte LE ][ content ]`): O(1) length, binary-safe (any byte, including `0x00`), dynamically sized, with no separate length argument ever. Length-prefixing over a C-style null terminator is the modern consensus (Rust, Go, Swift, and Pascal all carry length); a `(pointer, length)` **slice** would be marginally better still (free substrings, no per-string prefix) but changes how values are represented at runtime, so it was deferred as not worth the larger core change. `bytes` stays for genuinely fixed buffers, so each contract is explicit about which it is. The full type and grammar specification is Reference §2.5.

---

## Part 4: Compiler Verification

The compiler enforces interpretation/operation consistency.

```
h_int = ALLOC(8, int)
h_float = ALLOC(8, float)
h_bytes = ALLOC(100, bytes)

// Valid
IADD(LOAD(h_int, 8), 5, 8)           // int op on int handle
FADD(LOAD(h_float, 8), 1.5, 8)       // float op on float handle
COPY(h_bytes, dest, 50)               // copy on bytes handle

// COMPILE ERROR
IADD(LOAD(h_float, 8), 5, 8)         // int op on float handle
FADD(LOAD(h_int, 8), 1.5, 8)         // float op on int handle
IMUL(LOAD(h_bytes, 8), 2, 8)         // arithmetic on bytes handle
```

---

## Part 5: Conversion Between Interpretations

Explicit conversion is required. No implicit coercion.

### Integer to Float

```
h_int = ALLOC(8, int)
h_float = ALLOC(8, float)

STORE(h_int, 42, 8)
int_val = LOAD(h_int, 8)

float_val = ITOF(int_val, 8, 8)      // Convert int to float
STORE(h_float, float_val, 8)
```

### Float to Integer

```
h_float = ALLOC(8, float)
h_int = ALLOC(8, int)

STORE(h_float, 3.7, 8)
float_val = LOAD(h_float, 8)

int_val = FTOI(float_val, 8, 8)      // Convert float to int (truncates)
STORE(h_int, int_val, 8)             // int_val is 3
```

### No Arithmetic Conversion for Bytes

`bytes` cannot be converted to `int` or `float` via ITOF/FTOI.

To interpret bytes as integer:
```
h_bytes = ALLOC(8, bytes)
h_int = ALLOC(8, int)

// Copy the raw bytes, then use as int
raw = LOAD(h_bytes, 8)
STORE(h_int, raw, 8)                 // Now can use int operations
```

This is explicit reinterpretation, not conversion.

---

## Part 6: External Data Mapping

When data comes from outside (database, network, file), the boundary behavior declares interpretation.

```
LEAF BEHAVIOR read_customer_age(db_conn, out):
  CONTRACT:
    INPUT:
      db_conn: handle(bytes)         // Raw connection handle
    OUTPUT:
      out: handle(int, 4)            // Output is 4-byte integer

  IMPLEMENTATION:
    raw = CALL db-read db_conn        // NATIVE behavior backed by the C runtime
    STORE(out, raw, 4)
```

The contract declares `out` is `int`. Compiler verifies all usage of `out` uses integer operations.

### Common External Mappings

| External Type | Our Interpretation | Size |
|---------------|-------------------|------|
| INT8, INT16, INT32, INT64 | `int` | 1, 2, 4, 8 |
| FLOAT, DOUBLE | `float` | 4, 8 |
| VARCHAR, BLOB, BINARY | `bytes` | Variable |
| BOOLEAN | `int` (0 or 1) | 1 |

---

## Part 7: What We Rejected

### Composite Types (Structs, Arrays)

**What it is:**
```cpp
struct Person {
    int age;
    char name[100];
};
Person people[10];
```

**Why considered:**
- Humans use structs to organize related data
- Arrays for indexed collections

**Why rejected:**

From AI's perspective:
```
Struct = bytes at offsets
  person.age     →  LOAD(person, 4, offset=0)
  person.name    →  LOAD(person, 100, offset=4)

Array = bytes with computed offset
  arr[i]         →  LOAD(arr, 4, offset=i*4)
```

AI doesn't need named fields or array syntax. AI can:
- Track that "bytes 0-3 are age (int)"
- Compute offsets directly

**Conclusion:** Composite types are human organization. AI uses `bytes` + offset arithmetic.

---

### Pointers

**What it is:**
```cpp
int* ptr = &value;
*ptr = 42;
Node* next = node->next;
```

**Why considered:**
- Dynamic data structures (linked lists, trees, graphs)
- Indirection

**Analysis from AI perspective:**

What do pointers provide?

| Need | With Pointers | With Pool + Index |
|------|---------------|-------------------|
| Indirection | Load address, load from address | Load index, compute offset, load |
| Dynamic structures | Nodes point to nodes | Nodes store indices |
| Sharing | Multiple pointers to same location | Multiple places store same index |

What risks do pointers introduce?

| Risk | With Pointers | With Pool + Index |
|------|---------------|-------------------|
| Forge invalid address | Yes | No |
| Dangling pointer | Yes | No |
| Buffer overflow | Yes | No (pool is bounded) |

**Why rejected:**

AI needs "reference to other data" - not raw addresses.

Pool + index pattern:
```
pool = ALLOC(1600, bytes)     // 100 nodes × 16 bytes

// "Pointer" is just index (0-99)
node_index = 5
node_offset = IMUL(node_index, 16, 8)
value = LOAD(pool, 8, offset=node_offset)

// "next pointer" is stored as integer index
next_index = LOAD(pool, 8, offset=node_offset+8)
```

This achieves:
- Indirection ✓
- Dynamic structures ✓
- Sharing ✓

Without:
- Address forgery risk
- Dangling pointers
- Breaking safety model

**Conclusion:** AI does not need pointers. Pool + index provides same functionality within safety model.

---

### Reference Counting / Shared Ownership Types

**What it is:**
```cpp
shared_ptr<Data> a = make_shared<Data>();
shared_ptr<Data> b = a;  // Both own it
```

**Why rejected:**

Already rejected in Memory Model. Single ownership + borrowing is sufficient.
Type system doesn't need special "shared" types.

---

## Part 8: Summary

### What We Have

| Interpretation | Purpose | Valid Operations |
|----------------|---------|------------------|
| `int` | Whole numbers | Integer arithmetic, comparison, bitwise |
| `float` | Decimal numbers | Float arithmetic, comparison |
| `bytes` | Raw data | LOAD, STORE, COPY only |
| `string` | Variable-length text/data | Self-describing (length-prefixed); LEN + string behaviors |

### What We Rejected

| Concept | Reason |
|---------|--------|
| Composite types | AI uses bytes + offsets |
| Pointers | Pool + index pattern |
| Shared types | Single ownership model |

### Declaration Syntax

```
// In ALLOC
h = ALLOC(size, interpretation)

h_age = ALLOC(4, int)
h_price = ALLOC(8, float)
h_buffer = ALLOC(1024, bytes)

// In contracts
CONTRACT:
  INPUT:
    data: handle(bytes, 1024)
  OUTPUT:
    result: handle(int, 4)
```

---

## Part 9: For AI Context

When AI works with data:

1. **Allocation declares interpretation**
   ```
   h = ALLOC(8, float)    // This is float data
   ```

2. **Compiler enforces consistency**
   ```
   FADD(LOAD(h, 8), 1.0, 8)    // OK
   IADD(LOAD(h, 8), 1, 8)      // COMPILE ERROR
   ```

3. **Contracts declare external data types**
   ```
   OUTPUT: result: handle(int, 4)
   ```

4. **Conversion is explicit**
   ```
   ITOF(int_val, 8, 8)    // int → float
   FTOI(float_val, 8, 8)  // float → int
   ```

This information is IN the code/contract, not just in AI's memory. Survives context loss.

---

*Document created during ideation session, January 2025*
*Four interpretations: int, float, bytes, string. Everything else is patterns and behaviors.*
