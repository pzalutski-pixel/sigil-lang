# Project Structure

> **Design specification.** This describes Sigil as designed; see [STATUS.md](../STATUS.md) for what the current compiler actually implements.

## Overview

This document defines how code is organized into files and projects, with a focus on AI discoverability and context limits. The core principle: AI maintains source files, compiler generates navigation indexes.

---

## Part 1: The Problem

### AI Challenges with Large Codebases

| Challenge | Impact |
|-----------|--------|
| Can't read everything | 1000 behaviors = too many files |
| Context limits | Can't hold entire codebase in memory |
| Session boundaries | Forgets between sessions |
| Finding things | Needs efficient search |
| Creating things | Needs to know where things go |
| Understanding impact | Needs to know dependencies |

### What AI Needs

| Need | Solution |
|------|----------|
| Quick overview | Summary files |
| Find by description | Searchable indexes |
| Find by type | Type indexes |
| Minimal updates | One file per change |
| Session continuity | Context file |
| Dependency awareness | Dependency graph |

---

## Part 2: Design Decisions

### Decision 1: Source vs Generated Files

**Problem:** If AI must update multiple files for one change, errors will happen.

**Options considered:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| All manual | AI updates all indexes manually | Full control | Error-prone, many files |
| All generated | Compiler generates everything | Consistent | AI can't customize |
| **Source + Generated** | AI updates source, compiler generates indexes | Minimal AI work, always consistent | Two types of files |

**Decision: Source + Generated**

- AI maintains: source files (.beh, manifest)
- Compiler generates: indexes, summaries, graphs
- AI reads generated files for navigation
- AI never edits generated files

**Reasoning:**
- One behavior = one file to edit
- Indexes always consistent with source
- No sync errors possible
- AI's job is simplified

### Decision 2: One Behavior Per File

**Problem:** How many behaviors per file?

**Options considered:**

| Option | Pros | Cons |
|--------|------|------|
| Multiple per file | Fewer files | AI must navigate within file, larger context |
| **One per file** | Minimal context, focused edits | More files |

**Decision: One behavior per file**

**Reasoning:**
- AI loads exactly what it needs
- Editing doesn't risk breaking other behaviors
- File name = behavior name (easy to find)
- Smallest possible context per task

### Decision 3: Predictable Directory Structure

**Problem:** Where do things go?

**Decision:** Namespace = directory

```
units/http/parse-request.beh
      └─┬─┘
        └── namespace "http" = directory "http"
```

**Reasoning:**
- No ambiguity where things go
- Easy to find: namespace tells you directory
- Easy to create: new namespace = new directory

### Decision 4: Layered Information

**Problem:** AI can't read everything, but needs to find things.

**Decision:** Information in layers, increasing detail

```
Layer 0: Project summary      (~50 tokens)
Layer 1: Unit summaries       (~200 tokens)
Layer 2: Behavior index       (~1000 tokens)
Layer 3: Full contracts       (on demand)
Layer 4: Implementations      (on demand)
```

**Reasoning:**
- AI reads summary first (always fits)
- AI drills down only as needed
- Most tasks don't need full codebase
- Context limits respected

### Decision 5: Session Continuity File

**Problem:** AI forgets between sessions.

**Decision:** `.context` file AI maintains

**Reasoning:**
- AI writes what it's working on
- AI reads to resume
- Explicit, not implicit memory
- Survives session boundaries

---

## Part 3: Directory Structure

```
project/
│
├── project.manifest              # Source: Project config
├── .context                      # Source: AI working state
│
├── units/                        # Source: All behaviors
│   ├── http/
│   │   ├── parse-request.beh
│   │   ├── parse-headers.beh
│   │   └── format-response.beh
│   │
│   ├── database/
│   │   ├── connect.beh
│   │   ├── query.beh
│   │   └── close.beh
│   │
│   └── cache/
│       ├── store.beh
│       ├── retrieve.beh
│       └── invalidate.beh
│
├── patterns/                     # Source: Pattern definitions
│   ├── http-server.pattern
│   └── worker-pool.pattern
│
├── executables/                  # Source: Entry points
│   ├── server.exe
│   └── cli.exe
│
└── .generated/                   # Generated: Navigation indexes (DESIGN INTENT - not implemented)
    ├── project.summary
    ├── units.summary
    ├── behaviors.index
    ├── contracts.registry
    ├── dependencies.graph
    └── types.index
```

