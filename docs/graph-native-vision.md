# Graph-Native Vision for Sigil

The design record for Sigil's graph-native direction. **Sigil is already graph-native
where it counts:** the artifact *is* a content-addressed behavior graph, and the
compiler builds the whole-program graph and verifies it *as a graph* on every compile
(`compiler/src/graph/` — transitive purity across the call graph, dependency-cycle
rejection, hash-pin consistency, and the outputs-consumed half of edge-level
completeness, `E0511`). The text format is just the current way that graph is *written
down*, because today's models emit tokens — not the nature of the artifact.

What this document explores is the **forward** piece: a graph-native *authoring*
interface — the model constructing the graph through validated tool-calls instead of
typing its serialization (the MCP graph-construction API, "Mode 2") — plus the binary
format, the full edge-level port model (so `find_gaps` runs over explicit edges), and
the gap-driven elicitation loop. Those are **unbuilt designs**; defer to
[STATUS.md](STATUS.md) for what ships.

---

## 1. Executive Summary

Sigil programs ARE graphs. Compositions are dataflow graphs (nodes = behaviors, edges = data flow). Implementations are control flow graphs (nodes = operations, edges = control/data flow). The current text format is a serialization compromise -- designed in January 2025 when LLMs could only generate tokens sequentially and couldn't output graph structures directly.

