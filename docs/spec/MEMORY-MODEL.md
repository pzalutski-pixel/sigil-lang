# Memory Model

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements. (Handle-based memory and scoped allocation are implemented.)

## Overview

This document defines how memory works in the AI-native language: how it's allocated, owned, accessed, and freed. The model is designed for compile-time safety verification while supporting all real-world use cases.

---

## Part 1: The Core Problem

Traditional languages give humans tools to manage memory:
- Variables with names
- Stack frames from functions
- Heap allocation with manual free or garbage collection
- Smart pointers for automatic cleanup

Our language has no variables, no functions in the traditional sense. We need a memory model that:
1. AI can reason about locally (limited context)
2. Compiler can verify completely (safety)
3. Supports all real-world patterns (practical)

---

## Part 2: Handles Replace Variables

### The Problem

Without named variables, how does AI track "this memory is X, that memory is Y"?

### Options Considered

**Option A: Explicit addresses**
```
AI manually picks addresses:
  STORE(0x1000, value_a, 8)
  STORE(0x1008, value_b, 8)
```
- Problem: AI can accidentally create overlapping addresses
- Problem: AI can forge invalid addresses
- Problem: No bounds checking possible
- **Rejected**

**Option B: Opaque handles**
```
h1 = ALLOC(8)    // Returns opaque handle
h2 = ALLOC(8)    // Returns different handle

STORE(h1, value_a, 8)
STORE(h2, value_b, 8)
```
- Handle contains: {id, size, internal_addr}
- AI cannot see or manipulate the internal address
- System guarantees non-overlapping allocations
- Compiler can verify bounds
- **Accepted**

### Indexed Access

For array-like access, handles support offset:
```
arr = ALLOC(100)              // 100 bytes
STORE(arr, value, 4, offset=0)   // First element
STORE(arr, value, 4, offset=4)   // Second element
LOAD(arr, 4, offset=40)          // 11th element
```

Offset is explicit. Compiler verifies: offset + size <= handle.size

### Conclusion

**Handles are opaque identifiers. AI tracks them, cannot forge addresses. Compiler verifies all access is within bounds.**

---

## Part 3: Scoped Memory (Temporary)

### The Problem

Functions provide stack frames - automatic allocation and cleanup. Without functions, what provides this?

### Solution: Pattern Scopes

```
BEHAVIOR do_something(input, output):
  SCOPE
    temp1 = ALLOC(64)      // Allocated here
    temp2 = ALLOC(128)     // Allocated here

    ... use temp1, temp2 ...

  END_SCOPE                // temp1, temp2 automatically freed
```

**Rules:**
- SCOPE begins a lifetime boundary
- Any ALLOC within scope is owned by that scope
- END_SCOPE frees all handles allocated in that scope
- Nested scopes work like nested stack frames

### Nested Scopes

```
BEHAVIOR outer():
  SCOPE
    h1 = ALLOC(8)

    BEHAVIOR inner():
      SCOPE
        h2 = ALLOC(8)      // Valid only in inner
      END_SCOPE            // h2 freed

    // h1 still valid
  END_SCOPE                // h1 freed
```

### Conclusion

**Scoped memory replaces stack allocation. Automatic cleanup at scope end. Compile-time verifiable.**

---

## Part 4: Pattern Memory (Persistent)

### The Problem

Scoped memory is temporary. What about:
- Cache that lives across many requests?
- Connection pool for application lifetime?
- State that persists between behavior invocations?

### Insight: Pattern as Memory Holder

Like C++ `unique_ptr` or `auto_ptr`, the pattern itself owns memory.

```
C++ auto_ptr:
  Object owns heap memory
  Object destructor frees memory
  Memory lives as long as object

Our model:
  Pattern owns memory
  Pattern termination frees memory
  Memory lives as long as pattern
```

### Solution: Pattern MEMORY Section

