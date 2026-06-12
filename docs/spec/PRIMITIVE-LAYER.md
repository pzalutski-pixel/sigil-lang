# Primitive Layer Definition

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements. (The 43 primitives are implemented.)

## What Are Primitives?

Primitives are the lowest-level operations of the language — the atoms from which all behaviors are composed. (They are a deliberately ISA-flavored set chosen for directness, not a *minimal* basis: some, like the full family of integer comparisons, are conveniences derivable from others.)

**Key principle:** Primitives are the ONLY things that "do work." Everything else is composition.

```
PRIMITIVES (Level 0)
    ↓
LEAF BEHAVIORS (Level 1) - graphs of primitives
    ↓
COMPOSITE BEHAVIORS (Level 2+) - wiring of behaviors
```

---

## Design Decisions and Reasoning

### Decision 1: Data Is Bytes, Operations Are Typed

**The question:** Do we need data types (int, float, string)?

**The insight:** At the machine level, memory is just bytes. The CPU doesn't know if `0x40490FDB` is:
- The float `3.14159...`
- The integer `1078530011`
- Part of a string
- Machine code

It's just bytes.

**Types exist because humans forget.** You write `int x` so that 6 months later you remember what `x` holds. AI doesn't forget - it can track internally that "bytes at address X came from a float column."

**However, operations ARE typed.** The CPU has different instructions for:
- Integer add (`IADD`)
- Float add (`FADD`)

The operation interprets the bytes.

**Conclusion:**
- Data has SIZE (number of bytes), not TYPE
- Operations have TYPE (integer vs float vs bitwise)
- AI tracks semantics internally
- Compiler verifies operations match intended interpretation

### Decision 2: Sizes Are Explicit

**The question:** What sizes do we support?

**The reality:** CPUs work with standard sizes:
- 1 byte (8 bits)
- 2 bytes (16 bits)
- 4 bytes (32 bits)
- 8 bytes (64 bits)

**For integers:** All four sizes are common (char, short, int, long).

**For floats:** Two sizes are standard:
- 4 bytes (float32, single precision)
- 8 bytes (float64, double precision)

**Conclusion:** Primitives take explicit size parameters where relevant.

### Decision 3: Conversion Primitives Are Necessary

**The question:** Do we need type conversion operations?

**Two kinds of "conversion":**

1. **Semantic conversion** (string "123" → integer 123)
   - This is PARSING - a behavior, not a primitive
   - Built from loading bytes, interpreting as digits, computing value
   - Not a CPU instruction

2. **Representation conversion** (float bits → integer bits)
   - This IS a CPU instruction
   - FTOI: Interpret float, produce integer (loses precision)
   - ITOF: Interpret integer, produce float

**We considered:** Could everything be float?
- Float64 can exactly represent integers up to 2^53
- After that, precision loss
- For addresses, indices, counters - wasteful and dangerous

**Conclusion:** Both integer and float operations needed. FTOI/ITOF primitives needed for conversion.

### Decision 4: Atomics Are Primitives

**The question:** Do we need atomic operations at the primitive level?

**The answer:** Absolutely yes.

**Reasoning:** Concurrency requires operations that cannot be interrupted:
- Reading a value another thread might change
- Writing a value another thread might read
- Compare-and-swap for lock-free algorithms

Without atomics as primitives, you cannot build:
- Semaphores
- Mutexes
- Lock-free data structures
- Thread-safe counters

**Conclusion:** Atomic operations are primitives, not behaviors.

> **Status note.** The atomic primitives are implemented (lowered to LLVM atomic instructions),
> and the concurrency runtime they serve **works**: SPAWN/WAIT/CHANNEL run on a green-thread
> worker pool over real OS threads, and the concurrency examples build and run. The compiler
> also **enforces** atomic access to `SHARED` memory — a non-atomic LOAD/STORE on a SHARED
> handle is a compile error — and rejects freeing a handle a live SPAWN still borrows. What's
> not yet built is *automatic parallelization* (independent calls run sequentially). See
> [STATUS.md](../STATUS.md).

### Decision 5: SYSCALL Is Raw, Portability Is Behavior-Level

> **IMPLEMENTATION NOTE (January 2025):** This decision was ultimately **REJECTED**. SYSCALL was removed from the grammar entirely. See end of this section for rationale.

