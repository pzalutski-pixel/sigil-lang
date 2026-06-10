# Theoretical Foundations of Sigil

**Formal Definitions, Decidability Results, and Prior Art Positioning**

This is the design record for Sigil's theoretical basis: the formal model, why each
claimed check is decidable, and where each mechanism sits in prior work. The formal
content stands as written; implementation-status statements throughout are current
and defer to [STATUS.md](STATUS.md) where they ever differ.

---

## Executive Summary

This document records the formal definitions, decidability results, and prior-art positioning for the Sigil language: where each mechanism comes from, what is and isn't novel, and which checks are decidable and why.

**The assessment:** Sigil combines ideas from at least six distinct lineages of computer science research. Every individual mechanism has clear antecedents. The innovation lies in a specific combination -- contracts-as-identity + explicit composition graphs + compile-time guarantee verification + untyped bytes with interpretations + native compilation -- and in the AI-generation-oriented design philosophy. No single prior system combines all of these, but none of the individual pieces are new.

**The core design achievement:** Sigil's language restrictions (no general recursion, finite CFGs, no higher-order behaviors, no dynamic dispatch, handle-based memory without pointer arithmetic) are precisely chosen to make all claimed compile-time checks decidable. The language design does the heavy lifting, not the verification engine.

**Critical findings:**
1. There is a **direct terminology collision** with Harel's Behavioral Programming (2012 CACM) that must be addressed.
2. Sigil's guarantees are a **first-order, closed-vocabulary effect system** -- simpler than algebraic effects but non-compositional across CALL boundaries.
3. Sigil's contracts are **degenerate binary session types** (the "receive-all-then-send-all" fragment).
4. The structural checks are decidable but were staged in implementation. Implemented and wired: the **outputs-consumed** half of edge-level validation (`E0511`), **transitive purity**, **circular-dependency** rejection, **dependency-hash pins**, and a decidable **`all_paths_terminate`**. The one piece still open is *full* graph-level **input-sourcing** over an explicit edge graph (built in `graph/gaps.rs`, unwired on V1; inputs are checked at the CALL-argument level instead). See [STATUS.md](STATUS.md).
5. All claimed properties **are decidable** given Sigil's restrictions -- this is formally justified.

---

## Part 1: Formal Definitions

### 1.1 Preliminary Definitions

Let **Name** be the set of valid Sigil identifiers (matching `[A-Za-z_][A-Za-z0-9_-]*`).

Let **Hash** = {s in {0..9, a..f}^8} be 8-character lowercase hexadecimal strings.

Let **Interp** = {int, float, bytes, string} be the set of interpretation tags.

Let **Size** = N+ (positive naturals), with well-formedness:

```
WF_size : Interp x Size -> Bool
WF_size(int, s)   = s in {1, 2, 4, 8}
WF_size(float, s) = s in {4, 8}
WF_size(bytes, s) = s > 0
```

`string` is the exception: it is **self-describing** (length-prefixed, §2.5) and
carries no `Size` in a port — a `string` port is the pair (n, string). The size-
bearing definitions below range over {int, float, bytes}.

Let **Guar** = {pure, no_alloc, writes_output} be the set of guarantee tags.

A **port** is a triple (n, tau, s) where n in Name, tau in Interp, s in Size, and WF_size(tau, s) holds.

### 1.2 Behavior

**Definition (Behavior).** A behavior B is a 6-tuple:

```
B = (I, O, R, M, G, Impl)
```

where:

- **I** = (i_1, ..., i_m) is an ordered sequence of input ports with distinct names
- **O** = (o_1, ..., o_n) is an ordered sequence of output ports with distinct names
- **R** subset Name x Hash is a finite set of required behavior references
- **M** subset Name is a finite set of accessible pattern memory names
- **G** subset Guar is the set of declared guarantees
- **Impl** in {Leaf(P*), Composite(C), Native} is the implementation

**Assumption (Name disjointness).** names(I) intersect names(O) = emptyset. Enforced syntactically but not stated in the language reference. Without this, contract hashing becomes ambiguous.

**Assumption (Acyclic dependencies).** The dependency relation D, where (B1, B2) in D iff B2's name appears in B1's REQUIRES, has an irreflexive transitive closure. This prevents circular compilation dependencies and is critical for decidability (see Part 3).

### 1.3 Contract and Identity

**Definition (Contract).** The contract of B = (I, O, R, M, G, Impl) is the projection:

```
contract(B) = (I, O, R, G)
```