> **DESIGN INTENT, partially realized (see [STATUS.md](../STATUS.md)):** For *application projects*, the `.generated/` navigation-index system and the `.context` session-continuity file are design intent — the compiler does not generate them. The idea is implemented for the *standard library*: the `--lib` build writes `behaviors.index` (name@hash), `contracts.registry` (full contracts), and `catalog.md` into `lib/release/.generated/`, and those are the discovery files agents actually use. `capabilities.index` is removed outright — capability tracking was rejected along with the SYSCALL primitive.

---

## Part 4: Source Files (AI Maintains)

### project.manifest

Project configuration and metadata.

```
PROJECT my-application
VERSION 1.0.0

DESCRIPTION
  Web server application with REST API and PostgreSQL database.
  Handles HTTP requests, authenticates users, manages data.

ENTRY server executables/server.exe
ENTRY cli executables/cli.exe

END
```

### {behavior}.beh

One behavior per file. Contains contract and implementation.

```
BEHAVIOR parse-request

DESCRIPTION
  Parses raw HTTP request bytes into structured request data.
  Extracts method, path, headers, and body.

CONTRACT
  INPUT request bytes 2048
  OUTPUT parsed bytes 512
  REQUIRES string::split@a1b2c3
  GUARANTEES pure writes_output

HASH d4e5f6a7

IMPLEMENTATION
  v1 = LOAD request 2048
  # ... implementation ...
  STORE parsed result 512
END
```

### {pattern}.pattern

Pattern with memory and behaviors.

```
PATTERN http-server

DESCRIPTION
  HTTP server that accepts connections and handles requests.

MEMORY
  config bytes 256
  connections bytes 8192

ON_CREATE
  config = ALLOC 256 bytes
  connections = ALLOC 8192 bytes
  # ... initialization ...

BEHAVIOR handle-connection
  CONTRACT
    INPUT conn bytes 64
    OUTPUT response bytes 4096
    REQUIRES http::parse-request@d4e5f6 http::format-response@a1b2c3
    GUARANTEES writes_output
  HASH e5f6a7b8
  COMPOSITION
    request = CALL http::parse-request conn
    # ... handling ...
    CALL http::format-response data -> response
  END

ON_DESTROY
  FREE config
  FREE connections

END_PATTERN
```

### {executable}.exe

Entry point definition.

```
EXECUTABLE server

DESCRIPTION
  Main HTTP server process.

USES http-server.pattern

ENTRY
  config = CALL load-config
  CALL run-http-server config

END_EXECUTABLE
```

> **Note (see [STATUS.md](../STATUS.md)):** Earlier drafts of this entry point used `server = SPAWN http-server config` followed by `WAIT server`. SPAWN/WAIT/CHANNEL **do** work — the `spawn-test` example uses exactly this pattern — so this is just a simpler synchronous illustration; the compile-time *safety* checks around concurrency are what remain unenforced, not the runtime.

### .context

AI's working state for session continuity.

```
CONTEXT my-application

TASK
  Implementing query result caching

WORKING_ON
  units/cache/cache-query.beh

STATUS
  In progress - implementation 60% complete

LOOKED_AT
  database::query@abc123 - studied for integration
  cache::store@def456 - pattern to follow

DECISIONS
  - Cache key will be hash of SQL query string
  - TTL configurable via input parameter (default 300s)
  - Will invalidate on any write to same table

NEXT_STEPS
  1. Finish implementation
  2. Add invalidation logic
  3. Test with database::query
  4. Update server to use cache

BLOCKERS
  None

UPDATED 2025-01-05T14:30:00Z

END
```

---

## Part 5: Generated Files (Compiler Maintains)

> **DESIGN INTENT, partially realized (see [STATUS.md](../STATUS.md)):** The compiler does not produce these files for application projects. Two of them exist for the standard library — the `--lib` build writes `behaviors.index` and `contracts.registry` (plus a `catalog.md`) into `lib/release/.generated/` — but the project-level files below (`project.summary`, `units.summary`, `dependencies.graph`, `types.index`) are not generated anywhere. Examples that reference `CAPABILITIES_USED`, `SYSCALL`, or `capabilities.index` are doubly out of date, since capability/syscall tracking was rejected along with the SYSCALL primitive.