**The question:** Should syscalls be raw numbers or named abstractions?

**The tension:**
- Raw numbers (`SYSCALL(1, ...)`) - minimal, but OS-specific and unverifiable
- Named abstractions (`SYS_WRITE(...)`) - portable, but adds a layer

**The insight:** Portability is not the primitive layer's concern.

We looked at how C/C++ handles this:
```
Your code:        fwrite(buf, size, count, file)
                           ↓
Stdlib layer:     (implementation hidden)
                           ↓
Linux:            syscall(1, fd, buf, len)
macOS:            syscall(4, fd, buf, len)
Windows:          WriteFile(handle, buf, len, ...)
```

The standard library provides the abstraction. Not the language primitives.

**Our architecture already has a place for this: Leaf Behaviors.**

```
PRIMITIVE (raw, universal):
  SYSCALL(num, args...) → result

LEAF BEHAVIOR (where portability lives):
  write-file:
    CONTRACT: INPUT(fd, buf, len) OUTPUT(bytes_written)
    IMPLEMENTATION[linux]: SYSCALL(1, fd, buf, len)
    IMPLEMENTATION[macos]: SYSCALL(4, fd, buf, len)
    IMPLEMENTATION[windows]: (Win32 API calls)

COMPOSITE BEHAVIOR (portable by default):
  Uses "write-file", doesn't know about syscalls.
```

**The elegance:**
- Primitives stay pure and minimal
- Platform complexity is contained in leaf behaviors only
- Composite behaviors are automatically portable
- No abstraction pollution at primitive level

**Conclusion:** SYSCALL is raw. Portability is a behavior-level concern.

---

**Addendum (January 2025): SYSCALL Rejected, NATIVE Behaviors Implemented Instead**

The architecture shown above (SYSCALL primitive with platform-specific IMPLEMENTATION blocks) was the original design. During implementation, the entire SYSCALL approach was rejected:

**Why SYSCALL was rejected entirely:**
1. **AI doesn't need syscall numbers** - Sigil is AI-native; requiring AI to memorize OS-specific syscall numbers (Linux write=1, macOS write=4, Windows uses Win32 API) defeats the language's purpose
2. **Sigil code contains OS details** - Syscall numbers exposed in language layer breaks portability
3. **Code duplication** - Same logic repeated with only syscall numbers differing across platforms
4. **Maintenance burden** - Adding platform support requires editing Sigil files
5. **CAPABILITIES complexity** - Required compiler to track which syscalls were authorized per behavior

**Current Architecture: NATIVE + Runtime**

Stdlib behaviors now use NATIVE declaration (note: CAPABILITIES also removed):
```
BEHAVIOR write
CONTRACT
  INPUT fd int 8
  INPUT data bytes 65536
  INPUT length int 8
  OUTPUT bytes_written int 8
  GUARANTEES writes_output
HASH a1b2c3d4
NATIVE
```

The runtime library (C code in `runtime/src/`) provides implementations:
```c
// runtime/src/file.c
void sigil_write(int64_t* fd, uint8_t* data, int64_t* length, int64_t* bytes_written) {
    #ifdef _WIN32
        // Windows implementation
    #else
        // POSIX implementation
    #endif
}
```

**Benefits:**
- Sigil code defines contracts only (clean separation)
- Platform handling in C where it's natural
- Single point of maintenance per behavior
- Runtime can be compiled separately per platform

**The original vision was correct about hierarchy:**
- Primitives stay pure and minimal ✓
- Platform complexity contained at leaf level ✓
- Composite behaviors portable by default ✓

**What changed:** The "leaf level" for platform code is the runtime (C), not Sigil IMPLEMENTATION blocks. NATIVE is the bridge.

**CAPABILITIES also removed:** The original spec required behaviors using SYSCALL to declare CAPABILITIES (e.g., `CAPABILITIES syscall:write`). With SYSCALL removed, CAPABILITIES became unnecessary - NATIVE behaviors are trusted to do what their contract says, verified by testing rather than static capability tracking.

**no_syscall guarantee removed:** The `no_syscall` guarantee (promising a behavior makes no system calls) was also removed. Without SYSCALL in the language, this guarantee is meaningless - all system interaction goes through NATIVE behaviors, which are at the stdlib level, not user code.