```
PATTERN connection_pool:
  MEMORY:                          // Declared pattern-level memory
    pool: 4096
    active_count: 8

  ON_CREATE:                       // When pattern instantiated
    pool = ALLOC(4096)
    active_count = ALLOC(8)

  BEHAVIOR get_connection(out):
    SCOPE
      temp = ALLOC(16)             // Scoped, temporary
      ... find connection in pool ...
      ... write to out ...
    END_SCOPE                      // temp freed, pool persists

  BEHAVIOR return_connection(conn):
    SCOPE
      ... return conn to pool ...
    END_SCOPE

  ON_DESTROY:                      // When pattern terminates
    // pool and active_count automatically freed
    // (or explicit FREE if needed)
```

### The Hierarchy

```
ROOT PATTERN (application lifetime)
│
├── MEMORY: global_config          // Lives forever
│
├── PATTERN: server                // Lives while server runs
│   │
│   ├── MEMORY: cache              // Lives while server exists
│   │
│   └── BEHAVIOR: handle_request   // Called many times
│       │
│       └── SCOPE                  // Per-invocation
│           └── temp               // Freed after each call
```

### Conclusion

**Pattern memory persists for pattern lifetime. Pattern is the memory holder (like auto_ptr). Scoped memory is temporary workspace.**

---

## Part 5: Ownership and Safety

### The Core Question

How does data escape a scope to return results?

### Options Considered

**Option A: Copy out (caller provides output handle)**
```
PATTERN outer:
  SCOPE
    output = ALLOC(8)           // Caller owns
    CALL inner(output)          // Pass to callee
  END_SCOPE                     // Caller frees

BEHAVIOR inner(out_handle):     // Receives, doesn't own
  SCOPE
    temp = ALLOC(8)             // Callee owns temp
    ... compute ...
    COPY(temp, out_handle, 8)   // Write to caller's memory
  END_SCOPE                     // temp freed
```
Compile-time verifiable:
- Ownership trivial: allocator always owns
- No ownership transfer to track
- COPY sizes verified against handle sizes
- **Accepted**

**Option B: Transfer ownership**
```
BEHAVIOR inner():
  SCOPE
    result = ALLOC(8)
    RETURN MOVE(result)         // Ownership transfers
  END_SCOPE                     // result NOT freed

h = CALL inner                  // Caller now owns
```
- Requires tracking ownership through call boundaries
- More complex compiler analysis
- Rust does this successfully, but adds complexity
- **Rejected for simplicity, may reconsider later**

**Option C: Explicit escape**
```
BEHAVIOR inner():
  SCOPE
    result = ALLOC(8, ESCAPE)   // Marked as escaping
  END_SCOPE                     // Not freed
  RETURN result                 // Who owns this?
```
- Unclear ownership after escape
- Leak risk if no one takes ownership
- Hard to verify statically
- **Rejected**

### The Rule

```
RULE: Allocator is always the owner.
RULE: Owner's scope/pattern end = automatic free.
RULE: To return data, caller provides handle, callee writes to it.
RULE: Borrowed handles can be read/written but not freed by borrower.
```

### Borrowing

When a handle is passed to a behavior:
- The behavior can READ from it
- The behavior can WRITE to it (if contract allows)
- The behavior CANNOT free it
- The behavior CANNOT store it beyond scope (unless pattern memory)

```
BEHAVIOR process(input, output):   // input and output are borrowed
  SCOPE
    // Can read input
    // Can write output
    // Cannot FREE input or output
    // Cannot store input/output in pattern MEMORY
  END_SCOPE
```

### Conclusion

**Single ownership. Allocator owns. Borrowing for access. Compile-time verifiable. No ownership transfer (simplicity).**

---

## Part 6: Memory Patterns Considered and Rejected

### Shared Ownership (like shared_ptr)

**What it is:**
```cpp
shared_ptr<Data> a = make_shared<Data>();
shared_ptr<Data> b = a;  // Both own it
// Freed when BOTH release (reference counting)
```

**Why considered:**
- Multiple patterns might need same data
- Convenient when ownership is unclear

