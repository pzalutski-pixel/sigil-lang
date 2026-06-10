# Sigil Language Reference

This is the authoritative reference for the Sigil grammar and semantics — the definition of the language that the compiler implements.

---

## 1. Notation and Lexical Elements

### 1.1 Grammar Notation

This document uses Extended Backus-Naur Form (EBNF):

```
production  = name "=" expression ";"
expression  = term { "|" term } ;
term        = factor { factor } ;
factor      = name | literal | "(" expression ")" | "[" expression "]" | "{" expression "}" ;
```

| Notation | Meaning |
|----------|---------|
| `name` | Non-terminal |
| `"text"` | Terminal (literal) |
| `[ x ]` | Optional (zero or one) |
| `{ x }` | Repetition (zero or more) |
| `x \| y` | Alternative |
| `( x )` | Grouping |

### 1.2 Source Format

Sigil uses line-based format. Each statement occupies one line.

```ebnf
source_file = { line } ;
line        = [ statement ] [ comment ] newline ;
comment     = "#" { any_char } ;
newline     = "\n" | "\r\n" ;
```

### 1.3 Characters

```ebnf
letter      = "A".."Z" | "a".."z" | "_" ;
digit       = "0".."9" ;
hex_digit   = digit | "A".."F" | "a".."f" ;
any_char    = (* any Unicode character except newline *) ;
```

### 1.4 Whitespace

Space characters separate tokens. Newlines terminate statements.

```ebnf
whitespace  = " " | "\t" ;
separator   = whitespace { whitespace } ;
```

### 1.5 Identifiers

```ebnf
identifier  = letter { letter | digit | "-" } ;
```

Identifiers name behaviors, handles, labels, and patterns.

**Examples:** `parse-request`, `h1`, `loop_start`, `my_buffer`

### 1.6 Keywords

Reserved words that cannot be used as identifiers:

**Structure:** `BEHAVIOR` `CONTRACT` `IMPLEMENTATION` `COMPOSITION` `NATIVE` `END` `PATTERN` `END_PATTERN` `MEMORY` `ON_CREATE` `ON_DESTROY` `LIBRARY` `END_LIBRARY` `EXECUTABLE` `END_EXECUTABLE` `ENTRY` `USES` `DESCRIPTION` `HASH`

**Contract:** `INPUT` `OUTPUT` `REQUIRES` `GUARANTEES`

**Guarantees:** `pure` `no_alloc` `writes_output`

**Interpretations:** `int` `float` `bytes` `string`

**Control:** `LABEL` `BRANCH` `JUMP` `SCOPE` `END_SCOPE`

**Composition:** `CALL` `SET` `DISCARD`

**Concurrency:** `SPAWN` `WAIT` `WAIT_ALL` `WAIT_ANY` `CHANNEL` `CHANNEL_SEND` `CHANNEL_RECEIVE` `CHANNEL_CLOSE` `SHARED`

### 1.7 Literals

```ebnf
integer_lit = [ "-" ] digit { digit } ;
hex_lit     = "0x" hex_digit { hex_digit } ;
string_lit  = '"' { string_char } '"' ;
string_char = (* any char except '"' and newline *) | escape ;
escape      = "\\" ( "n" | "r" | "t" | "\\" | '"' ) ;
```

**Examples:** `42`, `-17`, `0x1F`, `"Hello\n"`

### 1.8 Operators and Punctuation

| Symbol | Usage |
|--------|-------|
| `=` | Assignment |
| `->` | Output direction |
| `.` | Field access |
| `@` | Hash reference |
| `,` | Separator |

---

## 2. Types and Interpretations

Sigil has no traditional types. Data is bytes; operations interpret them.

### 2.1 Interpretations

An interpretation specifies how bytes are treated by operations.

```ebnf
interpretation = "int" | "float" | "bytes" | "string" ;
```

| Interpretation | Description | Valid Sizes (bytes) |
|----------------|-------------|---------------------|
| `int` | Signed two's complement integer | 1, 2, 4, 8 |
| `float` | IEEE 754 floating point | 4 (single), 8 (double) |
| `bytes` | Raw byte sequence (fixed capacity) | Any positive integer |
| `string` | Variable-length, self-describing text/data | none — see §2.5 |

### 2.2 Size

Size specifies the number of bytes.

```ebnf
size = integer_lit ;
```

**Constraints:**
- `int`: size must be 1, 2, 4, or 8
- `float`: size must be 4 or 8
- `bytes`: size must be positive integer
- `string`: no size — a `string` is self-describing (§2.5)

### 2.3 Interpretation Rules

1. Operations determine interpretation: `IADD` treats operands as int, `FADD` treats operands as float
2. Handles carry interpretation metadata assigned at allocation
3. Compiler verifies operations match handle interpretation
4. No implicit conversion; use `ITOF`/`FTOI` for explicit conversion

### 2.4 Valid Operations by Interpretation