### .generated/project.summary

Auto-generated overview.

```
# AUTO-GENERATED FROM project.manifest + analysis

PROJECT my-application
VERSION 1.0.0

DESCRIPTION
  Web server application with REST API and PostgreSQL database.
  Handles HTTP requests, authenticates users, manages data.

STATISTICS
  Units: 5
  Behaviors: 23
  Patterns: 3
  Executables: 2

UNITS
  http: HTTP request/response handling (5 behaviors)
  database: PostgreSQL operations (4 behaviors)
  cache: In-memory caching (3 behaviors)
  auth: Authentication/authorization (6 behaviors)
  util: Utility functions (5 behaviors)

# NOTE: the CAPABILITIES_USED block shown in earlier drafts is removed -
# capability/syscall tracking was rejected. System access is via NATIVE stdlib behaviors.

ENTRY_POINTS
  server: Main HTTP server
  cli: Command-line admin tool

GENERATED 2025-01-05T14:00:00Z
```

### .generated/units.summary

Auto-generated unit descriptions.

```
# AUTO-GENERATED FROM unit behaviors

UNITS

UNIT http
  PATH units/http/
  BEHAVIORS 5
  DESCRIPTION HTTP request and response handling

  BEHAVIORS_LIST
    parse-request: Parse raw HTTP into structured data
    parse-headers: Extract headers from request
    parse-body: Extract and decode body
    format-response: Format data as HTTP response
    format-error: Format error response

  # (CAPABILITIES field removed - capability tracking was rejected; these are all pure.)

  DEPENDENCIES
    USES: string (for parsing)
    USED_BY: server, cli

UNIT database
  PATH units/database/
  BEHAVIORS 4
  DESCRIPTION PostgreSQL database operations

  BEHAVIORS_LIST
    connect: Establish database connection
    query: Execute SQL query
    execute: Execute SQL without results
    close: Close connection

  # CAPABILITIES line removed - capability/syscall tracking was rejected.
  # System access is via NATIVE stdlib behaviors listed in REQUIRES.

  DEPENDENCIES
    USES: none
    USED_BY: server, cache

# ... more units ...

GENERATED 2025-01-05T14:00:00Z
```

### .generated/behaviors.index

Searchable one-liner index.

```
# AUTO-GENERATED FROM .beh files

BEHAVIORS

http::parse-request
  DESC Parses raw HTTP request bytes into structured data
  IN request:bytes:2048
  OUT parsed:bytes:512
  TAGS http parsing input
  PURE yes
  HASH d4e5f6a7

http::parse-headers
  DESC Extracts headers from HTTP request
  IN request:bytes:2048
  OUT headers:bytes:1024
  TAGS http parsing headers
  PURE yes
  HASH a1b2c3d4

http::format-response
  DESC Formats response data as HTTP response bytes
  IN data:bytes:4096 status:int:4
  OUT response:bytes:4096
  TAGS http formatting output
  PURE yes
  HASH b2c3d4e5

database::connect
  DESC Establishes connection to PostgreSQL database
  IN config:bytes:256
  OUT conn:bytes:64
  TAGS database connection
  HASH c3d4e5f6

database::query
  DESC Executes SQL query and returns result rows
  IN conn:bytes:64 sql:bytes:1024
  OUT results:bytes:8192
  TAGS database sql query
  HASH d4e5f6a7

# NOTE: the "SYSCALL yes" tag shown in earlier drafts is removed -
# syscall/capability tracking was rejected. Such behaviors REQUIRE NATIVE stdlib behaviors instead.

# ... all behaviors ...

GENERATED 2025-01-05T14:00:00Z
```

### .generated/contracts.registry

Full contracts for all behaviors.

```
# AUTO-GENERATED FROM .beh files

REGISTRY

BEHAVIOR http::parse-request
  CONTRACT
    INPUT request bytes 2048
    OUTPUT parsed bytes 512
    REQUIRES string::split@a1b2c3
    GUARANTEES pure writes_output
  HASH d4e5f6a7
  LOCATION units/http/parse-request.beh

BEHAVIOR http::format-response
  CONTRACT
    INPUT data bytes 4096
    INPUT status int 4
    OUTPUT response bytes 4096
    REQUIRES string::concat@b2c3d4
    GUARANTEES pure writes_output
  HASH e5f6a7b8
  LOCATION units/http/format-response.beh

# ... all contracts ...

GENERATED 2025-01-05T14:00:00Z
```

