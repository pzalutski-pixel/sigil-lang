# Concurrency

> **Status: the runtime works, and the core compile-time safety checks work too.** `SPAWN`, `WAIT`, `WAIT_ALL`, `WAIT_ANY`, `CHANNEL`, `CHANNEL_SEND`, `CHANNEL_RECEIVE`, and `CHANNEL_CLOSE` are implemented end-to-end: the compiler lowers them to a green-thread runtime on real OS threads with bounded channels, and the `spawn-test`, `parallel-test`, `channel-test`, and `producer-consumer` examples build and run (Windows). The compile-time safety this document describes is largely enforced and tested: **non-atomic access to a `SHARED` handle is a compile error, and freeing a handle a live `SPAWN` still borrows is a compile error.** What is **not** done is *automatic parallelization* (the compiler does not infer parallelism — independent calls run sequentially) and full `WAIT` result-passing (simplified in codegen). See [STATUS.md](../STATUS.md).

## Overview

This document defines the concurrency model for the language. The core principle: AI expresses WHAT should run concurrently, the runtime decides HOW to run it efficiently.

---

## Part 1: The Core Insight

### Semantics vs Implementation

Most concurrency complexity is about IMPLEMENTATION, not SEMANTICS.

| What AI Needs to Express | What Humans Typically Deal With |
|--------------------------|--------------------------------|
| "Run this concurrently" | OS thread? Green thread? Thread pool? Async task? |
| "Wait for completion" | Blocking? Polling? Callback? Future? |
| "Send data to other unit" | Shared memory? Lock? Channel? Message queue? |
| "Wait for I/O" | Blocking I/O? Async I/O? Event loop? Select/poll/epoll? |

**AI doesn't care about implementation details. AI cares about expressing concurrency semantics.**

### The Division of Responsibility

```
AI's Job:
  - Express what should run concurrently
  - Express data flow between concurrent units
  - Express when to wait for results

Runtime's Job:
  - Decide OS threads vs green threads
  - Manage scheduling and load balancing
  - Handle I/O efficiently (async under the hood)
  - Optimize for hardware (core count, memory, etc.)
```

---

## Part 2: Real-World Use Cases Analysis

### Use Case 1: Background Task

**Example:** Write to log file while handling request.

**What's actually needed:** Another execution context that runs independently.

```
worker = SPAWN log-writer message
# ... continue main work ...
# Optionally wait later:
WAIT worker
```

**Any threading model works.** This is simple.

### Use Case 2: Parallel Computation (CPU-bound)

**Example:** Process 1000 images across 8 CPU cores.

**What's actually needed:** Work distributed across physical cores.

```
workers = SPAWN_EACH images process-image
results = WAIT_ALL workers
```

**Requires real parallelism.** Async/single-thread won't help. Need multiple OS threads or green threads mapped to cores.

**Runtime handles:** Distributes work across available cores.

### Use Case 3: I/O Concurrency (10,000 connections)

**Example:** Web server handling 10,000 concurrent connections.

**What's actually needed:** Efficient waiting for I/O on many connections.

```
LOOP
  conn = ACCEPT server
  SPAWN handle-connection conn
END
```

**10,000 OS threads = disaster.** Each thread uses ~1MB stack. Would need 10GB just for stacks.

**Solution:** Green threads or async I/O. Few OS threads, runtime multiplexes.

**AI doesn't care.** AI just spawns. Runtime handles efficient I/O waiting.

### Use Case 4: Producer/Consumer Pipeline

**Example:** Read from network → Process → Write to disk.

**What's actually needed:** Queue between stages, coordination.

```
queue = CHANNEL item 1000

SPAWN producer queue input
SPAWN processor queue output_queue
SPAWN writer output_queue
```

**Channels are the natural abstraction.** Works regardless of threading model.

### Use Case 5: Periodic Task

**Example:** Health check every 5 seconds.

**What's actually needed:** Timer + execution.

```
PATTERN health-checker
  BEHAVIOR run:
    LOOP
      CALL check-health
      SLEEP 5000
    END
```