**Primitive count is now 43** (was 44 with SYSCALL).

---

### Decision 6: SIMD Is Not a Primitive (For Now)

**The question:** Do we need vector/SIMD operations?

**SIMD (Single Instruction Multiple Data):** Process multiple values in parallel using wide registers (128/256/512 bits).

**Our reasoning:**
- SIMD is an optimization, not a semantic difference
- "Add these 1000 numbers" means the same whether done with scalar or vector ops
- Compilers (LLVM) are good at auto-vectorization
- Exposing SIMD is premature optimization

**Conclusion:** Leave SIMD out for now. Compiler can optimize. Can add as extension later if needed.

---

## The Complete Primitive Set

### Memory Operations

```
LOAD (addr, size) → value
```
Read `size` bytes from memory address `addr`. Returns the value.

```
STORE (addr, value, size)
```
Write `value` to memory address `addr`. Writes `size` bytes.

```
ALLOC (size) → handle
```
Request `size` bytes of memory. Returns opaque handle.
Handle guarantees non-overlapping allocation.

```
FREE (handle)
```
Release memory associated with handle.

### Atomic Memory Operations

```
ATOMIC_LOAD (addr, size) → value
```
Read that cannot be torn (interrupted mid-read by another thread).

```
ATOMIC_STORE (addr, value, size)
```
Write that cannot be torn.

```
CAS (addr, expected, new, size) → success
```
Compare-And-Swap. If value at `addr` equals `expected`, replace with `new`.
Returns 1 if swap happened, 0 if not.
Fundamental building block for lock-free algorithms.

```
ATOMIC_ADD (addr, value, size) → old_value
```
Atomically add `value` to memory at `addr`. Returns value before addition.

```
ATOMIC_SUB (addr, value, size) → old_value
```
Atomically subtract `value` from memory at `addr`. Returns value before subtraction.

### Integer Operations

All integer operations take a size parameter (1, 2, 4, or 8 bytes).

**Arithmetic:**
```
IADD (a, b, size) → result      Integer addition
ISUB (a, b, size) → result      Integer subtraction
IMUL (a, b, size) → result      Integer multiplication
IDIV (a, b, size) → result      Integer division (truncates toward zero)
IMOD (a, b, size) → result      Integer modulo
INEG (a, size) → result         Integer negation
```

**Comparison (return 0 or 1):**
```
IEQ (a, b, size) → 0|1          Equal
INE (a, b, size) → 0|1          Not equal
ILT (a, b, size) → 0|1          Less than (signed)
IGT (a, b, size) → 0|1          Greater than (signed)
ILE (a, b, size) → 0|1          Less than or equal (signed)
IGE (a, b, size) → 0|1          Greater than or equal (signed)
```

Note: Unsigned comparisons may be added if needed (ILTU, IGTU, etc.)

### Float Operations

Float operations work on 4-byte (float32) or 8-byte (float64) values.

**Arithmetic:**
```
FADD (a, b, size) → result      Float addition
FSUB (a, b, size) → result      Float subtraction
FMUL (a, b, size) → result      Float multiplication
FDIV (a, b, size) → result      Float division
FNEG (a, size) → result         Float negation
```

**Comparison (return 0 or 1):**
```
FEQ (a, b, size) → 0|1          Equal
FNE (a, b, size) → 0|1          Not equal
FLT (a, b, size) → 0|1          Less than
FGT (a, b, size) → 0|1          Greater than
FLE (a, b, size) → 0|1          Less than or equal
FGE (a, b, size) → 0|1          Greater than or equal
```

### Conversion Operations

```
FTOI (float_val, float_size, int_size) → int_val
```
Convert float to integer. Truncates toward zero.

```
ITOF (int_val, int_size, float_size) → float_val
```
Convert integer to float. May lose precision for large integers.

### Bitwise Operations

```
AND (a, b, size) → result       Bitwise AND
OR  (a, b, size) → result       Bitwise OR
XOR (a, b, size) → result       Bitwise XOR
NOT (a, size) → result          Bitwise NOT (complement)
SHL (a, n, size) → result       Shift left by n bits
SHR (a, n, size) → result       Shift right by n bits (logical, zero-fill)
SAR (a, n, size) → result       Shift right arithmetic (sign-preserving)
```