Note: M (memory declarations) is **excluded** from the contract. Memory access patterns are implementation details, not interface. Two behaviors with identical (I, O, R, G) but different M declarations have the same contract hash. This is intentional.

**Definition (Contract Hash).** The identity function H maps contracts to hashes:

```
H : Contract -> Hash
H(I, O, R, G) = truncate_4(SHA-256(serialize(I, O, R, G)))
```

where serialize is the deterministic encoding from Section 5.7 of the language reference.

**Property (Determinism).** H is a function: identical contracts produce identical hashes.

**Property (Collision bound).** H is NOT injective. With 32-bit output, the birthday bound gives expected collision at ~65,536 distinct contracts. Adequate for single projects; insufficient for a global registry. Mitigation: use full SHA-256 for cross-project linking.

**Property (No subtyping).** Contract equivalence is nominal (hash-based), not structural. A behavior with more outputs cannot satisfy a requirement for fewer outputs. This trades flexibility for simplicity and hash-based linking.

### 1.4 Control Flow Graph

**Definition (CFG).** For behavior B, the control flow graph is:

```
CFG(B) = (V, E, v_entry, V_exit)
```

where V is a finite set of basic blocks, E subset V x V x {unconditional, true, false}, v_entry is the unique entry block, and V_exit subset V is the set of exit blocks (no successors).

### 1.5 Primitive Effect Classification

The 43 primitives have effect classes:

```
eff(p) = 'alloc'     if p = ALLOC
eff(p) = 'free'      if p = FREE
eff(p) = 'read'      if p in {LOAD, ATOMIC_LOAD}
eff(p) = 'write'     if p in {STORE, ATOMIC_STORE}
eff(p) = 'readwrite' if p in {CAS, ATOMIC_ADD, ATOMIC_SUB}
eff(p) = 'pure'      if p in arithmetic U comparison U conversion U bitwise
eff(p) = 'control'   if p in {BRANCH, JUMP}
```

---

## Part 2: Structural Completeness Predicate

This is Sigil's central claim: "incomplete graphs are not programs." We formalize it as a conjunction of four sub-predicates.

### 2.1 Master Definition

**Definition (Structural Completeness).** A behavior B is structurally complete, written SC(B), iff:

```
SC(B) = OutputConsumption(B) /\ InputSourcing(B) /\ PathTermination(B) /\ ResourcePairing(B)
```

### 2.2 OutputConsumption

**Definition.** Every output port of B must be written on every execution path to exit.

```
OutputConsumption(B) =
    forall o in O(B):
        forall pi in Paths(CFG(B), v_entry, V_exit):
            exists stmt in pi: stmt writes to o
```

This is the `writes_output` guarantee. It is a standard **must-analysis** (definite assignment) computed via set intersection at merge points.

**Ideal version:** Check only feasible paths (those reachable by some input). **Undecidable** -- path feasibility reduces to Diophantine constraint solving.

**Implemented version:** Check all syntactic CFG paths. **Decidable, sound overapproximation.** May reject valid programs where outputs are written on all feasible paths but not on some infeasible syntactic path.

**Precedent:** Java's definite assignment (JLS Chapter 16), C# definite assignment.

### 2.3 InputSourcing

**Definition.** Every variable read must be preceded by a write on all paths reaching that read.

```
InputSourcing(B) =
    forall (stmt, var) where stmt reads var:
        forall pi in Paths(CFG(B), v_entry, block(stmt)):
            exists stmt' in pi preceding stmt: stmt' defines var
```

This is a standard **reaching definitions** problem. Same decidability and complexity as OutputConsumption.

### 2.4 PathTermination

**Ideal definition (undecidable):**

```
PathTermination_ideal(B) = forall inputs sigma: execution of B on sigma terminates
```

This is the halting problem. Undecidable by Rice's theorem.

**Weakened definition (decidable):**

```
PathTermination_weak(B) =
    forall v in Reachable(CFG(B)):
        v in V_exit OR exists path from v to some v' in V_exit
```

This checks that every reachable block can reach an exit. Decidable in O(V+E) via graph reachability. Does NOT guarantee termination -- allows infinite loops as long as the loop block has some structural path to an exit.

**Important clarification:** The Language Reference (Section 13.4) states "all paths reach END or loop." This is the weakened definition, explicitly allowing loops. The Behavioral Model paper's phrasing ("every possible execution path must reach an endpoint") is misleading and should be corrected.