**Why rejected:**
- Reference counting adds runtime cost
- Compile-time verification is harder (prove all refs freed)
- "Shared" data is usually one owner + multiple accessors
- Our model: single owner + borrowed handles achieves same goal

**Real examples:**
- Cache: Cache manager OWNS it, handlers BORROW access
- Connection pool: Pool OWNS, workers BORROW
- Config: App init OWNS, everything else BORROWS

**Conclusion:** Shared ACCESS (borrowing) is the real need, not shared OWNERSHIP. Model handles this.

---

### Static/Constant Data

**What it is:**
```cpp
const char* msg = "Hello";     // Compiled into binary
static int table[] = {1,2,3};  // Never freed
```

**Why considered:**
- String literals must live somewhere
- Lookup tables, constant data

**Analysis:**
- This is a real need, not human concept
- But AI doesn't need to manage it explicitly

**Solution:**
```
AI writes: STORE(h, "Hello", 5)
Compiler sees: literal value "Hello"
Compiler: embeds "Hello" in binary's data section
Runtime: literal exists at fixed address, read-only

No explicit memory "type" needed.
Compiler handles literals implicitly.
```

**Conclusion:** Real need, but compiler handles it. Not a memory type in the language.

---

### Resizable Memory (like vector)

**What it is:**
```cpp
vector<int> v;
v.push_back(1);  // Grows dynamically
```

**Why considered:**
- Unknown size at compile time
- Data grows during execution

**Analysis:**
- This is a real need, not human concept
- But `vector` is convenience wrapper over primitives

**What actually happens:**
```
1. ALLOC initial buffer
2. Fill it
3. When full: ALLOC bigger, COPY data, FREE old
4. Continue
```

**Do we need REALLOC primitive?**
- REALLOC tries to extend in-place (optimization)
- Semantically: ALLOC + COPY + FREE = same result
- Optimization can be added later

**Solution:**
```
BEHAVIOR resize(old, old_size, new_size, out):
  SCOPE
    new_buf = ALLOC(new_size)
    COPY(old, new_buf, old_size)
    // Caller will FREE old after this returns
    COPY(new_buf, out, new_size)
  END_SCOPE
```

**Conclusion:** Real need. Implemented as behavior from primitives. Not a new primitive or memory type.

---

### Thread-Local Storage

**What it is:**
```cpp
thread_local int counter = 0;  // Each thread has own copy
```

**Why considered:**
- Avoid passing context through every call
- Per-thread state

**Analysis:**
- Mostly human convenience
- Avoids explicitly passing data through call chains

**Our model already handles this:**
```
If thread = pattern instance:

PATTERN worker:
  MEMORY:
    my_counter: 8        // Each instance has its own
    my_cache: 1024

  BEHAVIOR process():
    ... uses my_counter, my_cache ...

Thread 1 runs worker instance A → has its own MEMORY
Thread 2 runs worker instance B → has its own MEMORY
```

Pattern memory IS thread-local when each thread is a pattern instance.

**Conclusion:** Not needed as separate concept. Pattern instance already provides this.

---

## Part 7: Summary of Memory Types

| Type | Lifetime | Owned By | Freed When | Use Case |
|------|----------|----------|------------|----------|
| Scoped | Single invocation | Scope | END_SCOPE | Temporary workspace |
| Pattern | Pattern lifetime | Pattern | Pattern terminates | Persistent state, caches |
| Borrowed | Caller decides | Caller | Caller's decision | Passing data between behaviors |

**That's it. Three concepts. No more needed.**

---

## Part 8: Compile-Time Safety Guarantees

The compiler MUST verify:

### Bounds Safety
```
Every LOAD/STORE proven within allocation bounds.
LOAD(h, size, offset) requires: offset + size <= h.size
```

### Lifetime Safety
```
No access to freed memory.
Handle valid only within owning scope/pattern.
Cannot store borrowed handle in pattern MEMORY (would outlive borrow).
```

### Initialization Safety
```
No read before write.
Every LOAD must be preceded by STORE to that location.
```