### Control Flow

```
BRANCH (cond, target_true, target_false)
```
If `cond` is non-zero, jump to `target_true`. Otherwise jump to `target_false`.
This is the ONLY conditional primitive. All other control flow builds from this.

```
JUMP (target)
```
Unconditional jump to `target`.

### System

> **REJECTED (January 2025):** SYSCALL was removed from the implemented grammar. System interactions use NATIVE behaviors backed by C runtime instead. See Decision 5 addendum above.

```
SYSCALL (num, args...) → result
```
Raw kernel system call. `num` is the syscall number (OS-specific).
`args` are passed to the kernel. Returns kernel's result.

**Used only in leaf behaviors.** Composite behaviors never see this directly.

---

## What Is NOT a Primitive

**Not primitives (built as behaviors instead):**

- String operations (parsing, concatenation, comparison)
- File operations (open, read, write, close) - leaf behaviors over NATIVE behaviors backed by the C runtime
- Network operations - leaf behaviors over NATIVE behaviors backed by the C runtime
- Memory copy - behavior using LOAD/STORE
- Array operations - behaviors using LOAD/STORE with computed addresses

**Not primitives (compiler optimization):**

- SIMD/vector operations - compiler auto-vectorizes
- Register allocation - compiler handles
- Instruction scheduling - compiler handles

---

## Primitive Count Summary

| Category | Count | Primitives |
|----------|-------|------------|
| Memory | 4 | LOAD, STORE, ALLOC, FREE |
| Atomic | 5 | ATOMIC_LOAD, ATOMIC_STORE, CAS, ATOMIC_ADD, ATOMIC_SUB |
| Integer | 12 | IADD, ISUB, IMUL, IDIV, IMOD, INEG, IEQ, INE, ILT, IGT, ILE, IGE |
| Float | 11 | FADD, FSUB, FMUL, FDIV, FNEG, FEQ, FNE, FLT, FGT, FLE, FGE |
| Conversion | 2 | FTOI, ITOF |
| Bitwise | 7 | AND, OR, XOR, NOT, SHL, SHR, SAR |
| Control | 2 | BRANCH, JUMP |
| System | 1 | ~~SYSCALL~~ (rejected) |
| **Total** | **44** | **(43 as implemented)** |

44 primitives originally specified. **43 as implemented** (SYSCALL rejected). Everything else is composition.

---

## Relationship to Other Layers

```
┌─────────────────────────────────────────────────────────────┐
│                 COMPOSITE BEHAVIORS                         │
│                                                             │
│  Portable. Compose leaf behaviors.                          │
│  Never see SYSCALL or OS-specific details.                  │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ uses
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                   LEAF BEHAVIORS                            │
│                                                             │
│  Graphs of primitives, plus NATIVE behaviors.               │
│  System access is via NATIVE behaviors whose                │
│  implementations live in the C runtime (runtime/src/).      │
│  "write" is a NATIVE behavior; the C runtime handles         │
│  per-platform details (Windows, Linux, macOS validated).    │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ uses
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                     PRIMITIVES                              │
│                                                             │
│  43 operations. Universal. Minimal.                         │
│  The only things that "do work."                            │
│  This document.                                             │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ compiles to
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    NATIVE CODE                              │
│                                                             │
│  x86-64 and arm64 machine instructions today (the LLVM      │
│  backend targets the host; cross-targets are not built).    │
└─────────────────────────────────────────────────────────────┘
```

---

## Open Questions for Future Consideration

1. **Unsigned operations:** Do we need ILTU, IGTU for unsigned comparisons? Or is explicit sign-extension enough?

2. **Extended precision:** Do we need 128-bit integers or float128 for specific domains?

3. **Memory ordering:** For atomics, do we need memory fence/barrier primitives? (Current assumption: sequential consistency)

4. **Exception handling:** How do we handle divide-by-zero, overflow, invalid float ops? Trap? Return special value?

5. **Endianness:** Do we assume little-endian? Make it explicit? Let compiler handle?

These can be resolved as implementation progresses.

---

*Document created during ideation session, January 2025*
*43 primitives. Everything else is composition.*