**Trade-off:** Weakening from ideal to implementable loses termination guarantees. Non-terminating programs are accepted if their loop blocks structurally connect to exits. This is standard practice -- most production languages (Rust, Go, Java, C) do not require termination. Only total languages (Agda, Coq, Idris) enforce it, sacrificing Turing-completeness.

### 2.5 ResourcePairing

**Definition.** Every allocation is matched by deallocation or is scoped.

```
ResourcePairing(B) =
    forall pi in Paths(CFG(B)):
        forall stmt = (h = ALLOC ...) in pi:
            InScope(stmt) OR exists stmt' = (FREE h) in pi after stmt
```

Equivalently: at every exit block, the set of allocated-but-not-freed handles (excluding scoped handles) is empty.

**Relationship to linear types:** This is a **linear type discipline** on handles. Each handle must be used exactly once (freed exactly once) or scoped. Known to be decidable and polynomial for first-order programs without higher-order functions. Sigil has no higher-order functions, so this is firmly tractable.

**Precedent:** Rust's ownership system, Linear Haskell, Clean's uniqueness types, Austral's linear types, Linear Dafny (OOPSLA 2022).

### 2.6 Decidability of SC

**Theorem.** SC(B) is decidable for finite Sigil programs with the weakened PathTermination definition.

**Proof sketch:** Each sub-predicate reduces to a finite graph property:
- OutputConsumption/InputSourcing: fixed-point dataflow (set intersection at merge points)
- PathTermination: graph reachability
- ResourcePairing: linear resource tracking over finite CFG

All are polynomial in CFG size. The lattice height is bounded by |Vars|, guaranteeing convergence.

**Complexity:** O(|V| * |E| * |Vars|) for fixed-point iteration.

---

## Part 3: Decidability Results

### 3.1 Complete Decidability Map

| # | Property | Decidable? | Complexity (Current) | Complexity (Optimal) | Key Restriction |
|---|----------|-----------|---------------------|---------------------|----------------|
| 1 | All outputs consumed (inter-behavior) | Yes | O(V+E) | O(V+E) | Finite graph |
| 2 | All inputs sourced (inter-behavior) | Yes | O(V+E) | O(V+E) | Finite graph |
| 3 | All paths terminate (structural) | Yes | O(V+E) | O(V+E) | Finite CFG |
| 3' | All executions terminate (semantic) | **No** | N/A | N/A | Halting problem |
| 4 | All resources paired | Yes | O(2^B * N) | O(V*E) | No aliasing, no HOF |
| 5 | `pure` (local) | Yes | O(N) | O(N) | Syntactic check |
| 5' | `pure` (transitive) | Yes | O(N+D) | O(N+D) | Finite call graph |
| 6 | `no_alloc` | Yes | O(N) | O(N) | Syntactic check |
| 7 | `writes_output` | Yes | O(2^B * N) | O(V*E*O) | Finite CFG |
| 8 | Bounds (static offsets) | Yes | O(1)/access | O(1)/access | Constant offsets |
| 8' | Bounds (dynamic offsets) | **No** | N/A | N/A | Rice's theorem |
| 9 | Ownership | Yes | O(N) | O(N) | Single owner |
| 10 | No use-after-free | Yes | O(2^B * N) | O(V*E) | Finite CFG |
| 11 | No memory leaks | Yes | O(2^B * N) | O(V*E) | Finite CFG |
| 12 | Initialization | Yes | O(2^B * N) | O(V*E) | Finite CFG |
| 13 | Contract type/size match | Yes | O(N) | O(N) | Finite types |
| 14 | Dependency hash match | Yes | O(R) | O(R) | Finite hashes |

Legend: B = branch points, N = nodes, V = CFG vertices, E = edges, O = outputs, D = dependencies, R = requires, HOF = higher-order functions.

### 3.2 What Makes Decidability Possible

Sigil's language design restrictions are precisely chosen:

| Restriction | What It Enables | What Would Break If Removed |
|-------------|----------------|---------------------------|
| No general recursion | Finite CFGs, bounded analysis | Termination, resource pairing become undecidable |
| No higher-order behaviors | Static call graph, contract matching | All transitive analyses become undecidable |
| No dynamic dispatch | Compile-time resolution of all CALLs | Purity, no_alloc, contract matching undecidable |
| No pointer arithmetic | Handle-based memory, no aliasing | Bounds checking, ownership undecidable |
| Handles non-overlapping | Linear resource tracking | Alias analysis required (NP-hard in general) |
| Fixed-vocabulary guarantees | Enumeration-based checking | Open effect systems require inference |