Fourteen months later, the landscape has shifted. Not because LLMs can output graphs natively (they still can't -- transformers still predict tokens sequentially), but because **tool-use as graph construction** sidesteps the limitation entirely. Instead of generating a serialized graph, the LLM calls `add_behavior()`, `set_contract()`, `connect()` operations incrementally. Each call is validated immediately. The graph is built correctly by construction. Current frontier models can chain 50-100+ tool calls reliably. This IS the graph-native vision realized through a different mechanism than originally envisioned.

**The direction this points at: a dual-mode architecture.**

1. **Mode 1 (Text v2):** a denser v2 text grammar (the [v2 grammar candidates](v2-grammar-candidates.md) collect the options) with constrained-decoding support — works today with every LLM, the safe foundation.

2. **Mode 2 (Graph Construction API):** a tool-use API (~10 tools) for incremental graph construction, published as an MCP server. The text format becomes a serialization/display format, not the primary authoring format.

3. **Internal graph IR:** regardless of input mode, the compiler builds and retains a full program graph (not per-behavior CFGs), enabling cross-behavior validation, transitive guarantee checking, and gap-driven elicitation. *This is the piece that's built.*

The critical reframing: the Retrospective asked "can LLMs output graphs directly?" The better question is **"can LLMs construct graphs incrementally through validated operations?"** The answer to that is yes, today. No other AI-native language (Dana, MoonBit) has gone graph-native. Sigil would be first.

---

## 2. Background: From Text Compromise to Graph-Native

### 2.1 The Original Vision

The Retrospective (January 2025) identified a fundamental insight: programs are graphs. Text is a serialization format designed for human cognitive limitations that AI doesn't have. The ideal pipeline:

```
AI IDEAL: [Intent] ─────────────────────────→ [Structure] → [Optimize] → [Binary] → [Execute]
```

Skip the text layer entirely. AI outputs graph structures at Level 3. Nodes and edges instead of text and syntax.

### 2.2 Why Text Was Chosen

The Retrospective immediately identified the blocker: "Current LLMs can't do this. LLMs generate text token by token... they can't output graph structures directly." Text was a pragmatic compromise that preserved the graph vision (behaviors, contracts, compositions) while working within LLM constraints.

### 2.3 What Changed (2025-2026)

Three developments shifted the landscape:

1. **Structured JSON output** reached near-100% schema compliance (OpenAI strict mode, Anthropic tool use). Small-to-medium graphs (~15 behaviors) can be reliably expressed as JSON.

2. **Constrained decoding** advanced from CFG-only to context-sensitive parsing. SynCode achieves 96% syntax error reduction. Type-constrained code generation (PLDI 2025) reduces compilation errors by >50%.

3. **Tool-use reliability** reached production quality. Claude Opus 4.5 achieves 88.1% on MCP evaluations. Models can chain 50-100+ tool calls. Production systems (ComfyUI-Copilot: 85K+ queries; n8n: 220 executions/sec) already use LLMs to construct graphs via tool-use APIs.

### 2.4 The Earlier Study's Blind Spot

The v2 grammar candidates study (docs/v2-grammar-candidates.md) designed excellent expression-level improvements -- reducing token overhead from 8.1x to 3.3x vs Python. But they optimized the TEXT representation exclusively. They treated the graph structure as something to be serialized more efficiently, rather than questioning whether serialization was necessary at all.

---

## 3. Research Findings

### 3.1 Graph Representation Design


#### The Node/Edge Model

Both composition graphs and leaf behavior graphs share a common metamodel:

```
Port = (name, interpretation, size, direction: {in, out, error})

Node = (
    id:        NodeId,
    label:     String,
    kind:      {behavior, operation, control, call, subgraph, placeholder},
    ports:     [Port],
    subgraph:  Option<Graph>,
)

Edge = (
    source:    (NodeId, PortName),
    target:    (NodeId, PortName),
    kind:      {data_flow, control_flow, error_flow},
)

DanglingEdge = (
    endpoint:  (NodeId, PortName),
    direction: {inbound, outbound},
    question:  Option<String>,    -- elicitation question this gap generates
)
```

**Key design decision: Incomplete graphs are first-class.** `DanglingEdge` and `placeholder` node kinds allow expressing graphs with gaps. Each gap generates a typed elicitation question. This is the critical feature that no existing format (ONNX, TensorFlow, WASM) supports. It is what makes Sigil's graph model genuinely novel.

#### Binary Format: FlatBuffers with Merkle Hashing

The binary canonical format uses FlatBuffers-style zero-copy access with content-addressable Merkle hashing:

- **Zero-copy access:** Compiler memory-maps `.sigil-bin` files and reads contract hashes without deserializing entire files. For a project with 1000 behaviors but only 10 changed, reading 1000 contract hashes costs microseconds.
- **Content-addressable identity:** Every graph, node, and contract has a hash computed from its content. This enables caching, deduplication, and incremental compilation.
- **Schema evolution:** FlatBuffers' field-ID system allows adding fields without breaking old readers.

```
File Layout:
+---------------------------+
| Magic: "SIGL" (4 bytes)  |
| Version: u16             |
+---------------------------+
| Section Table             |
|   Contract, Nodes, Edges, |
|   Ports, Strings,         |
|   Dangling, Subgraphs,    |
|   Hashes                  |
+---------------------------+
| Section Data...           |
+---------------------------+
```

A complete FlatBuffers schema is defined in the full representation designer report (Appendix, Section 4.3).

#### Text Format: Hybrid Graph DSL

The text serialization uses a hybrid approach that respects the nature of each graph type:

- **Compositions** use explicit `node`/`edge` declarations -- because compositions ARE graphs with no inherent order among nodes.
- **Implementations** use v2 imperative syntax inside a `body` block -- because implementations ARE sequential (control flow imposes order).

This hybrid recognizes a real structural difference: trying to represent sequential control flow as nodes and edges is verbose and less readable than imperative syntax, while trying to represent dataflow topology as imperative call sequences hides the graph structure.

#### Size Comparison

| Format | Pipeline Total | vs Python |
|--------|---------------|-----------|
| v1 Sigil text | ~930 tokens | 8.1x |
| v2 Candidate B | ~385 tokens | 3.3x |
| Graph Text (hybrid) | ~385 tokens | 3.3x |
| Graph Binary | ~1,356 bytes | N/A |
| JSON equivalent | ~1,080 tokens | 9.4x |

The graph text format is **token-neutral** with v2 Candidate B overall. Compositions save tokens (explicit edges are shorter than call+error patterns). The graph framing adds small overhead. The real win is structural, not token-based.

### 3.2 LLM Graph Generation Capabilities


#### Approach 1: Structured JSON Output

Near-100% schema compliance is achievable with modern constrained decoding. But JSON Schema **cannot enforce referential integrity** between graph nodes (ensuring an edge references an existing node). Cross-reference consistency runs at ~85-90% without explicit validation. Works for small graphs (~15 behaviors), hits output token limits at scale.

**Verdict:** Necessary but insufficient. Good for small graphs; needs validation layer.

#### Approach 2: Constrained Decoding for Graph Grammars

CFG-based constrained decoding is production-ready (SynCode, XGrammar). Context-sensitive constrained decoding has emerged (arXiv:2508.15866) supporting variable scope and type constraint tracking. But global graph properties (completeness, acyclicity) cannot be enforced token-by-token.

**Feasibility by constraint type:**

| Constraint | Enforceable During Generation? | Status |
|-----------|-------------------------------|--------|
| Syntax | Yes (CFG) | Production-ready |
| Node reference validity | Yes (context-sensitive) | Research prototype |
| Port type matching | Yes (context-sensitive) | Research prototype |
| Acyclicity | Expensive at generation time | Post-hoc preferred |
| Completeness | No (global property) | Post-hoc only |

**Verdict:** Useful for text v2 syntax; insufficient alone for graph well-formedness.

#### Approach 3: Tool-Use as Graph Construction (The Breakthrough)

Instead of generating a serialization, the LLM calls graph operations incrementally:

```
LLM: create_behavior("validate-input")
LLM: set_contract("validate-input", inputs=[...], outputs=[...])
LLM: create_behavior("transform-data")
LLM: set_contract("transform-data", ...)
LLM: connect("validate-input.cleaned", "transform-data.data")
     → Server validates: types match (bytes/4096 → bytes/4096) ✓
LLM: set_implementation("validate-input", body="if raw_length <= 0...")
     → Server runs semantic checks ✓
```

Each tool call is validated immediately. Invalid operations are rejected with error messages. The graph is **correct by construction.**

**Reliability data:**
- Frontier models chain 50-100+ tool calls reliably
- Prompt2DAG: 78-93% success rate for DAG generation
- ComfyUI-Copilot: 85K+ queries from 19K users (production)
- Claude Opus 4.5: 88.1% accuracy on MCP evaluations

**Scalability:**
- 5-10 behaviors: ~30-60 tool calls (high reliability)
- 10-25 behaviors: ~100-200 tool calls (good with retry loops)
- 25-50+ behaviors: ~200-500+ calls (feasible with hierarchical decomposition)

**Verdict:** The most promising path. Achieves the original vision through a different mechanism.

#### Approach 4: Multi-Modal

Vision models can understand diagrams but cannot reliably output graph structures. Graph2Token (February 2026, arXiv:2602.01771) shows promise for graph understanding but is not generation-ready.

**Verdict:** Not ready. Interesting for future input modality (sketch → graph).

#### Other AI-Native Languages

Neither Dana (AI Alliance, Python-like with `reason()` calls) nor MoonBit (AI-friendly syntax with structured sampling) went graph-native. Both chose to optimize text-based approaches. ComfyUI, n8n, LangGraph, and Neo4j all demonstrate production LLM-driven graph construction using tool-use/API patterns.

**Verdict:** Sigil would be first to go graph-native. No precedent, but the pattern (tool-use for graph construction) is validated in adjacent domains.

### 3.3 Verification on Native Graphs


#### What Gets Simpler

Five verification phases currently in `semantic/mod.rs` become unnecessary or trivial:

| Check | Current | On Graph | Change |
|-------|---------|----------|--------|
| Label resolution | Two-pass scan, build HashMap | Eliminated -- edges are direct | **Removed entirely** |
| CFG reconstruction | O(N+E) from text | Eliminated -- program IS graph | **Removed entirely** |
| Contract hash verification | Recompute SHA-256 from text | Structural identity | **Removed entirely** |
| Type checking on CALL args | Post-hoc arg matching | Validated at edge creation | **Amortized to O(1)** |
| Path enumeration | O(2^B) exponential | O(V*E) polynomial dataflow | **Orders of magnitude** |

#### What Becomes Possible

Three checks the per-behavior compiler structurally couldn't do become straightforward graph queries — and the internal graph IR (§7.1, since built) now performs the latter two and half of the first:

1. **Cross-behavior edge-level validation:** the biggest gap in the per-behavior compiler. The **outputs-consumed** half (unrouted errors, ignored return values) is now caught across a composition (`E0511`); the remaining piece is full **input-sourcing** over explicit edges. On a full program graph, both are O(ports) — linear.

2. **Transitive guarantee checking:** now enforced — a `pure` behavior that depends, even transitively, on a non-pure one is a compile error. Follow CALL edges and check callee guarantees, O(N+D) on the call graph.

3. **Cycle detection in dependency graphs:** the acyclicity assumption (critical for decidability) is now verified — the graph pass rejects dependency cycles. O(V+E) via Tarjan/Kahn.

#### The Draft/Complete Duality

The most architecturally significant finding: structural completeness can become a **construction constraint** rather than a post-hoc check, but only with a two-phase model:

**Draft graphs** (elicitation mode):
- Dangling edges permitted -- each IS a typed question
- Partial contracts allowed (hash = `???`)
- Placeholder nodes represent unspecified behaviors
- Verification reports what is MISSING (gap set), not pass/fail

**Complete graphs** (compilation mode):
- All construction constraints enforced
- No dangling edges, no placeholders
- Full structural completeness (OutputConsumption, InputSourcing, ResourcePairing)
- Transitive guarantees satisfied

**The transition from draft to complete IS the elicitation process.** Each step fills a gap and reduces incompleteness. The process terminates because gaps are finite and each step reduces them.

Formally:
```
WF(G, draft) =
    valid port types AND type-compatible edges AND acyclic dependencies

WF(G, complete) =
    WF(G, draft) AND no dangling edges AND SC(G) AND TransitiveGuarantees(G)

Gaps(G) = { unconnected ports } ∪ { unsatisfied SC properties }
```

#### Gap-Driven Elicitation: The Killer Feature

On text, gaps are implicit -- the compiler parses text, reconstructs a graph, finds missing connections, and generates diagnostic messages. The AI must parse error messages to understand what's needed.

On graphs, gaps are **first-class structural entities**:

| Aspect | Text-Based | Graph-Based |
|--------|-----------|------------|
| Gap identification | Compile, parse errors | Direct query: unconnected ports |
| Gap type | String: "outputs not written" | Typed port: (bytes, 4096, output) |
| Gap context | Re-read surrounding text | Graph neighborhood query |
| Completion validation | Full recompile | Local edge check + incremental verify |
| AI interface | "Fix error on line 42" | "Connect this typed port" |
| Iteration speed | Seconds (full recompile) | Milliseconds (local check) |

The gap carries its type constraint, location in the graph, surrounding context, and what would satisfy it. The AI doesn't guess -- the graph TELLS it what's needed.

#### Incremental Verification

Graph mutations are localized. Recent work (IncIDFA, POPL 2025) demonstrates up to 11x speedups for incremental dataflow analysis on changed graph portions. This enables real-time verification during construction -- verification results in milliseconds after each graph mutation.

---

## 4. Vision A: Graph-Native Sigil Today

*What's buildable with current technology (structured output, tool use, constrained decoding).*

### 4.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Developer / AI                        │
│                                                          │
│   ┌──────────────┐         ┌──────────────────────┐     │
│   │  Text v2     │         │  Graph Construction  │     │
│   │  (.beh files)│         │  API (MCP Server)    │     │
│   └──────┬───────┘         └──────────┬───────────┘     │
│          │ parse                      │ validated ops    │
│          │                            │                  │
│   ┌──────▼────────────────────────────▼───────────┐     │
│   │              Internal Graph IR                 │     │
│   │   (full program graph, content-addressable)    │     │
│   └──────┬──────────────────────────┬─────────────┘     │
│          │                          │                    │
│   ┌──────▼──────────┐    ┌─────────▼──────────┐        │
│   │  Verification    │    │  Gap Analysis       │        │
│   │  (incremental)   │    │  (elicitation)      │        │
│   └──────┬──────────┘    └─────────┬──────────┘        │
│          │                          │                    │
│   ┌──────▼──────────┐    ┌─────────▼──────────┐        │
│   │  LLVM Codegen    │    │  Serialization      │        │
│   │  (native binary) │    │  (.sigil-bin, text)  │        │
│   └─────────────────┘    └────────────────────┘        │
└─────────────────────────────────────────────────────────┘
```

Two input paths, one internal representation, one verification engine.

### 4.2 The Graph Construction API (MCP Tools)

A Sigil MCP server exposing ~10 tools:

| Tool | Parameters | What It Does | Validation |
|------|-----------|-------------|-----------|
| `create_behavior` | name, kind (leaf\|composite) | Create new behavior node | Name uniqueness |
| `set_contract` | name, inputs[], outputs[], errors[], guarantees[] | Define contract | Valid types/sizes, guarantee set |
| `set_requires` | name, dependencies[] | Set behavior dependencies | Referenced behaviors exist |
| `set_implementation` | name, body (v2 text) | Set leaf behavior body | Compiler semantic checks |
| `connect` | source (behavior.port), target (behavior.port) | Wire behaviors together | Both ports exist, types match |
| `disconnect` | source, target | Remove a wire | Edge exists |
| `add_placeholder` | name, inputs[], outputs[] | Add unspecified behavior | Valid port definitions |
| `get_graph` | (optional: name) | View current graph state | -- |
| `get_gaps` | (optional: name) | View unconnected ports/gaps | -- |
| `compile` | output_path | Full compilation check + codegen | All completeness checks |

**Why ~10 tools:** Reliability degrades with tool count. OpenAI finds <100 tools performs reliably. Anthropic's Tool Search helps manage larger libraries. A minimal set is better than a comprehensive one.

**The key insight:** Each tool call IS a graph operation. The LLM doesn't generate a serialization -- it constructs the graph. Invalid operations are rejected with error messages, and the LLM retries. The graph is correct by construction.

### 4.3 Internal Graph Format

The compiler's internal representation is the graph model from Section 3.1:
- Nodes with typed ports
- Edges with source/target port references
- DanglingEdges for incomplete graphs
- Content-addressable hashing for caching

Storage formats:
- `.beh` -- v1/v2 text input (backward-compatible)
- `.sigil` -- graph text format (human-readable, version-controlled)
- `.sigil-bin` -- graph binary format (FlatBuffers, compiler cache)

### 4.4 Text Display Format

When a human needs to see the program, it's rendered as the hybrid graph DSL:

- Compositions show explicit nodes and edges (the topology IS the information)
- Implementations show v2 imperative syntax (the sequence IS the information)

This is a VIEW of the graph, not the source of truth. The graph is the source of truth.

### 4.5 Verification Pipeline

The verification pipeline simplifies dramatically:

**Current:** text → lex → parse → AST → CFG reconstruction → exponential path enumeration → per-behavior check

**Graph-native:** graph → polynomial dataflow analysis → full-program verification

Specifically:
1. Skip label resolution, CFG reconstruction, hash re-verification (eliminated)
2. Type checking amortized to edge creation time
3. Path enumeration replaced by polynomial dataflow analysis (the dead code in `cfg.rs` lines 380-463 comes alive)
4. Cross-behavior edge-level validation (outputs-consumed enforced; input-sourcing the remaining half) becomes a simple port connectivity check
5. Transitive guarantee checking (now enforced for `pure`) becomes call-graph traversal
6. Incremental re-verification on graph mutation (milliseconds, not seconds)

### 4.6 Gap-Driven Elicitation Loop

The concrete interaction:

```
1. Developer describes intent: "Build a pipeline that validates input,
   transforms it, and writes output"

2. AI calls graph construction tools:
   create_behavior("validate-input", kind="leaf")
   create_behavior("transform-data", kind="leaf")
   create_behavior("process-pipeline", kind="composite")
   → Graph created with 3 behavior nodes, all ports disconnected

3. AI sets contracts:
   set_contract("validate-input",
     inputs=[{name: "raw", type: "bytes", size: 4096}, ...],
     outputs=[{name: "cleaned", type: "bytes", size: 4096}, ...])
   → Contract validated ✓

4. AI wires composition:
   connect("validate-input.cleaned", "transform-data.data")
   → Types match (bytes/4096 → bytes/4096) ✓

5. AI queries gaps:
   get_gaps()
   → Returns: [
       "process-pipeline: input 'raw' unsourced",
       "process-pipeline: output 'processed' unconsumed",
       "validate-input: error ports unhandled",
       "validate-input: no implementation",
       "transform-data: no implementation"
     ]

6. AI fills gaps iteratively:
   set_implementation("validate-input", body="if raw_length <= 0...")
   → Semantic checks pass. Guarantee 'pure' verified. ✓

   connect("process-pipeline.raw", "validate-input.raw")
   → Pipeline input wired to first behavior ✓

7. Repeat until get_gaps() returns []

8. compile("output.exe")
   → Full verification passes. Native binary produced.
```

Each gap is typed and contextual. The AI doesn't parse error messages -- it queries the graph for unconnected ports.

### 4.7 Example: Hello World (Full Interaction)

**Developer prompt:** "Create a hello-world program that prints a greeting."

**LLM tool calls:**
```
create_behavior("hello-world", kind="composite")
set_contract("hello-world",
  outputs=[{name: "status", type: "int", size: 8}],
  requires=["println@5cb2c2f1"],
  guarantees=["writes_output"])

connect("literal:Hello, World!", "println.message")
connect("literal:13", "println.length")
connect("println.status", "hello-world.status")
```

**Resulting graph text display:**
```
graph hello-world : composition
  contract
    out status int 8
    requires println@5cb2c2f1
    guarantees writes_output
  hash 0281265c

  node $1 behavior println@5cb2c2f1

  edge literal:"Hello, World!" -> $1.message : data
  edge literal:13 -> $1.length : data
  edge $1.status -> $.status : data
end
```

**Verification result:** Complete. No gaps. All outputs sourced. `writes_output` satisfied via `println`.

### 4.8 Example: Three-Behavior Pipeline (Full Interaction)

**Developer prompt:** "Build a data processing pipeline: validate input, transform it, produce output. Handle errors at each stage."

**LLM tool calls (abbreviated -- full sequence would be ~25-35 calls):**
```
-- Create behaviors
create_behavior("validate-input", kind="leaf")
create_behavior("transform-data", kind="leaf")
create_behavior("process-pipeline", kind="composite")

-- Set contracts
set_contract("validate-input",
  inputs=[{name:"raw", type:"bytes", size:4096},
          {name:"raw_length", type:"int", size:8}],
  outputs=[{name:"cleaned", type:"bytes", size:4096},
           {name:"cleaned_length", type:"int", size:8}],
  errors=[{name:"code", type:"int", size:4},
          {name:"message", type:"string", size:256}],
  guarantees=["pure", "writes_output"])

set_contract("transform-data", ...)  -- similar

set_contract("process-pipeline",
  inputs=[...], outputs=[...], errors=[...],
  requires=["validate-input@a3f7c910", "transform-data@b40e82d1"],
  guarantees=["writes_output"])

-- Wire composition
connect("process-pipeline.raw", "validate-input.raw")
connect("process-pipeline.raw_length", "validate-input.raw_length")
connect("validate-input.cleaned", "transform-data.data")
connect("validate-input.cleaned_length", "transform-data.length")
connect("transform-data.result", "process-pipeline.processed")
connect("transform-data.result_length", "process-pipeline.processed_length")

-- Set implementations
set_implementation("validate-input", body="""
  if raw_length <= 0
    fail code = 1 message = "Input is empty"
  end
  if raw_length > 4096
    fail code = 2 message = "Input exceeds maximum size"
  end
  header = raw[0]
  if header != 1
    fail code = 3 message = "Invalid header byte"
  end
  out cleaned_length = raw_length
  i = 0
  while i < raw_length limit 4096
    out cleaned[i] = raw[i]
    i = i + 1
  end
""")

set_implementation("transform-data", body="""
  out result_length = length
  i = 0
  while i < length limit 4096
    b = data[i]
    if b == 255
      fail code = 10 message = "Invalid byte 0xFF in data"
    end
    out result[i] = bit_xor(b, 170) + 1
    i = i + 1
  end
""")

-- Check gaps
get_gaps()
→ ["validate-input: error ports unhandled in process-pipeline",
   "transform-data: error ports unhandled in process-pipeline"]

-- Fill remaining gaps (error handling)
-- ... (error propagation wiring)

compile("pipeline.exe")
→ ✓ Complete. Binary produced.
```

**Resulting graph text display:**
```
graph validate-input : implementation
  contract
    in raw bytes 4096
    in raw_length int 8
    out cleaned bytes 4096
    out cleaned_length int 8
    err code int 4
    err message string 256
    guarantees pure writes_output
  hash a3f7c910

  body
    if raw_length <= 0
      fail code = 1 message = "Input is empty"
    end
    if raw_length > 4096
      fail code = 2 message = "Input exceeds maximum size"
    end
    header = raw[0]
    if header != 1
      fail code = 3 message = "Invalid header byte"
    end
    out cleaned_length = raw_length
    i = 0
    while i < raw_length limit 4096
      out cleaned[i] = raw[i]
      i = i + 1
    end
  end
end

graph transform-data : implementation
  contract
    in data bytes 4096
    in length int 8
    out result bytes 4096
    out result_length int 8
    err code int 4
    err message string 256
    guarantees pure writes_output
  hash b40e82d1

  body
    out result_length = length
    i = 0
    while i < length limit 4096
      b = data[i]
      if b == 255
        fail code = 10 message = "Invalid byte 0xFF in data"
      end
      out result[i] = bit_xor(b, 170) + 1
      i = i + 1
    end
  end
end

graph process-pipeline : composition
  contract
    in raw bytes 4096
    in raw_length int 8
    out processed bytes 8192
    out processed_length int 8
    err stage int 4
    err code int 4
    err message string 256
    requires validate-input@a3f7c910 transform-data@b40e82d1
    guarantees writes_output
  hash e5d019f3

  node $1 behavior validate-input@a3f7c910
  node $2 behavior transform-data@b40e82d1

  edge $.raw -> $1.raw : data
  edge $.raw_length -> $1.raw_length : data
  edge $1.cleaned -> $2.data : data
  edge $1.cleaned_length -> $2.length : data
  edge $2.result -> $.processed : data
  edge $2.result_length -> $.processed_length : data

  on_error $1
    fail stage = 1 code = $1.error.code message = $1.error.message
  end
  on_error $2
    fail stage = 2 code = $2.error.code message = $2.error.message
  end
end
```

---

## 5. Vision B: The North Star

*What Sigil looks like when LLMs can output graphs directly. No text layer at all.*

### 5.1 The Graph-First Programming Model

In the north star, there is no text representation of a program. The program IS the graph. The graph IS the program. There is no serialization, no parsing, no reconstruction.

The AI constructs the graph through validated operations. The compiler verifies the graph's structural completeness. The graph compiles directly to native code. The human interacts through a visual graph editor or conversational interface -- never through a text file.

```
[Intent] → [AI constructs graph] → [Compiler verifies graph] → [Native binary]
              ↕ interactive                ↕ incremental
         [Human reviews/edits]      [Real-time verification]
```

### 5.2 Developer Interaction

The developer never sees text code. Instead:

**Conversational mode:** The developer describes intent in natural language. The AI constructs the graph via tool-use. The developer sees a live visualization of the graph being built. Dangling edges glow as questions. The developer can click on any node to inspect its contract, any edge to see the data flowing through it, any gap to understand what's needed.

**Visual editing mode:** The developer drags and drops behavior nodes onto a canvas. Typed ports show as colored connectors. Edges snap between compatible ports (type checking at connection time). Incompatible connections are visually rejected. The verification state updates in real-time.

**Hybrid mode:** The developer describes high-level architecture conversationally ("I need a pipeline that validates, transforms, and outputs"), then refines visually (dragging edges, adjusting contracts, filling gaps). The AI assists by suggesting completions for gaps.

### 5.3 Verification by Construction

In the north star, many classes of bugs literally cannot be expressed:

- **Type mismatches:** Edges can only connect compatible ports. The graph API rejects incompatible connections.
- **Missing inputs:** Draft mode shows all unconnected input ports as glowing gaps. Complete mode refuses to compile until all gaps are filled.
- **Unhandled errors:** Error ports are visible. An unhandled error port is a gap.
- **Guarantee violations:** A `pure` behavior node cannot contain CHANNEL or MEMORY_ACCESS operation nodes. The graph API rejects them.
- **Circular dependencies:** The graph API rejects edges that create cycles (Sigil's acyclicity constraint).

What remains hard: semantic correctness (does the implementation do what the developer intends?) -- this is a fundamental limit regardless of representation.

### 5.4 Example: Hello World in North Star Vision

The developer says: "Print hello world."

The AI constructs a graph. The developer sees:

```
┌───────────────────────────────────┐
│       process-pipeline            │
│  ┌─────────┐     ┌────────────┐  │
│  │ "Hello,  │     │  println   │  │
│  │  World!" ├────→┤ message    │  │
│  └─────────┘     │            │  │
│  ┌─────────┐     │            │  │
│  │    13    ├────→┤ length     │  │
│  └─────────┘     │            │  │
│                   │    status ─┼──→ status
│                   └────────────┘  │
└───────────────────────────────────┘
```

No text. No syntax. Just structure. Click on `println` to see its contract. Click on the edge to see the data type (bytes/4096). Click "compile" to produce a binary.

### 5.5 Example: Pipeline in North Star Vision

```
┌──────────────────────────────────────────────────────────────┐
│                    process-pipeline                           │
│                                                               │
│  raw ──────→┌──────────────┐                                 │
│  raw_length→│validate-input │                                 │
│             │   [pure]      │                                 │
│             │   cleaned ────┼──→┌───────────────┐             │
│             │   cleaned_len─┼──→│transform-data  │             │
│             │   err ────┐   │   │   [pure]       │             │
│             └───────────│───┘   │   result ──────┼──→processed│
│                         │       │   result_len ──┼──→proc_len │
│                         │       │   err ─────┐   │             │
│                         │       └────────────│───┘             │
│                         │                    │                 │
│                    ┌────▼────────────────────▼──┐             │
│                    │     Error Handler          │             │
│                    │  stage, code, message ─────┼──→ err      │
│                    └───────────────────────────┘             │
└──────────────────────────────────────────────────────────────┘
```

Every port is visible. Every edge shows data flow. Guarantees are badges on nodes. Gaps would appear as pulsing unconnected ports.

### 5.5 Required LLM Advances

For the full north star, LLMs would need:

1. **Native graph tokens** -- the Graph2Token research (February 2026) encodes graph topology into single tokens. This is understanding-only today; generation would require the reverse mapping.

2. **Spatial reasoning** -- generating graphs requires understanding 2D topology, not just sequential relationships. Current transformers are weak at this.

3. **Persistent graph state** -- the LLM must maintain and manipulate a graph state across an entire session. Current tool-use approximates this but doesn't internalize it.

4. **Multi-modal output** -- generating both graph structure and visual layout simultaneously.

**Timeline estimate:** 2-4 years for tool-use to become seamless enough that it feels like native graph output. 5-10 years for true native graph generation (if ever -- this may not be how transformer architectures evolve).

---

## 6. The Critical Question: Graphs vs Text

### 6.1 The Case for Graphs

**Elicitation is structurally richer.** A dangling edge IS the question. It carries type, context, and constraints. Text error messages are lossy projections of this information. The AI working with gaps can make more targeted completions.

**Verification is simpler.** No parsing, no CFG reconstruction, no label resolution, no hash re-verification. Five phases eliminated. Polynomial replaces exponential. Cross-behavior validation becomes possible.

**The program's true structure is visible.** Compositions ARE dataflow graphs. Showing them as imperative call sequences hides the topology. Edge-based representation makes the wiring explicit.

**Tool-use construction achieves higher structural accuracy.** Each operation validated immediately. ~0% structural error rate vs ~10-15% cross-reference failures in text generation.

**Incremental verification enables real-time feedback.** Graph mutations are localized. Text changes require full re-parse. The difference is milliseconds vs seconds.

**No other language does this.** Dana and MoonBit chose text. ComfyUI, n8n, LangGraph validate the pattern in adjacent domains. Sigil would be genuinely novel.

### 6.2 The Case for Text

**Text is universal.** Every tool supports it: editors, git, grep, diff, code review. Graph tools are nascent. Building a graph ecosystem is a massive investment.

**LLMs are optimized for text.** That's what transformers DO. Text generation is mature, fast, and reliable. Graph construction via tool-use adds latency (5-30s vs 2-5s for small programs) and infrastructure (graph server).

**The v2 grammar is already very compact.** ~385 tokens for the pipeline. The graph text format achieves ~385 tokens too. There's no token saving.

**Adding a graph layer adds complexity.** Two input formats (text + graph API), two parsers, graph server infrastructure, MCP integration. This is real engineering cost.

**Nobody else went graph-native.** Dana (backed by IBM, Meta, AI Alliance) chose Python-like syntax. MoonBit chose text with AI-friendly structure. Either they're wrong, or there's a reason.

**Text works.** The v1 compiler works. Zero-shot generation succeeded (with rework). Constrained decoding achieves 96% syntax error reduction. Is the graph vision solving a real problem or an aesthetic preference?

### 6.3 Our Verdict

**Graphs win, but not because of tokens or performance. Graphs win because of gap-driven elicitation.**

The token comparison is a wash (~385 tokens either way). The performance improvement matters but isn't transformative. What IS transformative is the elicitation model:

On text, the programming cycle is: generate → compile → read errors → regenerate. The compiler is a gate: pass or fail.

On graphs, the programming cycle is: construct → verify incrementally → fill gaps → verify → repeat. The compiler is a navigator: "here's what's missing, here's what would satisfy it." Every gap is typed, contextual, and actionable.

This is Sigil's most novel idea -- that structural incompleteness generates questions. On text, this is awkward (parse errors → reconstruct what's missing → generate English questions). On graphs, it's natural (unconnected port → typed gap → the gap IS the question).

**But be clear about the costs.** Graph-native requires building infrastructure that doesn't exist: graph server, MCP integration, visual tools, graph-aware version control. Text has a 50-year ecosystem. The graph vision must EARN its place by delivering measurably better elicitation, not by being theoretically elegant.

**The pragmatic path:** Ship text v2 first (it's nearly free -- the grammar is designed). Build the graph construction API as an MCP server alongside it. Let both coexist. If the graph API produces measurably better programs (fewer rework cycles, fewer structural errors, faster iteration), the ecosystem will follow. If it doesn't, text v2 is a solid foundation.

---

## 7. Recommended Path Forward

### 7.1 Phase 1: Foundation (Internal Graph IR) — **built**

**What it is:** the compiler builds and retains a full program graph (not just per-behavior CFGs). Internal only — no new input formats. This phase is implemented (`compiler/src/graph/`, run on every compile).

**Done:**
- Builds the complete program graph after parsing all `.beh` files
- Cross-behavior edge-level validation — the **outputs-consumed** half (`E0511`)
- Transitive guarantee checking (transitive `pure`)
- Dependency-cycle detection, and dependency-hash-pin verification

**Still open:**
- Replace the exponential `all_paths()` with the polynomial `analyze_data_flow()` (written, still dead code)
- Full **input-sourcing** over explicit edges (`graph/gaps.rs::find_gaps` is built + tested but unwired on V1)

**Impact:** closed the biggest verification gaps and benefits all users regardless of graph-native authoring.

### 7.2 Phase 2: Text v2 Grammar

**What it would be:** a denser v2 text grammar with structured control flow (the [v2 grammar candidates](v2-grammar-candidates.md) collect the options; none is chosen or built).

**Sketch:**
- The v2 lexer/parser changes from v2-grammar-candidates.md
- Publish EBNF/GBNF grammar files for constrained decoding
- Maintain backward compatibility with v1 `.beh` files

**Effort:** Medium
**Impact:** Reduces token overhead from 8.1x to 3.3x. Enables constrained decoding.

### 7.3 Phase 3: Graph Construction API (MCP Server)

**What to build:** A Sigil MCP server that exposes ~10 graph construction tools.

**Concrete actions:**
- Implement the tool set from Section 4.2
- Wire tools to the internal graph IR from Phase 1
- Add gap analysis API (`get_gaps()` returns typed, contextual gaps)
- Implement graph serialization (text and binary formats)
- Publish as an MCP server

**Effort:** High
**Impact:** Enables graph-native programming. First validation of the vision.

### 7.4 Phase 4: Incremental Verification + Elicitation

**What to build:** Real-time verification during graph construction, gap-driven elicitation loop.

**Concrete actions:**
- Implement incremental dataflow analysis (IncIDFA approach)
- Build the draft/complete duality (two verification modes)
- Implement gap prioritization (blocking → completeness → optimization → documentation)
- Enable the elicitation loop: gaps → questions → AI fills → verify → repeat

**Effort:** High
**Impact:** The killer feature. Transforms the compiler from gatekeeper to navigator.

### 7.5 What NOT to Build

1. **Don't build a custom graph token type.** The Graph2Token research is too early. Let the research community advance this.
2. **Don't require visual input.** Multi-modal graph generation is not reliable enough.
3. **Don't abandon text.** Text v2 is the fallback and the universal format.
4. **Don't build a custom constrained decoder for graphs.** Tool-use achieves the same goal more reliably.
5. **Don't build a visual graph editor yet.** The API must prove itself before investing in visual tools.
6. **Don't build the binary format first.** The internal graph IR is sufficient until the API is validated. Binary serialization is optimization.

---

## 8. Open Questions

**Q1: Does tool-use graph construction actually produce better code than text generation?**
Hypothesis: yes (validation at each step prevents structural errors). Needs empirical testing. A direct comparison: same LLM, same task, tool-use vs text v2 generation. Measure: rework cycles, structural errors, time to completion.

**Q2: How many tool calls does a realistic Sigil project require?**
The experiments folder shows an HTTP relay system (~5 behaviors, estimated ~30 tool calls). A real-world microservice with 50+ behaviors: ~200+ calls. Need to test whether hierarchical decomposition (build sub-compositions, then wire) scales.

**Q3: What is the latency budget?**
Tool-use is slower than single-shot text generation. A 20-behavior composition: ~30-60 seconds via tool calls vs ~3-5 seconds via text. For interactive development, probably acceptable. For batch compilation, probably not. Need to establish expectations.

**Q4: Should the graph API expose implementation details or only composition?**
The tool-use approach works naturally for composition graphs (add_behavior, connect). For leaf implementations, the LLM still writes v2 imperative text (via `set_implementation`). A hybrid might be optimal: tool-use for composition, text for implementations. This matches the hybrid text format design.

**Q5: Can this work with open-source/local models?**
Tool-use in open-source models (LLaMA, Qwen, Mistral) lags behind proprietary models. Prompt2DAG showed Mistral Small at only 13.3% success. Text v2 as fallback is essential for these models.

**Q6: How should version control work for graph formats?**
The text serialization (`.sigil`) is version-controlled via git (line-based, diff-friendly). The binary format (`.sigil-bin`) is a build artifact. But if the graph API is the primary authoring path, what's the "source" file? Answer: the graph text serialization is generated FROM the graph and committed. This is analogous to lock files -- generated, but committed.

**Q7: How do graph fragments work for incremental elicitation?**
A graph fragment is a partial graph that can be merged into an incomplete graph. Fragments are the atomic unit of AI-human collaboration. Without fragments, the AI must regenerate the entire graph each time a gap is filled. With fragments, the AI generates only the missing piece.

**Q8: Is the hybrid text format (graph for compositions, imperative for implementations) the right split?**
The representation designer found that graph text wins for compositions but loses for implementations. This matches the structural nature of each. But having two sub-syntaxes in one format adds complexity. Need to test whether LLMs handle the mode switch naturally.

---

## 9. Appendix: Size and Token Comparisons

### 9.1 Hello World

| Format | Tokens | Bytes |
|--------|--------|-------|
| Python equivalent | ~15 | ~60 |
| v1 Sigil | ~65 | ~260 |
| v2 Candidate B | ~40 | ~160 |
| Graph Text (this design) | ~45 | ~180 |
| JSON equivalent | ~120 | ~480 |
| Graph Binary | -- | ~280 |

### 9.2 Three-Behavior Pipeline

| Format | validate-input | transform-data | process-pipeline | Total |
|--------|---------------|---------------|-----------------|-------|
| Python | ~50 | ~25 | ~40 | ~115 tokens |
| v1 Sigil | ~480 | ~300 | ~150 | ~930 tokens |
| v2 Candidate B | ~170 | ~100 | ~115 | ~385 tokens |
| Graph Text | ~155 | ~95 | ~135 | ~385 tokens |
| JSON | ~450 | ~280 | ~350 | ~1,080 tokens |
| Graph Binary | ~444 B | ~380 B | ~532 B | ~1,356 bytes |

### 9.3 Key Observations

1. **Graph text is token-neutral with v2 Candidate B.** The savings and costs redistribute: compositions save tokens (explicit edges vs call+error patterns), implementations are similar, graph framing adds overhead.

2. **The real win is structural, not token-based.** The graph format separates topology from computation, making it machine-processable. Diffing shows structural changes. Incomplete graphs are expressible. Verification operates directly.

3. **JSON is 3x worse** than both text formats, confirming the grammar study's finding. JSON's verbosity (quotes, braces, colons, commas) is substantial overhead.

4. **Binary format is compact.** ~1,356 bytes vs ~1,540 bytes of text (385 tokens × ~4 bytes/token). 12% smaller and infinitely faster to parse (zero-copy).

---

## 10. Appendix: Incomplete Graph Example

The following shows how an incomplete graph looks in the graph text format -- this is the elicitation interface:

```
graph process-pipeline : composition
  contract
    in raw bytes 4096
    in raw_length int 8
    out processed bytes 8192
    out processed_length int 8
    err stage int 4
    err code int 4
    err message string 256
    requires validate-input@a3f7c910
    guarantees writes_output
  hash ???

  node $1 behavior validate-input@a3f7c910
  node $2 placeholder transform
    in data bytes 4096
    in length int 8
    out result bytes 8192
    out result_length int 8

  edge $.raw -> $1.raw : data
  edge $.raw_length -> $1.raw_length : data
  edge $1.cleaned -> $2.data : data
  edge $1.cleaned_length -> $2.length : data
  edge $2.result -> $.processed : data
  edge $2.result_length -> $.processed_length : data

  dangling $1.error.code -> ? "How should validation errors be reported?"
  dangling $1.error.message -> ? "Should error messages be passed through or translated?"
end
```

**Generated elicitation questions (from `get_gaps()`):**
1. "Node $2 is a placeholder. Specify a behavior that accepts `data` (bytes/4096), `length` (int/8) and produces `result` (bytes/8192), `result_length` (int/8)."
2. "How should validation errors be reported? (Dangling error port: `code` int/4 from validate-input)"
3. "Should error messages be passed through or translated? (Dangling error port: `message` string/256 from validate-input)"

Each question is typed and structural. The AI does not need to guess what is needed -- the graph TELLS it.

---

## 11. Appendix: Sources and References


### Project Documents
- The experiment narrative (absorbs the earlier Retrospective): `docs/THE-SIGIL-EXPERIMENT.md`
- Theoretical Foundations: `docs/theoretical-foundations.md`
- V2 Grammar Candidates: `docs/v2-grammar-candidates.md`
- Language Reference: `docs/SIGIL-LANGUAGE-REFERENCE.md`

### External References

**Structured Output and Constrained Decoding:**
- JSONSchemaBench (arXiv:2501.10868) -- 10,000 schema evaluation
- Correctness-Guaranteed Code Generation (arXiv:2508.15866) -- context-sensitive constrained decoding
- Type-Constrained Code Generation (arXiv:2504.09246) -- PLDI 2025, >50% error reduction
- SynCode (arXiv:2403.01632) -- 96% syntax error reduction
- XGrammar (arXiv:2411.15100) -- 100x speedup for grammar-constrained decoding

**Tool-Use and Agent Reliability:**
- Anthropic Advanced Tool Use -- Opus 4.5: 88.1% MCP accuracy
- Prompt2DAG (arXiv:2509.13487) -- 78-93% DAG generation success
- BFCL (Berkeley Function Calling Leaderboard) -- multi-model comparison

**Graph Generation:**
- LLM-Based Multi-Agent Graph Generation (ACL Findings 2025) -- 100K node generation
- Graph2Token (arXiv:2602.01771) -- graph topology encoding, February 2026
- ComfyUI-Copilot (arXiv:2506.05010) -- 85K+ queries, production graph generation

**AI-Native Languages:**
- Dana (AI Alliance, June 2025) -- Python-like, `reason()` calls, NOT graph-native
- MoonBit (ACM LLM4Code 2024) -- AI-friendly text syntax, NOT graph-native

**Verification and Graph Theory:**
- IncIDFA (ACM POPL 2025) -- incremental dataflow analysis, 11x speedup
- Verigraph -- graph rewriting verification
- Port Graph Rewriting (Fernandez & Kirchner) -- port graphs with explicit connection points

**Graph Formats Analyzed:**
- DOT (Graphviz), GraphML, Protocol Buffers/FlatBuffers, WASM binary/text duality, MLIR, TensorFlow SavedModel/ONNX, Unreal Engine Blueprints, Apache TinkerPop, Mermaid/D2

---