### .generated/dependencies.graph

What uses what.

```
# AUTO-GENERATED FROM REQUIRES in contracts

DEPENDENCIES

http::parse-request
  HASH d4e5f6a7
  USES
    string::split@a1b2c3
  USED_BY
    server::handle-request@111111
    cli::process-input@222222

http::format-response
  HASH e5f6a7b8
  USES
    string::concat@b2c3d4
  USED_BY
    server::handle-request@111111

database::query
  HASH d4e5f6a7
  USES
    (none)
  USED_BY
    server::handle-request@111111
    cache::cache-query@333333

server::handle-request
  HASH 111111
  USES
    http::parse-request@d4e5f6a7
    http::format-response@e5f6a7b8
    database::query@d4e5f6a7
    auth::validate@444444
  USED_BY
    (entry:server)

# ... all dependencies ...

GENERATED 2025-01-05T14:00:00Z
```

### .generated/types.index

Find behaviors by input/output types.

```
# AUTO-GENERATED FROM contracts

TYPES

INPUT:bytes
  http::parse-request (2048)
  http::parse-headers (2048)
  http::format-response (4096)
  compression::compress (any)
  compression::decompress (any)

INPUT:int
  math::add (4)
  math::multiply (4)
  cache::set-ttl (4)

OUTPUT:bytes
  http::parse-request (512)
  http::format-response (4096)
  database::query (8192)

OUTPUT:int
  math::add (4)
  string::length (4)
  cache::get-ttl (4)

OUTPUT:bool
  auth::validate (1)
  cache::exists (1)
  file::exists (1)

GENERATED 2025-01-05T14:00:00Z
```

### .generated/capabilities.index (REMOVED)

> **REMOVED / not implemented (January 2025):** The `capabilities.index` file was dropped. SYSCALL:* capability tracking was never implemented, and both the SYSCALL primitive and CAPABILITIES declarations were rejected. The PURE / NO_ALLOC grouping concept could still apply to the implemented guarantees (`pure`, `no_alloc`) if a guarantee index were ever generated, but no such file exists today. The example below is retained only as a record of the rejected design — the `SYSCALL:*` groupings do not exist.

```
# REJECTED design - this file is not generated:

CAPABILITIES

PURE (no side effects)
  http::parse-request
  http::parse-headers
  http::format-response
  ...

NO_ALLOC (no memory allocation)
  math::add
  math::multiply
  string::length

# SYSCALL:* groupings from the original draft are gone -
# capability/syscall tracking was rejected.
```

---

## Part 6: AI Workflow

### Starting a Session

```
1. AI reads: .context (if exists)
   → Knows what it was working on

2. AI reads: .generated/project.summary
   → Understands project scope

3. AI reads: .generated/behaviors.index
   → Can search for anything
```

### Finding a Behavior

```
Task: "Find behavior that parses HTTP requests"

1. AI searches .generated/behaviors.index for "http" + "parse"
2. Finds: http::parse-request
   DESC Parses raw HTTP request bytes into structured data
   IN request:bytes:2048
   OUT parsed:bytes:512
   PURE yes
   HASH d4e5f6a7

3. If more detail needed, AI reads from:
   .generated/contracts.registry (full contract)
   OR
   units/http/parse-request.beh (implementation)
```

### Creating a New Behavior

```
Task: "Create behavior to cache database queries"

1. AI checks: .generated/units.summary
   → Sees cache unit exists at units/cache/

2. AI checks: .generated/behaviors.index
   → Finds similar: cache::store, cache::retrieve

3. AI reads: units/cache/store.beh
   → Studies pattern to follow

4. AI creates: units/cache/cache-query.beh
   BEHAVIOR cache-query
   DESCRIPTION Caches database query results
   CONTRACT
     INPUT conn bytes 64
     INPUT sql bytes 1024
     OUTPUT results bytes 8192
     REQUIRES database::query@d4e5f6a7 cache::store@aabbcc
     GUARANTEES writes_output
   HASH (computed)
   IMPLEMENTATION
     ...
   END

5. AI runs: compile
   → Compiler regenerates all .generated/ files

6. AI updates: .context
   → Notes what was done
```