**This is the core design achievement.** The language is restricted enough that ambitious checks are all decidable, while remaining expressive enough to implement real programs (via composition with NATIVE behaviors for system interaction).

### 3.3 Implementation Status

Status of the decidable structural properties — what is enforced today ([STATUS.md](STATUS.md) is authoritative):

| Property | Status |
|----------|--------|
| All outputs consumed (inter-behavior) | **Enforced** — `E0511` (`check_outputs_consumed`, every CALL output routed or `DISCARD`ed) |
| All inputs sourced (inter-behavior) | **Argument-checked** at each CALL; the full graph-level port-sourcing check (`graph/gaps.rs`) is built + tested but unwired on V1 |
| All paths terminate | **Enforced** — decidable reachability check (`cfg.rs::all_paths_terminate`), not a halting proof |
| `pure` (transitive) | **Enforced** — whole-graph pass (`graph/validate.rs`); residual gap: *undeclared* pattern-`MEMORY` |
| `no_alloc` (scoped) | **Enforced** — an ALLOC inside a balanced SCOPE satisfies it; only an escaping allocation is rejected |
| Exponential path enumeration | Still exponential; the polynomial `analyze_data_flow` exists but is unwired |

**Consequence:** Of the four "bugs that cannot exist," outputs-consumed (ignored return values / unrouted errors) is enforced across a composition (`E0511`); the one remaining inter-behavior gap is full *graph-level input-sourcing*, which is built (`gaps.rs`) but unwired on V1.

### 3.4 What the Completeness Claims Deliver

**Claim: "Bugs that cannot exist."** Partially justified. Use-after-free and memory leaks are correctly prevented; ignored return values and unrouted errors are now also prevented in compositions via outputs-consumed (`E0511`). The remaining edge-level gap is full graph-level *input*-sourcing (built in `gaps.rs`, unwired on V1).

**Claim: "Structural completeness is definitional."** Formally coherent as a language design choice (analogous to type soundness in typed languages), but only as strong as compiler enforcement. Now enforced for outputs-consumed plus the dependency/purity/cycle/hash-pin checks; full port-level input-sourcing over an explicit edge graph remains the unwired V1 piece.

**Claim: "The elicitation loop terminates."** Sound for implementation decomposition (primitives form a finite floor). But the claim conflates two processes: (1) decomposing a specified behavior into primitives (terminates by structural induction) and (2) eliciting requirements from a human (depends on the human providing finite answers). The termination argument applies to (1), not (2).

---

## Part 4: Prior Art Positioning

### 4.1 David Harel's Behavioral Programming (2012 CACM)

**CRITICAL: Direct terminology collision.** Harel uses "behavior" (via "b-threads") as the fundamental unit of computation. The mechanisms are fundamentally different:

| Aspect | Harel's BP | Sigil |
|--------|-----------|-------|
| Composition model | Event-based interleaving via central arbiter | Explicit dataflow graph wiring |
| Communication | Implicit via shared event space | Explicit via contract-defined I/O |
| Control flow | Emergent from request/wait/block negotiation | Explicit BRANCH/JUMP/CALL |
| Blocking | Any b-thread can globally veto events | No equivalent mechanism |
| Determinism | Non-deterministic event selection | Deterministic graph traversal |

In Harel's BP, emergent behavior arises from b-thread interaction at synchronization points -- no single thread controls the whole. In Sigil, composition is explicit and deterministic. Harel's BP is closer to constraint satisfaction; Sigil is closer to dataflow programming.

**Required action:** Either rename Sigil's "behavior" or add explicit disambiguation in all documentation. Anyone searching "behavioral programming" will find Harel's work first.

**What Sigil can learn:** Harel's blocking mechanism (vetoing events) enables safety properties Sigil currently lacks. Harel's incremental specification (adding behaviors without modifying existing ones) is a genuine advantage Sigil does not have.

### 4.2 Hewitt's Actor Model (1973)

Sigil's Pattern (MEMORY + lifecycle + SPAWN) is essentially an actor with a fixed contract:

| Actor Model | Sigil |
|-------------|-------|
| Actor with encapsulated state | Pattern with MEMORY |
| Message processing | Behavior INPUT/OUTPUT |
| Actor creation | SPAWN |
| `become` (behavior change) | **Not supported** (fixed contracts) |