### Ownership Safety
```
Only owner can FREE.
Borrowed handles cannot be freed by borrower.
All owned handles freed at scope/pattern end.
```

### No Leaks
```
Every ALLOC has corresponding FREE.
Scoped: automatic at END_SCOPE.
Pattern: automatic at pattern termination.
```

If ANY verification fails → compilation fails → AI must fix.

> **Status note.** Most of the list above is enforced today, by a CFG-aware pass that checks
> every execution path: static bounds checking on LOAD/STORE offsets, use-after-free,
> double-free, **initialization-before-read** (a LOAD with no prior STORE on a path is rejected),
> memory-leak detection (an ALLOC not freed on every path), ownership (no freeing a borrowed
> INPUT or a handle a live SPAWN borrows), and `writes_output`. All have unit tests. The genuine
> gaps are narrower: `pure` is checked within a behavior **and** transitively, with the residual gap being
> *undeclared* pattern-`MEMORY` use only; `no_alloc` is enforced including scoped ALLOC (an ALLOC inside a
> balanced `SCOPE` is freed at `END_SCOPE` and satisfies it); and `all_paths_terminate` is enforced as a
> decidable structural reachability check (every block can reach an exit — not a semantic halting proof).
> See [STATUS.md](../STATUS.md) for the authoritative per-feature breakdown.

---

## Part 9: Complete Model

```
┌─────────────────────────────────────────────────────────────┐
│                      ROOT PATTERN                           │
│                  (Application Lifetime)                     │
│                                                             │
│  MEMORY:                                                    │
│    global_config     ← Lives for entire program             │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              PATTERN: server                         │   │
│  │           (Server Lifetime)                          │   │
│  │                                                      │   │
│  │  MEMORY:                                             │   │
│  │    cache          ← Lives while server runs          │   │
│  │    pool           ← Lives while server runs          │   │
│  │                                                      │   │
│  │  ┌────────────────────────────────────────────┐     │   │
│  │  │        BEHAVIOR: handle_request            │     │   │
│  │  │                                            │     │   │
│  │  │  SCOPE:                                    │     │   │
│  │  │    temp        ← Freed after each call     │     │   │
│  │  │    buffer      ← Freed after each call     │     │   │
│  │  │                                            │     │   │
│  │  │  Can access: cache, pool (borrow)          │     │   │
│  │  │  Can access: input, output (borrow)        │     │   │
│  │  └────────────────────────────────────────────┘     │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## Part 10: Key Decisions Summary

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| Variable identity | Opaque handles | Prevents forged addresses, enables bounds checking |
| Temporary memory | Scopes | Automatic cleanup, replaces stack frames |
| Persistent memory | Pattern MEMORY | Pattern as owner (like auto_ptr) |
| Returning data | Caller provides output | Simplest ownership model, fully verifiable |
| Shared ownership | Not supported | Borrow handles instead, simpler |
| Static data | Implicit (compiler) | Real need, but compiler handles it |
| Resizable | Behavior, not primitive | Built from ALLOC + COPY + FREE |
| Thread-local | Pattern instance | Each thread's pattern has own MEMORY |

---

## Part 11: What AI Needs to Know

When working with memory, AI tracks:

1. **What handles exist** in current scope/pattern
2. **What size** each handle is (for bounds)
3. **Who owns** each handle (scope, pattern, or borrowed)
4. **What's borrowed** vs owned (can't free borrowed)

The contract declares all of this explicitly:
```
CONTRACT:
  INPUT:
    data: handle(1024)      // Borrowed, 1024 bytes
  OUTPUT:
    result: handle(256)     // Borrowed, caller provides, 256 bytes
  MEMORY:
    cache: handle(4096)     // Pattern-owned, persists
```

AI working on this behavior knows exactly:
- `data` is borrowed, can read, 1024 bytes
- `result` is borrowed, must write, 256 bytes
- `cache` is owned by pattern, persists
- Any ALLOC in scope is temporary

---

*Document created during ideation session, January 2025*
*Three memory types: Scoped, Pattern, Borrowed. Everything else is not needed.*