### Modifying a Behavior

```
Task: "Change parse-request to handle larger requests"

1. AI checks: .generated/dependencies.graph
   → Sees: USED_BY server::handle-request, cli::process-input

2. AI opens: units/http/parse-request.beh

3. AI modifies: INPUT request bytes 4096 (was 2048)

4. Contract changed → Hash changes

5. AI runs: compile
   → Compiler detects hash change
   → Compiler reports: server::handle-request uses old hash
   → AI must update dependents

6. AI opens: units/server/handle-request.beh
   → Updates REQUIRES to new hash

7. AI runs: compile
   → All hashes match
   → .generated/ files regenerated
```

### Ending a Session

```
1. AI updates: .context
   CONTEXT my-application

   TASK
     Implementing query result caching

   STATUS
     Complete - ready for testing

   COMPLETED
     - Created cache-query.beh
     - Integrated with server

   NEXT_STEPS
     1. Test with production-like data
     2. Add metrics/logging

   UPDATED 2025-01-05T16:00:00Z
   END

2. AI runs: compile
   → Ensures everything is consistent

3. Session can end safely
```

---

## Part 7: File Naming Conventions

### Behavior Files

```
{behavior-name}.beh

Examples:
  parse-request.beh
  format-response.beh
  validate-token.beh
```

### Pattern Files

```
{pattern-name}.pattern

Examples:
  http-server.pattern
  worker-pool.pattern
  connection-manager.pattern
```

### Executable Files

```
{executable-name}.exe

Examples:
  server.exe
  cli.exe
  migrate.exe
```

### Directory Names (Namespaces)

```
Lowercase, hyphenated

Examples:
  units/http/
  units/database/
  units/auth-service/
  units/cache-manager/
```

---

## Part 8: What We Rejected

### Manual Index Maintenance

**What:** AI updates all indexes manually.

**Why rejected:**
- Too many files to update
- Sync errors inevitable
- AI will forget to update one

### Everything in One File

**What:** All behaviors in one big file.

**Why rejected:**
- Large context required
- Editing risks breaking other behaviors
- Hard to navigate

### No Navigation Indexes

**What:** AI reads source files directly.

**Why rejected:**
- Can't search efficiently
- Must read many files
- Doesn't scale

### File Paths in Code

**What:** `REQUIRES "./libs/http/parse.beh"`

**Why rejected:**
- Paths are human concern
- AI shouldn't manage file paths
- Hash + name is sufficient

---

## Part 9: Platform Abstraction

> **DESIGN INTENT, partially realized (see [STATUS.md](../STATUS.md)):** `--target` selection is not implemented, and only Windows is built and validated. The cross-platform *code* exists — the C runtime carries POSIX `#else` branches throughout (`platform.h`, `file.c`, `net.c`, `console.c`: pthreads, BSD sockets, POSIX I/O), and a Linux/macOS CI workflow is prepared but has never run. The NATIVE + C-runtime architecture below is the portability path; what's missing is building and validating on non-Windows platforms, not the structure.

### The Problem

Different operating systems have different system call interfaces:

| Operation | Linux | macOS | Windows |
|-----------|-------|-------|---------|
| write | syscall 1 | syscall 4 | _write() |
| read | syscall 0 | syscall 3 | _read() |
| socket | syscall 41 | syscall 97 | WSASocket() |

How do we handle this without burdening AI with platform details?

### Decision: Platform Is Build Concern, Not Code Concern

**Options considered:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| AI writes platform code | AI generates platform conditionals | Full control | AI must know syscall numbers, duplicated logic |
| Named syscalls | `SYSCALL write ...` | AI uses names | Compiler must map, not all syscalls map 1:1 |
| **Stdlib abstracts** | AI calls stdlib behaviors | AI has zero platform knowledge | Stdlib must be comprehensive |

**Decision: Stdlib provides platform abstraction**

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AI-GENERATED CODE                        │
│                                                             │
│  Composite behaviors using stdlib                           │
│  CALL file/write, CALL network/send, etc.                   │
│  NO platform knowledge required                             │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ uses
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                 STANDARD LIBRARY                            │
│                                                             │
│  Pre-built leaf behaviors with platform implementations     │
│  Distributed with compiler per target platform              │
└─────────────────────────────────────────────────────────────┘
                          │
                          │ at build time
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                 COMPILER + TARGET                           │
│                                                             │
│  sigil-compiler --target <platform> main.beh                │
│  Selects correct stdlib implementation for target           │
└─────────────────────────────────────────────────────────────┘
```

### What AI Writes

AI generates portable code by calling stdlib behaviors:

```
COMPOSITION
  fd = CALL file/open path flags
  data = CALL file/read fd 4096
  CALL file/close fd