| Interpretation | Valid Primitives |
|----------------|------------------|
| `int` | IADD, ISUB, IMUL, IDIV, IMOD, INEG, IEQ, INE, ILT, IGT, ILE, IGE, AND, OR, XOR, NOT, SHL, SHR, SAR, LOAD, STORE |
| `float` | FADD, FSUB, FMUL, FDIV, FNEG, FEQ, FNE, FLT, FGT, FLE, FGE, LOAD, STORE |
| `bytes` | LOAD, STORE |
| `string` | passed to `CALL`, bound, and written via `SET`; self-describing, no primitive arithmetic (§2.5) |

### 2.5 The `string` Type

A `string` is a variable-length, **self-describing** interpretation for text and other variable-length data, alongside `int` / `float` / `bytes`.

**Length-prefixed layout.** A `string` value is laid out as:

```
[ len : 8-byte little-endian ][ content bytes … ]
```

The handle points at byte 0; anything holding it reads the prefix to get the length. A string literal `"Hi!"` is 11 bytes: `[3]` followed by `H i !`. The length travels with the value, so no separate length argument is ever passed.

**No size in contracts.** Because a string carries its own length, a consumed string declares no size — `INPUT text string`, not `INPUT text string 256`. A size is required only where overflow is possible (an `ALLOC`, or writing into a fixed `bytes` buffer); when a behavior merely *receives* variable-length data, the size disappears.

**As an OUTPUT.** A `string` output occupies an 8-byte slot holding a *pointer* to the length-prefixed data; `SET out str` stores that pointer and reading the field (`result.out`) dereferences it. This is sound only while the source outlives the call. A string literal (a program global) always qualifies. A string may also be **constructed from computed bytes**: because a `string` is just length-prefixed bytes, `SET out buf` is allowed where `buf` is a `bytes` handle whose contents are a valid `[len][content]` image — `SET` stores the buffer's pointer. This is sound exactly when the caller owns `buf` so it outlives the returned string (e.g. a pattern's `MEMORY`, the same caller-ownership rule as collections); the stdlib `bytes-to-string` wraps this.

**`string` vs `bytes`.** `bytes N` remains for genuinely **fixed** buffers — a fixed-size record, a network packet, file read/write content — where an explicit byte count belongs. `string` is for variable-length text/data. Each contract is then explicit about which it is.

---

## 3. Memory Model

### 3.1 Handles

A handle is an opaque reference to allocated memory. Handles cannot be forged or fabricated.

```ebnf
handle = identifier ;
```

**Properties:**
- Returned by `ALLOC`
- Carries: base address, size, interpretation
- Non-overlapping: distinct handles reference distinct memory
- Opaque: no arithmetic on handles

### 3.2 Memory Lifetimes

| Lifetime | Created | Freed | Use Case |
|----------|---------|-------|----------|
| Scoped | `ALLOC` in `SCOPE` | `END_SCOPE` (automatic) | Temporary workspace |
| Pattern | `ALLOC` in `ON_CREATE` | `ON_DESTROY` | Persistent state |
| Borrowed | Passed as input | Caller's responsibility | Passing data |

### 3.3 Ownership

1. **Allocator owns**: The scope/pattern that executes `ALLOC` owns the handle
2. **Owner frees**: Only owner can `FREE`
3. **Borrowing**: Input handles are borrowed; callee cannot free them
4. **No transfer**: Ownership never transfers

### 3.4 Scoped Memory

```
SCOPE
  h = ALLOC 64 bytes
  # h valid here
END_SCOPE
# h freed automatically, invalid after this point
```

### 3.5 Memory Access

Memory is accessed via `LOAD` and `STORE` with optional offset.

```
value = LOAD handle size [offset]
STORE handle value size [offset]
```

**Constraint:** `offset + size <= handle.allocated_size`

---

## 4. Primitives

Primitives are the 43 atomic operations. All *pure* computation reduces to primitives; system effects are provided by NATIVE behaviors.

### 4.1 Memory Primitives

#### ALLOC

Allocates memory and returns a handle.

```
handle = ALLOC size interpretation
```

| Parameter | Type | Description |
|-----------|------|-------------|
| size | integer | Bytes to allocate |
| interpretation | keyword | int, float, or bytes |
| **Returns** | handle | Opaque reference to allocated memory |

**Semantics:** Allocates `size` bytes. Memory contents are undefined until written.

**Example:**
```
h = ALLOC 64 bytes
h_int = ALLOC 8 int
h_float = ALLOC 4 float
```

#### LOAD

Reads bytes from memory.

```
value = LOAD handle size [offset]
```

| Parameter | Type | Description |
|-----------|------|-------------|
| handle | handle | Memory to read from |
| size | integer | Bytes to read |
| offset | integer | Optional byte offset (default 0) |
| **Returns** | value | Data read |

**Precondition:** `offset + size <= handle.allocated_size`

**Precondition:** Memory at location must have been written (STORE) on all paths.

**Example:**
```
v = LOAD h 8          # Read 8 bytes from offset 0
v2 = LOAD h 4 16      # Read 4 bytes from offset 16
```

#### STORE

Writes bytes to memory.

```
STORE handle value size [offset]
```