**Simple.** Any model works.

### Use Case 6: Event Handling

**Example:** React to GUI events, network events.

**What's actually needed:** Wait for events, dispatch handlers.

```
LOOP
  event = CHANNEL_RECEIVE events
  SPAWN handle-event event
END
```

**Channels model events naturally.** Event source sends to channel.

---

## Part 3: Threading Models Evaluated

### OS Threads (C/C++/pthreads)

```
Direct OS thread creation.
~1MB stack per thread.
Preemptive scheduling.
Shared memory by default.
Manual synchronization.
```

| Aspect | Assessment |
|--------|------------|
| Control | Maximum |
| Complexity | High |
| Scalability | Limited (thousands, not millions) |
| I/O efficiency | Poor (thread per connection doesn't scale) |
| AI-friendly | **No** - too much manual management |

### Green Threads (Go goroutines, Erlang processes)

```
Lightweight threads (~2KB).
Runtime manages scheduling.
Millions possible.
Often message-passing.
```

| Aspect | Assessment |
|--------|------------|
| Control | Medium |
| Complexity | Low |
| Scalability | Excellent |
| I/O efficiency | Good (runtime handles) |
| AI-friendly | **Yes** |

### Async/Await (JavaScript, C#, Rust async)

```
Single thread or few threads.
Event loop based.
Cooperative scheduling.
"Colored" functions (async vs sync).
```

| Aspect | Assessment |
|--------|------------|
| Control | Low |
| Complexity | Medium (function coloring problem) |
| Scalability | Good for I/O |
| CPU parallelism | Poor (single thread) |
| AI-friendly | **Partially** - coloring adds complexity |

### Actor Model (Erlang, Akka)

```
Isolated actors with own state.
Message passing only.
No shared memory.
Supervision trees.
```

| Aspect | Assessment |
|--------|------------|
| Control | Low |
| Complexity | Low |
| Safety | Excellent |
| Performance | Good (some message overhead) |
| AI-friendly | **Yes** |

### What We Learned

| Model | Good For | Bad For |
|-------|----------|---------|
| OS Threads | CPU-bound, few threads | Many concurrent tasks |
| Green Threads | Many concurrent tasks | Nothing |
| Async/Await | I/O-bound | CPU-bound, adds complexity |
| Actors | Safety, isolation | Raw performance |

**Green threads/actors are most AI-friendly.** Async adds complexity. OS threads don't scale.

---

## Part 4: The Chosen Model

### Design Principles

| Principle | Rationale |
|-----------|-----------|
| AI expresses semantics | What runs concurrently, not how |
| Runtime handles implementation | OS threads, scheduling, I/O |
| Channels for communication | Safe, clear ownership transfer |
| Shared memory as escape hatch | For performance when needed |

### Concurrency Unit: Pattern Instance

```
Pattern = Unit of concurrent execution

- Has isolated MEMORY (thread-local storage)
- Has behaviors (what it does)
- Has lifecycle (ON_CREATE, ON_DESTROY)
- Communicates via channels or shared handles
```

Why Pattern:
- Already defined in our model
- Natural isolation (MEMORY section)
- Natural lifecycle
- Fits ownership model

### Primitives

**SPAWN - Create Concurrent Unit**

```
handle = SPAWN pattern args
```

Creates new pattern instance running concurrently. Returns handle to it.

**WAIT - Wait for Completion**

```
result = WAIT handle
```

Blocks until concurrent unit completes. Returns its result.

**WAIT_ALL - Wait for Multiple**

```
r1, r2, r3 = WAIT_ALL h1 h2 h3
```

Waits for all to complete. Destructures results into separate identifiers.

```
WAIT_ALL h1 h2 h3
```

Without assignment, just waits and discards results.

### Decision: WAIT_ALL Return Structure

**Problem:** How should AI access results from multiple concurrent operations?

**Options considered:**

| Option | Syntax | Access Pattern |
|--------|--------|----------------|
| Single result | `results = WAIT_ALL h1 h2 h3` | `results.h1` or `results[0]` |
| **Destructuring** | `r1, r2, r3 = WAIT_ALL h1 h2 h3` | `r1`, `r2`, `r3` directly |

**Decision: Destructuring**

**Reasoning from AI perspective:**
- Consistent with WAIT_ANY multi-return style
- Results immediately usable, no field access needed
- Explicit: 3 spawns → 3 results
- Less to track mentally
- Optional assignment for fire-and-forget patterns

**WAIT_ANY - Wait for First**

```
first_result, which = WAIT_ANY h1 h2 h3
```

Returns when any completes. Returns result and which one.

**CHANNEL - Create Communication Channel**

```
ch = CHANNEL type capacity
```

Creates channel for passing data. Type is interpretation, capacity is buffer size.

**CHANNEL_SEND - Send to Channel**

```
CHANNEL_SEND ch value
```

Sends value to channel. Blocks if channel is full.

**CHANNEL_RECEIVE - Receive from Channel**

```
value = CHANNEL_RECEIVE ch
```

Receives value from channel. Blocks if channel is empty. Returns immediately with closed flag if channel is closed.

**CHANNEL_CLOSE - Close Channel**

```
CHANNEL_CLOSE ch
```

Closes channel, signaling no more values will be sent.

### Decision: Explicit vs Implicit Channel Close

**Problem:** How does a consumer know when a producer is done?

**Options considered:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| Implicit (pattern lifecycle) | Channel closes when owning pattern terminates | No extra syntax | AI must structure code around pattern lifetimes |
| Sentinel value | Producer sends special "done" value | No language change | Convention, not enforceable |
| **Explicit CHANNEL_CLOSE** | Producer calls CHANNEL_CLOSE when done | Clear semantics, compiler can verify | Extra operation |

**Decision: Explicit CHANNEL_CLOSE**

**Reasoning from AI perspective:**
- AI expresses semantic intent ("I'm done sending") directly
- No need to align pattern structure with data flow
- Handles unknown item counts, early termination, errors
- Compiler can verify "no use after close"
- Consumer gets clear signal, no ambiguity

**What AI writes:**
```
COMPOSITION
  # Producer
  LOOP
    item = CALL produce
    BRANCH item.done close_channel send_item

    LABEL send_item
      CHANNEL_SEND ch item
      JUMP LOOP

    LABEL close_channel
      CHANNEL_CLOSE ch
END
```

---

## Part 5: Communication Patterns

### Pattern 1: Fire and Forget

```
COMPOSITION
  SPAWN log-event event
  # Don't wait, continue immediately
END
```

### Pattern 2: Wait for Result

```
COMPOSITION
  worker = SPAWN compute-value input
  # ... do other work ...
  result = WAIT worker
END
```

### Pattern 3: Parallel Fan-Out

```
COMPOSITION
  w1 = SPAWN fetch-user user_id
  w2 = SPAWN fetch-orders user_id
  w3 = SPAWN fetch-prefs user_id

  user, orders, prefs = WAIT_ALL w1 w2 w3

  CALL combine user orders prefs -> output
END
```

### Pattern 4: Producer/Consumer

```
PATTERN producer
  BEHAVIOR run:
    COMPOSITION
      LOOP
        item = CALL produce
        CHANNEL_SEND out_channel item
      END
    END
END

PATTERN consumer
  BEHAVIOR run:
    COMPOSITION
      LOOP
        item = CHANNEL_RECEIVE in_channel
        CALL consume item
      END
    END
END

# Main
COMPOSITION
  ch = CHANNEL item 100
  p = SPAWN producer ch
  c = SPAWN consumer ch
  WAIT_ALL p c
END
```

### Pattern 5: Worker Pool

```
COMPOSITION
  work_queue = CHANNEL task 1000
  result_queue = CHANNEL result 1000

  # Spawn workers
  workers = []
  LOOP 8 times
    w = SPAWN worker work_queue result_queue
    workers.push(w)
  END

  # Submit work
  CALL submit-all-tasks work_queue tasks

  # Collect results
  results = CALL collect-all result_queue count
END
```

### Pattern 6: Request/Response

```
COMPOSITION
  # Create response channel
  response_ch = CHANNEL response 1

  # Send request with response channel
  request = CREATE_REQUEST data response_ch
  CHANNEL_SEND server_queue request

  # Wait for response
  result = CHANNEL_RECEIVE response_ch
END
```

---

## Part 6: Shared Memory (Escape Hatch)

### When Channels Aren't Enough

Channels involve copying data. For high-performance scenarios with large data:

```
# Explicitly shared memory
shared_buffer = ALLOC 1048576 bytes SHARED
```

### Rules for Shared Memory

| Rule | Enforcement |
|------|-------------|
| Must be marked SHARED | Compile-time |
| Must use atomics for access | Compile-time |
| Non-atomic access = compile error | Compile-time |

### Shared Memory Access

```
# Atomic operations required
ATOMIC_STORE shared_buffer value 8 offset
value = ATOMIC_LOAD shared_buffer 8 offset
success = CAS shared_buffer expected new 8 offset
```

### Example: Lock-Free Queue

```
BEHAVIOR enqueue:
  IMPLEMENTATION
    LABEL retry
      tail = ATOMIC_LOAD queue_tail 8
      next_tail = IADD tail item_size 8
      success = CAS queue_tail tail next_tail 8
      BRANCH success write retry

    LABEL write
      STORE queue_data item item_size tail
  END
```

### When to Use Shared Memory

| Use Case | Recommendation |
|----------|----------------|
| Simple data passing | Channel |
| Request/response | Channel |
| Large data, high frequency | Shared memory |
| Lock-free data structures | Shared memory |
| Default choice | **Channel** |

---

## Part 7: What Runtime Handles

AI doesn't manage these. Runtime handles automatically.

### Thread Management

| Aspect | Runtime Decision |
|--------|------------------|
| OS threads vs green threads | Based on workload type |
| Number of OS threads | Based on core count |
| Thread creation/destruction | Pooled and reused |
| Stack size | Adjusted dynamically for green threads |

### Scheduling

| Aspect | Runtime Decision |
|--------|------------------|
| Which unit runs on which core | Load balancing |
| When to preempt | Fair scheduling |
| Work stealing | Balance across cores |
| Priority | Based on I/O vs CPU bound |

### I/O Handling

| Aspect | Runtime Decision |
|--------|------------------|
| Blocking vs async | Async internally |
| Event mechanism | epoll/kqueue/IOCP per platform |
| I/O thread pool | Separate pool for blocking I/O |

### Example: How Runtime Handles 10,000 Connections

```
AI writes:
  LOOP
    conn = ACCEPT server
    SPAWN handle-connection conn
  END

Runtime does:
  1. Uses few OS threads (e.g., 8 for 8 cores)
  2. Creates green thread for each connection
  3. Uses epoll/kqueue to wait for I/O
  4. Schedules green threads when I/O ready
  5. Multiplexes thousands of green threads onto 8 OS threads
```

AI didn't specify any of this. AI just spawned.

---

## Part 8: What Compiler Verifies

### Channel Safety

```
# Compiler verifies:
- Channel send/receive types match
- Channel is accessible in scope
- No use after close
```

### Shared Memory Safety

```
# Compiler verifies:
- SHARED handles only accessed with atomics
- Non-atomic LOAD/STORE on SHARED = compile error
- Ownership rules still apply (someone owns the shared region)
```

### Ownership in Concurrency

```
# When passing handle to spawned pattern:
- Handle is borrowed by spawned pattern
- Original owner can't free until spawn completes
- Compiler tracks this
```

---

## Part 9: Lifecycle

### Pattern Instance Lifecycle

```
1. SPAWN called
2. Pattern instance created
3. ON_CREATE runs
4. Pattern executes (behaviors can be called)
5. Pattern completes (ON_CREATE finishes or explicit end)
6. ON_DESTROY runs
7. Pattern memory freed
8. WAIT returns result
```

### Termination

**Normal termination:**
```
PATTERN worker
  ON_CREATE
    CALL do-work input
    # ON_CREATE completes = pattern terminates
  END
END
```

**Result returning:**
```
PATTERN compute-worker
  MEMORY
    result int 8

  ON_CREATE
    value = CALL compute input
    STORE result value 8
  END
END

# Caller
worker = SPAWN compute-worker input
result = WAIT worker    # Gets result
```

**Long-running with signal:**
```
PATTERN server
  MEMORY
    running int 1

  ON_CREATE
    STORE running 1 1
    LABEL loop
      r = LOAD running 1
      BRANCH r continue stop

      LABEL continue
        CALL handle-request
        JUMP loop

      LABEL stop
  END

  BEHAVIOR stop:
    IMPLEMENTATION
      STORE running 0 1
    END
END

# Caller
server = SPAWN server config
# ... later ...
CALL server.stop
WAIT server
```

---

## Part 10: Complete Example

### Web Server

```
PATTERN http-server
  MEMORY
    config bytes 256
    running int 1

  ON_CREATE
    STORE running 1 1
    socket = CALL create-server-socket config.port

    LABEL accept_loop
      r = LOAD running 1
      BRANCH r accept stop

      LABEL accept
        conn = CALL accept-connection socket
        SPAWN connection-handler conn
        JUMP accept_loop

      LABEL stop
        CALL close-socket socket
  END

  BEHAVIOR stop:
    IMPLEMENTATION
      STORE running 0 1
    END
END

PATTERN connection-handler
  ON_CREATE
    # input: conn handle
    request = CALL read-request conn

    parsed = CALL parse-http request
    BRANCH parsed.valid handle_request bad_request

    LABEL handle_request
      response = CALL process-request parsed
      CALL write-response conn response
      JUMP done

    LABEL bad_request
      CALL write-error conn 400
      JUMP done

    LABEL done
      CALL close-connection conn
  END
END

# Main
EXECUTABLE web-app
  PATTERN main
    ON_CREATE
      config = CALL load-config
      server = SPAWN http-server config

      # Wait for shutdown signal
      CALL wait-for-shutdown

      CALL server.stop
      WAIT server
    END
  END_PATTERN

  ENTRY main
END_EXECUTABLE
```

### Parallel Image Processing

```
PATTERN image-processor
  ON_CREATE
    # input: image handle
    processed = CALL apply-filters image
    CALL save-result processed output_path
  END
END

PATTERN batch-processor
  ON_CREATE
    # input: images list, output_dir
    workers = []

    LOOP images
      w = SPAWN image-processor current_image output_dir
      workers.push(w)
    END

    WAIT_ALL workers
  END
END
```

---

## Part 11: Summary

### What AI Expresses

| Operation | Meaning |
|-----------|---------|
| `SPAWN pattern args` | Run concurrently |
| `WAIT handle` | Wait for completion |
| `WAIT_ALL h1 h2...` | Wait for all |
| `WAIT_ANY h1 h2...` | Wait for first |
| `CHANNEL type cap` | Create communication channel |
| `CHANNEL_SEND ch val` | Send to channel |
| `CHANNEL_RECEIVE ch` | Receive from channel |

### What Runtime Handles

| Aspect | Runtime Responsibility |
|--------|----------------------|
| Thread type | OS vs green |
| Thread count | Based on cores |
| Scheduling | Fair, efficient |
| I/O | Async internally |
| Load balancing | Work stealing |

### What We Rejected

| Concept | Why Rejected |
|---------|--------------|
| Explicit OS threads | Doesn't scale, complex |
| Async/await coloring | Adds complexity |
| Manual thread pools | Runtime should decide |
| Mutex/semaphore primitives | Use channels instead |

### What We Kept

| Concept | Why Kept |
|---------|----------|
| SPAWN/WAIT | Simple, expresses concurrency |
| Channels | Safe communication, clear ownership |
| Shared memory escape hatch | Performance when needed |
| Atomics | Required for shared memory |

---

*Document created during ideation session, January 2025*
*AI expresses concurrency. Runtime handles efficiency.*