END
```

**AI never writes:**
- Raw syscall numbers (`SYSCALL 1 ...`)
- Platform conditionals
- OS-specific APIs

### Build-Time Target Selection

Compiler accepts target platform flag:

```
sigil-compiler --target linux main.beh     # Linux executable   (DESIGN INTENT - not implemented)
sigil-compiler --target windows main.beh   # Windows executable  (DESIGN INTENT - no --target flag; Windows is the only output today)
sigil-compiler --target macos main.beh     # macOS executable    (DESIGN INTENT - not implemented)
```

> **Not implemented (see [STATUS.md](../STATUS.md)):** `--target` selection does not exist today. The compiler produces Windows executables only.

### Standard Library Structure

```
stdlib/
  units/
    file/
      open.beh      # NATIVE - runtime provides implementation
      read.beh
      write.beh
      close.beh
    network/
      socket.beh
      send.beh
      receive.beh
    console/
      print.beh
      read.beh

runtime/
  src/
    file.c          # Platform-specific implementations
    network.c
    console.c
```

Each stdlib behavior has:
- One CONTRACT (portable interface)
- NATIVE declaration (no Sigil implementation)

The runtime library (C code) provides platform-specific implementations:
- Uses `#ifdef _WIN32` / `#else` for platform branching
- Compiled separately per platform
- Linked with compiled Sigil code

---

**Addendum (January 2025): Architecture Update**

The original design showed `IMPLEMENTATION[linux]`, `IMPLEMENTATION[windows]`, etc. in stdlib .beh files. This was superseded:

**Original approach (superseded):**
```
BEHAVIOR write
IMPLEMENTATION[linux]
  result = SYSCALL 1 fd data len
END
IMPLEMENTATION[windows]
  result = SYSCALL 0 fd data len
END
```

**Current approach (NATIVE + Runtime):**
```
BEHAVIOR write
CONTRACT
  INPUT fd int 8
  INPUT data bytes 65536
  INPUT length int 8
  OUTPUT bytes_written int 8
NATIVE
```

```c
// runtime/src/file.c
void sigil_write(...) {
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
- Runtime compiled separately per platform

### Why This Works

| Benefit | Explanation |
|---------|-------------|
| AI focuses on logic | No platform knowledge needed |
| Portable by default | Same code compiles for all platforms |
| Platform in stdlib | Experts write platform code once |
| Build-time selection | No runtime overhead |
| Extensible | New platforms = new stdlib implementations |

### What We Rejected

**AI writes platform conditionals:**
- AI must memorize syscall numbers
- Duplicated logic across platforms
- Error-prone

**Runtime platform detection:**
- Runtime overhead
- Larger binaries
- Unnecessary complexity

---

## Part 10: Summary

### File Responsibilities

| File Type | Maintained By | Purpose |
|-----------|---------------|---------|
| project.manifest | AI | Project configuration |
| .context | AI | Session continuity |
| units/**/*.beh | AI | Behavior definitions |
| patterns/*.pattern | AI | Pattern definitions |
| executables/*.exe | AI | Entry points |
| .generated/* | Compiler | Navigation indexes |

### AI Workflow Summary

| Action | What AI Does |
|--------|--------------|
| Start session | Read .context, .generated/project.summary |
| Find behavior | Search .generated/behaviors.index |
| Create behavior | Write new .beh file, run compile |
| Modify behavior | Edit .beh file, run compile, update dependents if hash changed |
| End session | Update .context, run compile |

### Key Principles

| Principle | Implementation |
|-----------|----------------|
| One behavior = one file | Minimal context per task |
| Source + Generated | AI writes source, compiler generates indexes |
| Layered information | Summaries → Indexes → Contracts → Implementation |
| Session continuity | .context file |
| Namespace = directory | Predictable organization |

---

*Document created during ideation session, January 2025*
*AI maintains source. Compiler generates indexes. Minimal updates, maximum discoverability.*