The key genuine difference: actors can `become` (change behavior over time); Sigil behaviors have immutable contracts. This trades flexibility for verifiability.

**Note:** Erlang/OTP's "behaviour" modules (`gen_server`, `gen_statem`, `supervisor`) are remarkably close to Sigil's BEHAVIOR concept -- another terminology collision to acknowledge.

### 4.3 Meyer's Design by Contract / Eiffel (1986)

Sigil's CONTRACT section is a direct descendant of DbC. The INPUT declarations are preconditions, OUTPUT declarations are postconditions, GUARANTEES are invariants.

**Is the "ontological inversion" genuine?**

- **Partially genuine:** The contract hash IS the identity. Two implementations with the same contract are interchangeable by definition. This is a real structural difference from Eiffel.
- **Partially cosmetic:** Meyer himself advocated specification-first development since the 1980s. TLA+ takes it further. Sigil's "inversion" is more a matter of enforcement than paradigm shift.
- **Genuinely novel:** Content-addressed identity derived solely from the contract interface. Eiffel has no equivalent.

**Expressiveness gap:** Eiffel contracts can express arbitrary boolean conditions (`require balance >= amount`). Sigil's contracts express only structural properties (types, sizes, guarantee flags). Sigil cannot express "output value is always positive" or "output size equals input size."

### 4.4 Milner's Process Calculi (CCS, pi-calculus)

Sigil's CHANNEL mechanism is standard process calculus. CHANNEL_SEND, CHANNEL_RECEIVE, CHANNEL_CLOSE are standard channel operations with standard blocking semantics.

**Is structural completeness a form of deadlock freedom?**

Yes, partially. Within a single behavior, structural completeness subsumes deadlock freedom trivially (behaviors are sequential). But Sigil does NOT check deadlock across concurrent patterns communicating via channels. Two patterns sending to each other can deadlock, and the compiler will not detect it.

**Critical gap:** Sigil lacks formal operational semantics. Without them, claims about structural completeness cannot be rigorously verified. Process calculi provide a template.

### 4.5 Honda's Session Types

Sigil's contracts are **degenerate binary session types** -- the "receive-all-then-send-all" fragment:

```
?I_1.?I_2. ... ?I_m.!O_1.!O_2. ... !O_n.end
```

No interleaving, no choice, no recursion in the session. Each behavior invocation is a one-shot interaction.

**Critical gap:** For CHANNEL-based communication between spawned patterns, Sigil has **no session-type discipline at all**. Channels are typed by value interpretation and capacity but have no protocol type governing send/receive sequences.

**What Sigil can learn:** Session types for CHANNEL communication would enable compile-time verification of communication protocols. Multiparty session types could describe composition-level interaction patterns.

### 4.6 Additional Prior Art to Acknowledge

| System | Overlap | Key Difference |
|--------|---------|---------------|
| **Unison** | Content-addressed code identity | Unison hashes implementations; Sigil hashes contracts only |
| **Lustre/Esterel** | Dataflow composition, native compilation | Lustre processes infinite streams; Sigil processes finite data |
| **TLA+** | Specification IS the system | TLA+ is more general (temporal logic, unbounded states) |
| **Go/Rust** | Errors as values | Sigil's graph-level enforcement is a useful twist on the same idea |
| **Koka** | Effect tracking | Koka's row-polymorphic effects are far more expressive and compositional |

### 4.7 What Is Genuinely Novel

1. **Content-addressed identity derived from the contract interface alone** (not implementation). Different from Unison (which hashes implementations) and DbC (which does not hash at all).

2. **Structural completeness as a specification-completion metric for AI elicitation.** The idea that graph incompleteness drives human-AI interaction is novel. No prior system uses graph gaps as a driver for requirements gathering.

3. **The specific combination** of contracts-as-identity + composition graphs + compile-time guarantees + untyped bytes + native compilation. No prior system combines all of these.

4. **"Interpretations" as a replacement for types.** Operations determine how bytes are interpreted (IADD vs FADD) rather than variables carrying types. This is essentially structured assembly language -- genuinely different from both typed and dynamically-typed languages.

### 4.8 What Is Repackaged

| Sigil Feature | Prior Art |
|--------------|-----------|
| "Behavior" as fundamental unit | Harel (2012), Erlang behaviours |
| Contracts on interfaces | Meyer's DbC (1986) |
| Compile-time guarantee verification | Effect systems (1990s+) |
| Errors as outputs | Go, Rust errors-as-values |
| CHANNEL communication | CSP/pi-calculus |
| SPAWN/WAIT concurrency | Standard async/join |
| Handle-based ownership | Simplified Rust ownership |
| Content-addressed code | Unison (2015+) |