| Parameter | Type | Description |
|-----------|------|-------------|
| handle | handle | Memory to write to |
| value | value | Data to write |
| size | integer | Bytes to write |
| offset | integer | Optional byte offset (default 0) |

**Precondition:** `offset + size <= handle.allocated_size`

**Example:**
```
STORE h 42 4          # Write 4 bytes at offset 0
STORE h v 8 24        # Write 8 bytes at offset 24
```

#### FREE

Releases allocated memory.

```
FREE handle
```

| Parameter | Type | Description |
|-----------|------|-------------|
| handle | handle | Memory to release |

**Precondition:** Caller must own the handle.

**Postcondition:** Handle is invalid; further use is a compile error.

**Example:**
```
FREE h
```

### 4.2 Atomic Primitives

For concurrent access to SHARED memory. All atomic operations are **sequentially consistent**.

#### ATOMIC_LOAD

Atomically reads from memory.

```
value = ATOMIC_LOAD handle size
```

**Semantics:** Read cannot be torn by concurrent writes.

#### ATOMIC_STORE

Atomically writes to memory.

```
ATOMIC_STORE handle value size
```

**Semantics:** Write cannot be torn by concurrent reads.

#### CAS

Compare-and-swap.

```
success = CAS handle expected new size
```

| Parameter | Type | Description |
|-----------|------|-------------|
| handle | handle | Memory location |
| expected | value | Expected current value |
| new | value | Value to write if match |
| size | integer | Bytes to compare/swap |
| **Returns** | int | 1 if swapped, 0 if not |

**Semantics:** Atomically: if `*handle == expected`, set `*handle = new` and return 1; else return 0.

#### ATOMIC_ADD

Atomically adds to memory.

```
old = ATOMIC_ADD handle value size
```

**Returns:** Value before addition.

#### ATOMIC_SUB

Atomically subtracts from memory.

```
old = ATOMIC_SUB handle value size
```

**Returns:** Value before subtraction.

### 4.3 Integer Arithmetic Primitives

All integer operations take signed two's complement values.

#### IADD

```
result = IADD a b size
```

**Semantics:** `result = a + b` (signed integer addition)