---

## Part 5: Verification Landscape Position

### 5.1 Where Sigil Sits

Sigil occupies the "lightweight, decidable, zero-annotation structural verification" position:

```
                    More Expressive Properties
                              ^
                              |
             F* --------+-----+------ Dafny
                        |            /
            Liquid      |           /
            Haskell ----+          /
                        |         /
               TLA+ ----+        /
                        |       /
              Alloy ----+      /
                        |     /
                Koka ---+   /
                        |  /
                 Rust --+ /
                        |/
               Sigil ---+
                        |
    Less Annotation Burden ----> More Annotation Burden
```

| System | Mechanism | Decidable | Annotation | Properties | Compositional |
|--------|-----------|-----------|------------|------------|---------------|
| **Sigil** | Syntactic + CFG paths | Always | None | Fixed structural set | **No** |
| **Koka** | Type inference | Always | None | Effect types | Yes |
| **Rust** | Ownership analysis | Always | Light | Memory safety | Yes |
| **Liquid Haskell** | Refinement types + SMT | Base: yes | Light | QF-EUFLIA | Yes |
| **Alloy** | SAT (Kodkod) | Bounded | Model spec | Relational logic | Yes |
| **TLA+** | Model checking | Bounded | Model spec | Temporal logic | Yes |
| **Dafny** | SMT (Z3 via Boogie) | No | Heavy | Arbitrary FOL | Yes |
| **F*** | Dependent types + SMT | No | Very heavy | Arbitrary | Yes |

### 5.2 The Compositionality Gap

Sigil's guarantees compose across CALL boundaries only partially:

- **`pure` now composes.** A whole-graph pass enforces transitivity — a `pure` behavior that depends, even transitively, on a non-pure one is a compile error (`TransitivePurityViolation`). (Residual gap: a behavior that *uses* pattern-`MEMORY` without *declaring* it isn't forced to declare it, so an undeclared touch of state could still slip through.)
- **`no_alloc` does not propagate** through the call graph.
- There is no general effect polymorphism, inference, or handler mechanism.

**Comparison with Koka:** Koka's row-polymorphic effect types give the same guarantees with full compositionality, automatic inference, and zero annotation. The gap between Sigil's per-behavior boolean flags and Koka's effect types propagated through the call graph is the main architectural difference — now narrowed for purity, still open for the rest. Closing it for the other guarantees is decidable (check callee contracts), the same mechanism the purity pass already uses.

### 5.3 What Sigil Gets Right

1. **Decidability guaranteed.** All checks terminate. No SMT timeouts.
2. **Zero annotation burden.** Properties derived from language structure.
3. **Explicit about its limits.** The language explicitly acknowledges it cannot verify termination or semantic correctness.
4. **Language design does the heavy lifting.** Restrictions (no recursion, no HOF, no dispatch) make properties tractable rather than requiring complex verification machinery.

---

## Part 6: Guarantee Checking -- Formal Details

### 6.1 `pure`

**Definition.** Pure(B) holds iff:
```
M(B) = emptyset                                    -- no pattern MEMORY access
/\ forall stmt in Stmts(B): not IsChanOp(stmt)    -- no CHANNEL operations
/\ forall stmt in Stmts(B): writes only to O(B) or local handles
```

**Decidability:** O(|Stmts|). Single syntactic pass.

**Limitation:** This is *syntactic* purity, not semantic purity (referential transparency). A behavior that depends on external state smuggled through inputs is accepted. The check is a sound overapproximation: if Pure(B) holds, B is syntactically pure. Some semantically pure behaviors may be rejected.

**Precedent:** D language's `pure` keyword (syntactic), Haskell's IO monad (semantic), Koka's effect system (inferred).

### 6.2 `no_alloc`

**Definition.** NoAlloc(B) holds iff every ALLOC in B is within a SCOPE block.

**Decidability:** O(|Stmts|). Syntactic nesting check.

**Note:** The name is misleading -- it means "no heap escape" or "stack-only allocation." Scoped ALLOC is permitted.

### 6.3 `writes_output`

**Definition.** WritesOutput(B) = OutputConsumption(B). Every output written on every CFG path.

**Decidability:** O(|V| * |E| * |O|) via fixed-point dataflow.

---

## Part 7: What This Actually Is

This section maps Sigil's formalisms onto well-studied frameworks — what each one is, in established terms.

### 7.1 Contracts Are Degenerate Session Types

A Sigil contract is equivalent to a binary session type with the protocol:
```
?I_1.?I_2. ... ?I_m.!O_1.!O_2. ... !O_n.end
```
Receive all inputs, send all outputs, terminate. No interleaving, choice, or recursion. This makes Sigil sessions trivially deadlock-free within a single behavior (because the protocol has no branching).

Session types would become genuinely relevant if CHANNEL operations were integrated into the contract system (which they currently are not).

### 7.2 Guarantees Are a First-Order Effect System

The guarantee mechanism maps to a simple effect system:
- Effects: {channel_op, memory_access, alloc_escape, output_write}
- `pure` asserts absence of {channel_op, memory_access, non_output_write}
- `no_alloc` asserts absence of {alloc_escape}
- `writes_output` asserts presence of {output_write on all paths}

This is closer to Java's checked exceptions (fixed vocabulary) than to Koka's algebraic effects (open, user-defined, composable). There is no effect polymorphism, no effect handlers, no effect composition.

### 7.3 Compositions Are Dataflow Graphs with Imperative Control Flow

The CALL wiring mechanism is a dataflow graph. Adding BRANCH/JUMP/LABEL makes it a hybrid:
- Dataflow connectivity determines what data flows where
- Control flow determines when each node fires

Similar to MLIR's structured control flow dialect, TensorFlow's tf.cond/tf.while_loop, and Lustre/SCADE synchronous dataflow.

### 7.4 Structural Completeness Subsumes Deadlock Freedom -- But Only Within a Behavior

Within a single behavior, SC is stronger than deadlock freedom (it adds resource pairing and output completeness). Across concurrent patterns, SC is necessary but NOT sufficient -- circular channel dependencies can still cause deadlock, and this is not checked.

---

## Part 8: Trade-off Register

Every weakening of an ideal definition loses something. This section documents each loss.

### T1: Syntactic vs. Semantic Path Analysis

| | Ideal | Implemented | Loss |
|-|-------|-------------|------|
| Paths considered | Only feasible | All syntactic CFG paths | False positives (correct programs rejected) |
| Soundness | Sound | Sound | None |
| Completeness | Complete | Incomplete | Some valid programs rejected |

Affects: OutputConsumption, InputSourcing, ResourcePairing.

### T2: Termination Approximation

| | Ideal | Implemented | Loss |
|-|-------|-------------|------|
| Property | Termination for all inputs | Structural exit reachability | Non-terminating programs accepted |
| Soundness | Sound | Unsound (accepts too much) | Infinite loops possible |

### T3: Hash Collision Risk

32-bit hash space. Birthday collision at ~65K contracts. Negligible for projects; risky for ecosystems. Mitigate with full SHA-256 for cross-project linking.

### T4: Purity as Syntactic Property

Conservative: rejects some semantically pure behaviors. Accepts behaviors that depend on state smuggled through inputs. Standard tradeoff in effect systems.

### T5: No Subtyping

Exact hash match required. No polymorphism. Trades flexibility for simplicity. Less burdensome in AI-mediated development where the AI manages all references.

---

## Part 9: Open Directions

The compiler items this analysis originally called for have landed — transitive
`pure`, the `no_alloc` scoped fix, a decidable `all_paths_terminate`, and the
outputs-consumed half of edge-level validation (`E0511`) are implemented and wired
(§3.3), and the prior-art positioning it asked for is Part 4. What remains open,
in rough order of cost:

**Compiler.** Wire the full graph-level **input-sourcing** check (`graph/gaps.rs`
is built and tested but unwired on V1 — inputs are checked at the CALL-argument
level instead), and replace the exponential path enumeration with the polynomial
`analyze_data_flow` (written but dead).

**Language design.** Add **session types for CHANNEL communication** to catch
concurrent deadlock (the one completeness gap that survives across patterns, §4.4);
and consider **richer contract expressions** — value-level assertions, not only
structural types (Sigil cannot currently say "output size equals input size," §4.3).

**Formal.** Formalize the **operational semantics** in process-calculus notation;
**prove** that structural completeness implies deadlock freedom for Sigil's
concurrency subset (or document where it does not, §7.4); and a machine-checked
**soundness proof** for the semantic analyzer (Lean/Coq).

---

## Part 10: Open Questions

**Q1: Should mutual recursion among compositions be allowed?** No. It would make PathTermination undecidable and require interprocedural resource tracking. Express recursion as loops within a single behavior.

**Q2: Should the hash include MEMORY declarations?** Consider an optional "memory fingerprint" for behaviors accessing pattern memory, to allow callers to verify memory isolation.

**Q3: What is the formal status of NATIVE behaviors?** They are axiomatically correct -- the compiler trusts the runtime. This is a trust boundary that should be documented. Consider a test harness for runtime verification of NATIVE behaviors.

**Q4: Should the language support subtyping on contracts?** Not for V1. The AI-mediated workflow reduces the need for human-convenience features like subtyping.

**Q5: How should concurrent deadlock freedom be checked?** Start with "every SPAWN has a corresponding WAIT on all paths" (a form of ResourcePairing). Full concurrent deadlock analysis via multiparty session types is a future project.

---

## References

### Primary Sources

1. Harel, D., Marron, A., and Weiss, G. (2012). ["Behavioral Programming."](https://dl.acm.org/doi/10.1145/2209249.2209270) Communications of the ACM, 55(7).

2. Hewitt, C., Bishop, P., and Steiger, R. (1973). ["A Universal Modular ACTOR Formalism for Artificial Intelligence."](https://eighty-twenty.org/files/Hewitt,%20Bishop,%20Steiger%20-%201973%20-%20A%20universal%20modular%20ACTOR%20formalism%20for%20artificial%20intelligence.pdf) IJCAI.

3. Meyer, B. (1992). ["Design by Contract."](https://se.inf.ethz.ch/~meyer/publications/old/dbc_chapter.pdf) In Advances in Object-Oriented Software Engineering.

4. Milner, R., Parrow, J., and Walker, D. (1992). ["A Calculus of Mobile Processes."](https://www.cis.upenn.edu/~stevez/cis670/pdfs/pi-calculus.pdf) Information and Computation.

5. Honda, K. (1993). ["Types for Dyadic Interaction."](https://link.springer.com/chapter/10.1007/3-540-57208-2_35) CONCUR.

6. Honda, K., Yoshida, N., and Carbone, M. (2008). ["Multiparty Asynchronous Session Types."](https://dl.acm.org/doi/10.1145/2827695) JACM.

### Verification Systems

7. Leino, K.R.M. (2010). "Dafny: An Automatic Program Verifier for Functional Correctness." LPAR.

8. Swamy, N. et al. (2016). "Dependent Types and Multi-Monadic Effects in F*." POPL.

9. Vazou, N. et al. (2014). "Refinement Types for Haskell." ICFP.

10. Lamport, L. (2002). *Specifying Systems: The TLA+ Language and Tools for Hardware and Software Engineers.*

11. Jackson, D. (2012). *Software Abstractions: Logic, Language, and Analysis.* MIT Press.

12. Leijen, D. (2017). ["Type Directed Compilation of Row-Typed Algebraic Effects."](https://www.microsoft.com/en-us/research/project/koka/) POPL.

### Type Theory and Decidability

13. Girard, J.-Y. (1987). "Linear Logic." Theoretical Computer Science.

14. Rice, H.G. (1953). "Classes of Recursively Enumerable Sets and Their Decision Problems." Transactions of the AMS.

15. Kobayashi, N. (2006). "A New Type System for Deadlock-Free Processes." CONCUR.

16. Lee, E.A. and Messerschmitt, D.G. (1987). "Synchronous Data Flow." Proceedings of the IEEE.

17. Kahn, G. (1974). "The Semantics of a Simple Language for Parallel Programming." IFIP Congress.

### Implementation References

18. [Rust Polonius Borrow Checker](https://github.com/rust-lang/polonius)

19. [Unison Language -- The Big Idea](https://www.unison-lang.org/docs/the-big-idea/)

20. [Java Language Specification, Chapter 16: Definite Assignment](https://docs.oracle.com/javase/specs/jls/se9/html/jls-16.html)

21. [How Austral's Linear Type Checker Works](https://borretti.me/article/how-australs-linear-type-checker-works)

22. [Linear Dafny (OOPSLA 2022)](https://dl.acm.org/doi/10.1145/3527313)

23. [Sound Borrow-Checking for Rust via Symbolic Semantics (POPL 2024)](https://dl.acm.org/doi/10.1145/3674640)

24. [Formulas as Processes, Deadlock-Freedom as Choreographies (2025)](https://arxiv.org/html/2501.08928)