**Overflow:** Wraps (two's complement)

#### ISUB

```
result = ISUB a b size
```

**Semantics:** `result = a - b`

#### IMUL

```
result = IMUL a b size
```

**Semantics:** `result = a * b` (lower bits of product)

#### IDIV

```
result = IDIV a b size
```

**Semantics:** `result = a / b` (truncates toward zero)

**Precondition:** `b != 0`

**Edge case:** Division by zero behavior is undefined.

#### IMOD

```
result = IMOD a b size
```

**Semantics:** `result = a % b` (remainder after division toward zero)

**Precondition:** `b != 0`

#### INEG

```
result = INEG a size
```

**Semantics:** `result = -a`

### 4.4 Integer Comparison Primitives

All comparisons return 1 (true) or 0 (false).

#### IEQ

```
result = IEQ a b size
```

**Semantics:** `result = (a == b) ? 1 : 0`

#### INE

```
result = INE a b size
```

**Semantics:** `result = (a != b) ? 1 : 0`

#### ILT

```
result = ILT a b size
```

**Semantics:** `result = (a < b) ? 1 : 0` (signed comparison)

#### IGT

```
result = IGT a b size
```

**Semantics:** `result = (a > b) ? 1 : 0` (signed comparison)

#### ILE

```
result = ILE a b size
```

**Semantics:** `result = (a <= b) ? 1 : 0` (signed comparison)

#### IGE

```
result = IGE a b size
```

**Semantics:** `result = (a >= b) ? 1 : 0` (signed comparison)

### 4.5 Float Arithmetic Primitives

All float operations follow IEEE 754.

#### FADD

```
result = FADD a b size
```

**Semantics:** `result = a + b` (IEEE 754 addition)

#### FSUB

```
result = FSUB a b size
```

**Semantics:** `result = a - b`

#### FMUL

```
result = FMUL a b size
```

**Semantics:** `result = a * b`

#### FDIV

```
result = FDIV a b size
```

**Semantics:** `result = a / b`

**Edge case:** Division by zero produces infinity or NaN per IEEE 754.

#### FNEG

```
result = FNEG a size
```

**Semantics:** `result = -a`

### 4.6 Float Comparison Primitives

All comparisons return 1 (true) or 0 (false).

#### FEQ

```
result = FEQ a b size
```

**Note:** NaN != NaN (returns 0)

#### FNE

```
result = FNE a b size
```

#### FLT

```
result = FLT a b size
```

#### FGT

```
result = FGT a b size
```

#### FLE

```
result = FLE a b size
```

#### FGE

```
result = FGE a b size
```

### 4.7 Conversion Primitives

#### FTOI

Float to integer conversion.

```
int_val = FTOI float_val float_size int_size
```

**Semantics:** Truncates toward zero.

**Example:** (there are no float literals; load the value from a handle first)
```
f = LOAD h_float 8    # h_float holds 3.7
n = FTOI f 8 4        # n == 3
```

#### ITOF

Integer to float conversion.

```
float_val = ITOF int_val int_size float_size
```

**Note:** Large integers may lose precision.

### 4.8 Bitwise Primitives

#### AND

```
result = AND a b size
```

**Semantics:** Bitwise AND

#### OR

```
result = OR a b size
```

**Semantics:** Bitwise OR

#### XOR

```
result = XOR a b size
```

**Semantics:** Bitwise XOR

#### NOT

```
result = NOT a size
```

**Semantics:** Bitwise complement (flip all bits)

#### SHL

```
result = SHL a n size
```

**Semantics:** Shift left by `n` bits. Zeros fill from right.

#### SHR

```
result = SHR a n size
```

**Semantics:** Logical shift right by `n` bits. Zeros fill from left.

#### SAR

```
result = SAR a n size
```

**Semantics:** Arithmetic shift right by `n` bits. Sign bit fills from left.

### 4.9 Control Flow Primitives

#### BRANCH

Conditional jump.

```
BRANCH condition true_label false_label
```

**Semantics:** If `condition != 0`, jump to `true_label`. Else jump to `false_label`.

#### JUMP

Unconditional jump.

```
JUMP label
```

**Semantics:** Transfer control to `label`.

---

## 5. Contracts

A contract declares a behavior's interface.

### 5.1 Grammar

```ebnf
contract        = "CONTRACT" { contract_clause } ;
contract_clause = input | output | requires | memory_ref | guarantees ;
input           = "INPUT" identifier interpretation [ size ] ;
output          = "OUTPUT" identifier interpretation [ size ] ;
(* size is required for int/float/bytes; omitted for string (self-describing, §2.5) *)
requires        = "REQUIRES" { behavior_ref } ;
behavior_ref    = identifier "@" hex_string ;
memory_ref      = "MEMORY" { identifier } ;
guarantees      = "GUARANTEES" { guarantee } ;
guarantee       = "pure" | "no_alloc" | "writes_output" ;
```

### 5.2 INPUT

Declares an input parameter.

```
INPUT name interpretation size
```

**Semantics:** Caller provides a borrowed handle. Callee can read/write but not free. A `string` input declares no size — it is self-describing (§2.5).

**Example:**
```
INPUT request bytes 2048
INPUT customer_id int 8
INPUT path string
```

### 5.3 OUTPUT

Declares an output parameter.

```
OUTPUT name interpretation size
```

**Semantics:** Caller provides a handle. Callee writes result to it. A `string` output declares no size and is an 8-byte slot holding a pointer to length-prefixed data (§2.5).

**Example:**
```
OUTPUT response bytes 4096
OUTPUT status int 4
OUTPUT message string
```

### 5.4 REQUIRES

Declares behavior dependencies.

```
REQUIRES behavior@hash behavior@hash ...
```

**Semantics:** Listed behaviors must exist with matching contract hash.

**Example:**
```
REQUIRES parse-http@a1b2c3d4 format-json@d4e5f6a7
```

### 5.5 MEMORY

Declares pattern memory access.

```
MEMORY name name ...
```

**Semantics:** Behavior may access listed pattern memory handles.

### 5.6 GUARANTEES

Declares intended compiler-verified properties.

| Guarantee | Verification |
|-----------|---------------------|
| `pure` | No CHANNEL ops, no pattern MEMORY access, only writes to OUTPUT; and depends only on `pure` behaviors (transitive) |
| `no_alloc` | No ALLOC, or all ALLOC in SCOPE freed at END_SCOPE |
| `writes_output` | All OUTPUT handles written on all execution paths |

> **`writes_output` covers *every* path.** A write inside a loop body or a single branch is **not** sufficient on its own — the path where the loop runs zero times, or the branch is not taken, must also write the output. The idiom is to write a default value *before* the loop/branch, so the output is set on every path (otherwise `E0604`). This is a structural property of the control-flow graph, independent of how the control flow is spelled.

### 5.7 HASH

Contract hash for versioning and integrity verification.

```ebnf
hash = "HASH" hex_string ;
```

**Semantics:**

1. Let *data* be the empty byte sequence.
2. Append `"INPUTS:"` to *data*.
3. For each INPUT in declaration order:
   1. Append *name* as UTF-8 bytes.
   2. Append *type_byte* (see table below).
   3. Append *size* as 8-byte little-endian integer.
4. Append `"OUTPUTS:"` to *data*.
5. For each OUTPUT in declaration order:
   1. Append *name* as UTF-8 bytes.
   2. Append *type_byte*.
   3. Append *size* as 8-byte little-endian integer.
6. Append `"REQUIRES:"` to *data*.
7. For each behavior reference in declaration order:
   1. Append *name* as UTF-8 bytes.
   2. Append `"@"`.
   3. Append *hash* as UTF-8 bytes.
8. Append `"GUARANTEES:"` to *data*.
9. For each guarantee in declaration order:
   1. Append *guarantee_byte* (see table below).
10. Let *digest* be SHA-256(*data*).
11. Return first 4 bytes of *digest* as 8-character lowercase hex string.

| Interpretation | type_byte |
|----------------|-----------|
| `int` | 0x01 |
| `float` | 0x02 |
| `bytes` | 0x03 |
| `string` | 0x04 |

| Guarantee | guarantee_byte |
|-----------|----------------|
| `pure` | 0x01 |
| `no_alloc` | 0x02 |
| `writes_output` | 0x03 |

---

## 6. Behaviors

A behavior is the unit of computation.

### 6.1 Grammar

```ebnf
behavior     = "BEHAVIOR" identifier [ description ] contract "HASH" hex_string ( implementation | composition | native_decl ) ;
description  = "DESCRIPTION" { line } ;
implementation = "IMPLEMENTATION" { statement } "END" ;
composition  = "COMPOSITION" { comp_statement } "END" ;
native_decl  = "NATIVE" ;
```

### 6.2 Leaf Behavior (IMPLEMENTATION)

Contains primitives. Performs computation.

```
BEHAVIOR add-numbers

DESCRIPTION
  Adds two integers.

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
  STORE result sum 4
END
```

### 6.3 Composite Behavior (COMPOSITION)

Contains behavior calls. Wires behaviors together.

```
BEHAVIOR process-request

CONTRACT
  INPUT request bytes 2048
  OUTPUT response bytes 4096
  REQUIRES parse-http@a1b2c3d4 format-json@c3d4e5f6
  GUARANTEES writes_output

HASH e5f6a7b8

COMPOSITION
  parsed = CALL parse-http request
  CALL format-json parsed -> response
END
```

### 6.4 Native Behavior (NATIVE)

Declares interface for behavior implemented in native code (runtime library).

```
BEHAVIOR socket

CONTRACT
  INPUT domain int 8
  INPUT sock_type int 8
  OUTPUT fd int 8
  GUARANTEES writes_output

HASH f6b8d0e2

NATIVE
```

### 6.5 Hierarchy

```
Level 0: Primitives (built into compiler)
Level 1: Leaf behaviors (IMPLEMENTATION or NATIVE)
Level 2+: Composite behaviors (COMPOSITION with CALLs)
```

Implementation exists only at Level 1. All other levels are composition.

### 6.6 Leaf/Composite Separation (enforced)

The two kinds are strictly separated, and the compiler enforces it:

- A **composite** (`COMPOSITION`, and an executable's `ENTRY`) only *wires* behaviors: it may use `CALL`, `SET`, `BRANCH`/`JUMP`/`LABEL`, the concurrency operations, constant/field bindings (`x = 42`, `x = "text"`, `x = result.field`), and `DISCARD`. It may **not** compute or touch memory — no `ALLOC`/`FREE`/`STORE`/`LOAD`, no `SCOPE`, and no arithmetic/comparison/bitwise/atomic/conversion primitive. Violations are error `E0509`. To act on a result field, `BRANCH` on it directly (`BRANCH result.flag …`) rather than `LOAD`ing it.
- A **leaf** (`IMPLEMENTATION`) is the bottom of the call graph: it performs the actual computation from primitives and may **not** `CALL` another behavior. Violations are error `E0510`.

This makes the model's quarantine — algorithmic, can-be-wrong logic lives only in leaves, while composites are pure wiring whose mistakes are structural — a compiler-enforced rule rather than a convention. A `NATIVE` behavior has no body and is exempt.

---

## 7. Composition

### 7.1 CALL

Invokes a behavior.

```ebnf
call = [ identifier "=" ] "CALL" identifier { argument } [ "->" output_list ] ;
output_list = identifier { "," identifier } ;
```

**Forms:**
```
result = CALL behavior args...          # Result assigned to identifier
CALL behavior args -> output            # Result written to output handle
CALL behavior args -> out1, out2        # Multiple outputs
```

### 7.2 Field Access

Access structured output fields.

```
result = CALL parse-http request
CALL process result.method
CALL handle result.path
```

**Semantics:** Fields are offsets into output handle. Field structure determined by behavior contract.

### 7.3 SET

Assigns constant value.

```
SET handle value
```

**Example:**
```
SET status 200
SET error_flag 0
```

### 7.4 Labels

Defines jump target.

```
LABEL name
```

**Constraint:** Labels must be unique within behavior.

### 7.5 Control Flow in Composition

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

### 7.6 Loops

No explicit loop construct. Use BRANCH + JUMP:

```
COMPOSITION
  LABEL loop_start
    item = CALL next iterator
    BRANCH item.done loop_end loop_body

  LABEL loop_body
    CALL process item.value
    JUMP loop_start

  LABEL loop_end
END
```

### 7.7 DISCARD

Explicitly drops a produced output that is intentionally not used.

```
DISCARD result.status
DISCARD bind_result
```

**Semantics:** Marks the named output as *consumed* for the purposes of output-consumption checking (§7.8). `DISCARD result` drops every field of a CALL result; `DISCARD result.field` drops a single field. DISCARD generates no code — it records a deliberate decision to ignore a value, so that ignoring is explicit rather than silent.

Use DISCARD when a behavior is called for its effect and its result is deliberately unused.

### 7.8 Output Consumption

A COMPOSITION wires behaviors together: every value a CALL produces is an edge that must lead somewhere. An output produced inside a composition must be **consumed**. An output is consumed when it is:

- passed as an argument to another CALL,
- used as a BRANCH condition,
- written to one of the composition's own OUTPUTs (via SET or `->` output), or
- explicitly dropped with DISCARD (§7.7).

A produced output that is none of these leaves the composition structurally incomplete — a wire left unconnected — and is a compile error.

**Produced outputs.** For `result = CALL f ...`, each of `f`'s OUTPUTs is a produced output, reached as `result.<output-name>`. For `CALL f ... -> o1 o2`, each named output is a produced output. A bare `CALL f ...` (no `=`, no `->`) is therefore valid only when `f` has no outputs (a sink); otherwise its outputs are unconsumed.

Errors are ordinary outputs — a behavior that can fail declares an error OUTPUT beside its result — so an unhandled error is caught by this same rule: it must be branched on, propagated, or explicitly DISCARDed. This is the cross-behavior half of structural completeness (DESIGN-MODEL §5); the per-behavior half (every OUTPUT written on all paths) is the `writes_output` guarantee.

---

## 8. Patterns

A pattern is a unit with persistent memory.

### 8.1 Grammar

```ebnf
pattern      = "PATTERN" identifier [ memory_section ] [ on_create ] { behavior } [ on_destroy ] "END_PATTERN" ;
memory_section = "MEMORY" { memory_decl } ;
memory_decl  = identifier interpretation size ;
on_create    = "ON_CREATE" { statement } ;
on_destroy   = "ON_DESTROY" { statement } ;
```

### 8.2 Structure

```
PATTERN http-server

MEMORY
  cache bytes 1048576
  config bytes 256

ON_CREATE
  cache = ALLOC 1048576 bytes
  config = ALLOC 256 bytes
  # initialization...

BEHAVIOR handle-request
  CONTRACT
    INPUT request bytes 2048
    OUTPUT response bytes 4096
    MEMORY cache
    GUARANTEES writes_output
  HASH a1b2c3d4
  COMPOSITION
    # can access cache handle
    ...
  END

ON_DESTROY
  FREE cache
  FREE config

END_PATTERN
```

### 8.3 Lifecycle

1. `SPAWN` called
2. Pattern instance created
3. `ON_CREATE` executes
4. Pattern behaviors available
5. `ON_DESTROY` executes
6. Pattern memory freed
7. `WAIT` returns

---

## 9. Concurrency

Concurrency operations are runtime constructs, not CPU primitives.

### 9.1 SPAWN

Creates concurrent pattern instance.

```
handle = SPAWN pattern args
```

**Returns:** Handle to spawned instance.

### 9.2 WAIT

Waits for completion.

```
result = WAIT handle
```

**Blocks** until the spawned task completes. The bound value is a **completion status**, not data — a spawned pattern has no OUTPUT contract, so results are passed back through channels (§9.5), not through WAIT.

### 9.3 WAIT_ALL

Waits for all to complete.

```
r1, r2, r3 = WAIT_ALL h1 h2 h3
```

**With results:** Destructures results into separate identifiers. Count must match.

```
WAIT_ALL h1 h2 h3
```

**Without assignment:** Just waits, discards results.

### 9.4 WAIT_ANY

Waits for first to complete.

```
result, which = WAIT_ANY h1 h2 h3
```

**Returns:** Result and which handle completed.

### 9.5 CHANNEL

Creates communication channel.

```
ch = CHANNEL interpretation capacity
```

| Parameter | Description |
|-----------|-------------|
| interpretation | Type of values |
| capacity | Buffer size |

### 9.6 CHANNEL_SEND

Sends value to channel.

```
CHANNEL_SEND ch value
```

**Blocks** if channel is full.

### 9.7 CHANNEL_RECEIVE

Receives value from channel.

```
value = CHANNEL_RECEIVE ch
```

**Blocks** if channel is empty. Returns immediately with closed flag if channel is closed.

### 9.8 CHANNEL_CLOSE

Closes a channel, signaling no more values will be sent.

```
CHANNEL_CLOSE ch
```

**Semantics:**
- Subsequent CHANNEL_SEND on closed channel is a compile error
- CHANNEL_RECEIVE on closed empty channel returns immediately with closed flag
- Closing is idempotent

### 9.9 SHARED Memory

```
shared = ALLOC size interpretation SHARED
```

**Constraint:** SHARED handles require atomic operations. Non-atomic LOAD/STORE on a SHARED handle is a compile error (`NonAtomicSharedAccess`); use `ATOMIC_LOAD`/`ATOMIC_STORE`.

---

## 10. Scoped Memory

### 10.1 Grammar

```ebnf
scope = "SCOPE" { statement } "END_SCOPE" ;
```

### 10.2 Semantics

```
SCOPE
  h = ALLOC 64 bytes
  # use h...
END_SCOPE
# h automatically freed
```

All handles allocated in SCOPE are freed at END_SCOPE.

---

## 11. Library and Executable

### 11.1 Library

```
LIBRARY name

BEHAVIOR behavior1
  ...
END

BEHAVIOR behavior2
  ...
END

END_LIBRARY
```

### 11.2 Executable

```
EXECUTABLE name

DESCRIPTION
  text

USES pattern.pattern

ENTRY
  # entry point statements
END_EXECUTABLE
```

---

## 12. Files

| Extension | Contents | Structure |
|-----------|----------|-----------|
| `.beh` | Single behavior | One BEHAVIOR per file |
| `.pattern` | Pattern definition | PATTERN...END_PATTERN |
| `.sigil` | Entry point | EXECUTABLE...END_EXECUTABLE |

---

## 13. Validity Rules

### 13.1 Memory Validity

| Rule | Constraint |
|------|------------|
| Bounds | `offset + size <= allocated_size` |
| Lifetime | Handle valid only within owning scope/pattern |
| Initialization | LOAD requires prior STORE on all paths |
| Ownership | Only owner can FREE |
| No leaks | Every ALLOC has FREE or is scoped |
| No aliasing | Handles don't overlap unless SHARED |

### 13.2 Type Validity

| Rule | Constraint |
|------|------------|
| Interpretation match | Operation family matches handle interpretation |
| Explicit conversion | ITOF/FTOI required |
| Size match | Operation size matches operand size |

### 13.3 Contract Validity

| Rule | Constraint |
|------|------------|
| Input match | Caller provides correct interpretation/size |
| Output match | Callee writes correct interpretation/size |
| Requires satisfied | Dependencies exist with matching hash |
| Hash match | Declared hash equals computed hash |

### 13.4 Control Flow Validity

| Rule | Constraint |
|------|------------|
| Labels exist | BRANCH/JUMP targets defined |
| Unique labels | No duplicates in behavior |
| Scopes balanced | SCOPE has END_SCOPE |
| Paths terminate | All paths reach END or loop |

### 13.5 Concurrency Validity

| Rule | Constraint |
|------|------------|
| Atomic on SHARED | Non-atomic access forbidden (`NonAtomicSharedAccess`) |
| Channel types | Send/receive matches declaration |
| Borrow across spawn | Borrowed handle valid for spawn lifetime (`FreeWhileBorrowed`) |

### 13.6 Guarantee Validity

| Guarantee | Violation |
|-----------|-----------|
| `pure` | Pattern MEMORY access, CHANNEL ops, non-OUTPUT writes |
| `no_alloc` | ALLOC escaping scope |
| `writes_output` | Path without OUTPUT write (via STORE or SET) |

### 13.7 Composition Validity

| Rule | Constraint |
|------|------------|
| Output consumption | Every output a CALL produces is consumed — passed to a CALL, used as a BRANCH condition, written to an OUTPUT, or dropped with DISCARD (§7.8) |

### 13.8 Behavior Structure Validity

| Rule | Constraint |
|------|------------|
| Composite is wiring-only | A `COMPOSITION` / `ENTRY` may not compute or touch memory — `E0509` (§6.6) |
| Leaf does not call | An `IMPLEMENTATION` may not `CALL` another behavior — `E0510` (§6.6) |

---

## Appendix A: Complete Grammar

```ebnf
(* Top-level *)
source_file     = { behavior | pattern | library | executable } ;

(* Behavior *)
behavior        = "BEHAVIOR" identifier [ description ] contract hash ( implementation { implementation } | composition | native_decl ) ;
native_decl     = "NATIVE" ;
description     = "DESCRIPTION" { text_line } ;
contract        = "CONTRACT" { contract_clause } ;
contract_clause = input | output | requires | memory_ref | guarantees ;
input           = "INPUT" identifier interpretation [ size ] ;
output          = "OUTPUT" identifier interpretation [ size ] ;
(* size is required for int/float/bytes; omitted for string (self-describing, §2.5) *)
requires        = "REQUIRES" { behavior_ref } ;
behavior_ref    = identifier "@" hex_string ;
memory_ref      = "MEMORY" { identifier } ;
guarantees      = "GUARANTEES" { guarantee } ;
guarantee       = "pure" | "no_alloc" | "writes_output" ;
hash            = "HASH" hex_string ;
implementation  = "IMPLEMENTATION" { impl_statement } "END" ;
composition     = "COMPOSITION" { comp_statement } "END" ;

(* Implementation statements *)
impl_statement  = assignment | primitive_call | label | branch | jump | scope ;
assignment      = identifier "=" primitive_call ;
primitive_call  = primitive { argument } ;
primitive       = "ALLOC" | "LOAD" | "STORE" | "FREE"
                | "ATOMIC_LOAD" | "ATOMIC_STORE" | "CAS" | "ATOMIC_ADD" | "ATOMIC_SUB"
                | "IADD" | "ISUB" | "IMUL" | "IDIV" | "IMOD" | "INEG"
                | "IEQ" | "INE" | "ILT" | "IGT" | "ILE" | "IGE"
                | "FADD" | "FSUB" | "FMUL" | "FDIV" | "FNEG"
                | "FEQ" | "FNE" | "FLT" | "FGT" | "FLE" | "FGE"
                | "FTOI" | "ITOF"
                | "AND" | "OR" | "XOR" | "NOT" | "SHL" | "SHR" | "SAR" ;
label           = "LABEL" identifier ;
branch          = "BRANCH" value identifier identifier ;
jump            = "JUMP" identifier ;
scope           = "SCOPE" { impl_statement } "END_SCOPE" ;

(* Composition statements *)
comp_statement  = call | set | bind | discard | label | branch | jump | concurrency_stmt ;
call            = [ identifier "=" ] "CALL" identifier { argument } [ "->" output_list ] ;
output_list     = identifier { "," identifier } ;
set             = "SET" identifier value ;
bind            = identifier "=" value ;   (* plain composition binding: msg = "Hi!", fd = result.field *)
discard         = "DISCARD" ( field_access | identifier ) ;

(* Concurrency statements *)
concurrency_stmt = spawn | wait | wait_all | wait_any | channel | channel_send | channel_receive | channel_close ;
spawn            = identifier "=" "SPAWN" identifier { argument } ;
wait             = identifier "=" "WAIT" identifier ;
wait_all         = [ identifier { "," identifier } "=" ] "WAIT_ALL" identifier { identifier } ;
wait_any         = identifier "," identifier "=" "WAIT_ANY" { identifier } ;
channel          = identifier "=" "CHANNEL" interpretation capacity ;
channel_send     = "CHANNEL_SEND" identifier value ;
channel_receive  = identifier "=" "CHANNEL_RECEIVE" identifier ;
channel_close    = "CHANNEL_CLOSE" identifier ;

(* Pattern *)
pattern         = "PATTERN" identifier [ memory_section ] [ on_create ] { behavior } [ on_destroy ] "END_PATTERN" ;
memory_section  = "MEMORY" { memory_decl } ;
memory_decl     = identifier interpretation size ;
on_create       = "ON_CREATE" { pattern_statement } ;
on_destroy      = "ON_DESTROY" { pattern_statement } ;
pattern_statement = impl_statement | call | concurrency_stmt ;

(* Library *)
library         = "LIBRARY" identifier { behavior } "END_LIBRARY" ;

(* Executable *)
executable      = "EXECUTABLE" identifier [ description ] [ uses ] entry "END_EXECUTABLE" ;
uses            = "USES" identifier ".pattern" ;
entry           = "ENTRY" { comp_statement } ;

(* Common *)
interpretation  = "int" | "float" | "bytes" | "string" ;
size            = integer_lit ;
value           = identifier | integer_lit | hex_lit | string_lit | field_access ;
field_access    = identifier "." identifier ;
argument        = value | interpretation | size ;
identifier      = letter { letter | digit | "-" } ;
hex_string      = hex_digit { hex_digit } ;
integer_lit     = [ "-" ] digit { digit } ;
hex_lit         = "0x" hex_digit { hex_digit } ;
letter          = "A".."Z" | "a".."z" | "_" ;
digit           = "0".."9" ;
hex_digit       = digit | "A".."F" | "a".."f" ;
```

---

## Appendix B: Primitive Summary

| Category | Count | Primitives |
|----------|-------|------------|
| Memory | 4 | ALLOC, LOAD, STORE, FREE |
| Atomic | 5 | ATOMIC_LOAD, ATOMIC_STORE, CAS, ATOMIC_ADD, ATOMIC_SUB |
| Integer | 12 | IADD, ISUB, IMUL, IDIV, IMOD, INEG, IEQ, INE, ILT, IGT, ILE, IGE |
| Float | 11 | FADD, FSUB, FMUL, FDIV, FNEG, FEQ, FNE, FLT, FGT, FLE, FGE |
| Conversion | 2 | FTOI, ITOF |
| Bitwise | 7 | AND, OR, XOR, NOT, SHL, SHR, SAR |
| Control | 2 | BRANCH, JUMP |
| **Total** | **43** | |

---

## Appendix C: All Keywords

```
ALLOC AND ATOMIC_ADD ATOMIC_LOAD ATOMIC_STORE ATOMIC_SUB
BEHAVIOR BRANCH bytes
CALL CAS CHANNEL CHANNEL_CLOSE CHANNEL_RECEIVE CHANNEL_SEND
  COMPOSITION CONTRACT
DESCRIPTION DISCARD
END END_EXECUTABLE END_LIBRARY END_PATTERN END_SCOPE ENTRY EXECUTABLE
FADD FDIV FEQ FGE FGT FLE FLT FMUL FNE FNEG FREE FSUB FTOI float
GUARANTEES
HASH
IADD IDIV IEQ IGE IGT ILE ILT IMOD IMPLEMENTATION IMUL INE INEG
  INPUT int ISUB ITOF
JUMP
LABEL LIBRARY LOAD
MEMORY
NATIVE NOT no_alloc
ON_CREATE ON_DESTROY OR OUTPUT
PATTERN pure
REQUIRES
SAR SCOPE SET SHARED SHL SHR SPAWN STORE string
USES
WAIT WAIT_ALL WAIT_ANY writes_output
XOR
```
