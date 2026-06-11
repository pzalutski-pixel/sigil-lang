# Designed for the AI Author

**A contract-first programming model for machine-written code**

## Abstract

For decades, programming languages have layered abstraction upward, each layer accommodating the limits of a *human* author — names to aid memory, syntax to aid the eye, garbage collection and dynamic typing to spare attention. That the author was human was the one assumption never examined, because it never changed. It is changing now: AI writes a growing share of code, with a different profile — a large but bounded context window, no memory across sessions, high in-session recall, no fatigue.

This document describes the programming model Sigil bets on for that author. A program is a graph of **behaviors**; each behavior is a *contract* — its inputs, outputs, declared effects, and guarantees — paired with an implementation the compiler checks against it. The model's central property is **structural completeness**: when every output is consumed, every input sourced, every branch handled, and every resource paired, a specific class of bugs is excluded by construction. That exclusion is real *within* a behavior, and — as of June 2026 — *across* a composition too: the cross-behavior outputs-consumed check (every CALL output routed or explicitly discarded) is now enforced (§5.3). The program is built and verified as a content-addressed graph today; what remains is materializing a composition's wiring as explicit graph edges so the full gap detector runs over them rather than the implicit wiring V1 reconstructs from text (§10). Read the other way, the same incompleteness becomes a requirements signal — each structural gap is a question that must be answered.

The model is a synthesis of established ideas — design-by-contract, effect systems, dataflow, and linear/affine types — assembled for a new author, with one less-common element: gap-driven elicitation. It is deliberately bounded. It guarantees *structure*, not *semantics*. It is a programming model, not a theory of computation. And the compiler enforces a subset of it today; where design intent and implementation differ, [STATUS.md](STATUS.md) is authoritative. What follows is the reasoning, the prior art it rests on, what it cannot claim, and the questions that remain open.

---

## 1. The shifting author

Computing has spent decades stacking abstraction layers, each putting more distance between the author and the machine:

```
machine code → assembly → C → managed (JVM, .NET) → dynamic (Python, JS)
```

Every step traded execution efficiency for *authoring comfort*. Assembly gave names to opcodes. C gave structure and portability. Managed runtimes removed manual memory management. Dynamic languages removed static types and ceremony. The direction was always up, toward convenience, and the reason was always the same, though it usually went unsaid: a human was writing the code. Variable names, formatting, garbage collection, dynamic typing — these are not computational necessities. They are accommodations to human cognition: limited working memory, imperfect recall, fatigue.

That constant — a human author — held for every layer, so it was never questioned. It is now changing. AI writes a growing share of code, and its constraints differ: a large but bounded context window, no memory across sessions, but high recall within a session and no fatigue. The question Sigil starts from is therefore narrow and, we argue, fair: if the author is no longer human, is *more convenience* even the right direction — and what does a language shaped for an AI author look like?

Sigil is one bet on the answer, and [testing it](THE-SIGIL-EXPERIMENT.md) complicated the bet in instructive ways. The rest of this document is the model that bet rests on.

## 2. Sugar versus compression

"High-level" bundles two different things that this model insists on separating.

- **Ergonomics** — descriptive names, formatting, comments, familiar punctuation. These serve a *reader's* cognition. An AI author, with recall and no fatigue, needs them least.
- **Expressive density** — `a + b*c` instead of six load/multiply/store steps; `if` and `while`; functions; structured data. This is not for the eye. It is *compression*: the same computation in fewer symbols, whoever reads it.

The intuitive move — "an AI author needs no conveniences, so strip them" — collapses these together. Sigil's first design made exactly that move and stripped all the way down to register-level primitives. That was half right. Removing ergonomics is defensible; removing density is not, because density is compression and an AI's scarcest resource is its context window. The experiment showed this directly: register-level Sigil cost several times more tokens than the languages the model already knew — syntax functions, for a language model, as a compression algorithm. The corrected principle is therefore not "less abstraction for AI" but: **strip the ergonomics, keep the compression** — a different and harder target than going to the metal.

**Alternatives weighed.** The same reasoning ruled out the obvious off-the-shelf forms before settling on a flat, line-based, contract-first one. *JSON* costs roughly 2–3× the tokens, and its unordered objects fight the ordered `INPUT`/`OUTPUT` that contract hashing depends on. *A Python-like surface* — the most-trained option — was rejected for the reason that makes it trained: general-purpose flexibility (many valid spellings, implicit state) is exactly what makes a model err, where a rigid form does not. *Emitting LLVM IR directly* skips the layer that is the point — handles instead of raw pointers, contracts and guarantees, hash-versioned composition. *S-expressions* buy homoiconicity Sigil never uses (no macros, no eval) at the cost of deep delimiter nesting, a top LLM error source, where the flat line form recovers error-by-error line by line. Each path led back to the same place: the value is in the contract-first graph above the metal, not in borrowing a host syntax.

## 3. Programs as graphs

Text is not where computation lives. Compilers are graph machines: source becomes an AST (a tree of nodes), then an intermediate representation — and modern IRs *are* graphs. Control-flow graphs, SSA def-use graphs, and Cliff Click's "sea of nodes" (the IR inside the JVM's C2 compiler) are all explicit graph forms; LLVM, MLIR, and GCC's GIMPLE are graphs throughout. Node-and-edge programming is a paradigm in its own right: dataflow languages, Unreal Blueprints, Houdini, and the computation graphs of TensorFlow and PyTorch.

"A program is a graph of behaviors" is therefore not a novel claim to defend — it is the substrate the field already runs on, exposed rather than hidden. In Sigil it is exposed all the way to the surface and it is what ships: a behavior **is** a node, its contract's inputs and outputs **are** typed ports, and its `REQUIRES@hash` dependencies plus its composition's output-to-input wiring **are** edges. The compiler does not recover a graph from opaque source as an afterthought — it builds the program's behavior graph directly and checks it *as a graph* (`graph/` module; transitive purity, circular-dependency, and dependency-hash-pin checks run over it, §7). The author reasons in topology; the compiler operates on that same topology; the content-addressed contract is the node's identity.

One point about the present form: today's language models emit tokens, so the graph is *written and read as a line-based text serialization* — each line a node, its references the edges — rather than constructed through direct validated graph steps. That is an **authoring interface** detail, not the nature of the artifact: the compiler builds and verifies the same content-addressed graph either way. A graph-native authoring interface (the model mutating the graph through validated steps instead of typing its serialization) is a forward ergonomic, not a missing foundation (§10).

## 4. Behaviors and contracts

The unit of a Sigil program is a **behavior**: a contract paired with an implementation that fulfills it.

```
BEHAVIOR parse-http-request
CONTRACT
  INPUT  raw     bytes 8192
  OUTPUT request bytes 1024
  OUTPUT error   int 4
  GUARANTEES writes_output
```

### 4.1 The contract as the unit

Rather than "here is a function, and here is a contract describing it," the contract is the behavior and the implementation merely satisfies it. The consequence is practical, not philosophical: incompleteness becomes *structural*. A behavior with an unsourced input is not under-documented; it is structurally unfinished. The closest familiar analogy is the C++ header — a header is a contract and the `.cpp` an implementation — except that C++ headers are *incomplete* contracts, declaring signatures and nothing about memory, effects, or guarantees. A behavior's contract is meant to declare all of it.

### 4.2 Transparency, not statelessness

Contract-first does not mean pure-functional. A behavior may touch state or use a capability — it must only *declare* it. Purity is one optional guarantee, not a requirement; real systems have state, and the model handles state by requiring its declaration rather than forbidding its existence. The rule is that nothing is hidden: a behavior with undeclared state access is not poorly documented, it is invalid.

### 4.3 Leaves and composites

Behaviors come in two kinds. **Leaf** behaviors do real work, built from primitive operations; they hold the only algorithmic code in the system. **Composite** behaviors add no computation — they wire other behaviors together, output to input. A whole program is then a graph: leaves computing at the bottom, composites connecting above, the same model at every scale.

The scaling is literal. Primitives compose into leaves (`string-concat`, `parse-int`); leaves into components (an HTTP parser); components into services (a web server); services into systems (several servers, a database, a queue). At every rung the unit is still a behavior with a contract, composition is still output-to-input wiring, and the same completeness checks still apply — there is no separate theory of "architecture" distinct from "programming," it is behaviors all the way up. The payoff is where complexity lands: algorithmic complexity — the kind that can be *wrong* — is quarantined in leaves, while everything above is wiring, whose mistakes are structural and so are exactly the mistakes completeness checking can catch.

### 4.4 Identity by contract hash

A behavior's identity is a hash of its contract, not its name or location. Two behaviors with identical contracts are interchangeable; a changed contract is a changed hash. This is also how the model copes with an AI's lack of cross-session memory: a dependency is pinned as `parse-http@a3f8c2` — read the contract, see the hash, know the exact interface; if the dependency changed, the hash changed, and the compiler reports the mismatch without anyone needing to remember the implementation. This part is implemented.

### 4.5 Composition, and errors as ordinary outputs

A composite behavior is the wiring made concrete. It receives inputs, calls other behaviors, routes each output somewhere, and branches on the ones that report failure:

```
BEHAVIOR http-server          # CONTRACT / HASH / REQUIRES elided for brevity
COMPOSITION
  CALL receive connection -> raw
  CALL parse-http-request raw -> request parse_err
  BRANCH parse_err  bad-request  routing
  LABEL routing
    CALL route-and-handle request -> response route_err
    BRANCH route_err  server-error  respond
  LABEL respond
    CALL send-response connection response
    JUMP done
  LABEL bad-request
    CALL send-error connection code-400
    JUMP done
  LABEL server-error
    CALL send-error connection code-500
  LABEL done
END
```

Errors are not a separate mechanism here; they are ordinary outputs. A behavior that can fail declares an error output beside its result, and — because the model requires every output to be consumed (§5) — that error must be routed somewhere: handled, propagated, or converted. *In the model*, the silently swallowed exception has no place: an unconsumed error output leaves the graph structurally incomplete. As of June 2026 the V1 compiler enforces this for compositions: an unconsumed output — an unrouted error included — does not compile; it must be handled, propagated, or explicitly discarded (§5.3, error E0511). Enforcement still says nothing about whether the handler is *correct* — only that one exists and is wired.

## 5. Structural completeness

This is the model's central property, and the one most easily overstated.

### 5.1 What a complete graph excludes

A behavior graph is *structurally complete* when every output is consumed, every input is sourced, every branch is handled, and every resource is paired — whatever is opened is closed, whatever is allocated is freed. When that holds, an entire class of bugs is excluded by construction: unhandled error outputs, ignored return values, resource leaks, dangling data, unhandled cases — together with the tests one would otherwise write to catch them. One does not test "is the error path handled?" because an unhandled error leaves the graph incomplete and it will not build. This is the move a type system or Rust's borrow checker makes: a category of errors is pushed out of runtime and testing, into the structure, at build time.

The incomplete case is the dual: an incomplete graph does not merely fail — its gaps *are* the program's logical holes, surfaced as concrete questions ("this error goes nowhere," "this input has no source," "this branch is unhandled") before anything runs. §6 develops this.

### 5.2 The line: structural, not semantic

Structural completeness is not semantic correctness. A fully wired graph of *wrong* behaviors is still a wrong program. A complete graph proves the scaffolding is sound — every slot filled, every case routed — and proves nothing about whether the logic in each slot is right. The claim is therefore not "a complete graph is a correct program," and it is not "testing is unnecessary." It is narrower:

> A complete graph does not remove testing; it narrows it. The structure is guaranteed, so testing covers only what needs human judgment — whether the logic does the right thing. One tests *meaning* on a skeleton that is guaranteed wired.

A concrete way to see the line: suppose a behavior needs a `user-id` input and nothing supplies it. That is a structural gap, and an automated system could close it by wiring in a random-number generator — the types match, the input is now sourced, the graph is complete. Structurally valid; semantically nonsense. The gap was real and the fix satisfied the structure, but whether a random number is the *right* source for a user ID is a question of intent, which lives in a human's head and not in any graph. This is precisely the work the model hands to the machine (find and close gaps) and the work it cannot (decide whether the closure means the right thing).

### 5.3 What Sigil enforces today

The compiler enforces node-level completeness — within a behavior, outputs written on all paths and balanced allocation — and, as of June 2026, the **outputs-consumed** half of edge-level completeness: in a composition, every output a CALL produces must be consumed (routed to a CALL, branched on, or written to an OUTPUT) or explicitly discarded, else it does not compile (error E0511). Inputs-sourced is likewise enforced, via CALL argument checking. What is *not* yet built is the graph-native authoring interface (§10), on which these same checks would operate over an explicit graph instead of text-reconstructed wiring — the checks themselves now hold both within a behavior and across a composition. Today, incomplete nodes and unconsumed composition outputs alike do not compile.

## 6. Gaps as requirements

Extended one step, the completeness property stops being only about checking programs and becomes a way to *specify* them.

### 6.1 Incompleteness as a question

An incomplete behavior graph is a specification in progress, and each structural gap is a precise question. The kind of gap determines the kind of question:

| Structural gap | The question it asks |
|----------------|----------------------|
| Input with no source | Where does this data come from? |
| Output nothing consumes | What happens to this result? |
| Undefined behavior | What does this do — what is its contract? |
| Unknown size | How large is this, and what if it is exceeded? |
| Unhandled branch | What happens in the other case? |
| Unpaired resource | When is this released? |

The structure generates the questions; there is no requirements checklist to remember, and "the spec is done" has a mechanical meaning — no gaps remain.

Used this way, requirements-gathering becomes a loop with no intuition in it: (1) start from a root behavior that states the goal; (2) scan the graph for gaps; (3) turn each gap into its question; (4) put the questions to whoever holds the intent; (5) fold the answers back in, which defines new behaviors and usually exposes new gaps; (6) repeat until none remain. The analyst's skill — knowing what to ask next — is replaced by reading it off the structure. When the questions stop, the specification is complete, not because someone judged it so but because there is nothing left unsourced, unconsumed, or undefined.

### 6.2 Why elicitation terminates

A natural worry is infinite regress: could answers introduce new gaps forever? *Downward*, they cannot, for an architectural reason rather than a heuristic one. Every chain of decomposition bottoms out at the floor the environment provides — the primitive operations, and the NATIVE behaviors the runtime implements for system access. One can ask how a behavior is built until the answer reaches that floor, and a floor element has no further decomposition within the model.

The regress is real-looking but shallow. Ask how data is stored, and the answer might be "in Redis"; ask how Redis is reached, "open a socket"; ask how the socket is opened, and the answer is `socket` — a NATIVE behavior, declared in Sigil but implemented in the runtime rather than composed from other behaviors. There the chain stops, because a NATIVE behavior, like a primitive, is a floor the environment provides rather than something the model decomposes. Floor elements are the axioms: they have contracts but no Sigil implementation to open up.

What this bounds is *depth*, not *breadth*, and the distinction matters. No single chain of "how is this built?" descends forever — each terminates at the floor, so elicitation never recurses infinitely downward. It does **not** follow that a specification is small or that the *number* of questions is bounded: a complex system can require many behaviors, answering one gap routinely exposes new ones at the same level, and how many questions that takes scales with the problem, not with the mechanism. So the claim is the narrower one — the loop has no infinite *descent*, and "done" has a mechanical meaning (no gaps remain) — not that the total work is bounded in advance.

### 6.3 Libraries as pre-answered questions

A library, in this framing, is not merely reusable code — it is a collection of questions already answered. "How do I read a file? what calls? what errors?" arrive answered, complete with contracts, so the model does not ask them again. A rich standard library collapses large regions of the question space before elicitation begins, and a domain library collapses domain-specific regions. Library design becomes, in part, the work of pre-answering the questions future programs would otherwise raise.

This use of structural incompleteness as a mechanical requirements signal, with a guaranteed termination floor, is the part of the model that is more than an assembly of existing parts. It is also the least proven: a direction supported by argument, not yet by evidence.

## 7. What can and cannot be guaranteed

The model restricts its guarantees to what a compiler can actually check, and both sides matter.

**Checkable**, in principle, given the model's restrictions: output completeness on all paths, balanced allocation, declared-effect transparency, and — now that the outputs-consumed edge check exists (§5.3) — structural completeness across a composition. Because the language has no higher-order functions, no recursion, and no dynamic dispatch, these checks are decidable rather than heuristic. The expressive restriction is what buys the decidability.

Several of these guarantees are also meant to **compose along the graph**: two pure behaviors wired together are pure, two that never allocate compose into one that never allocates, so a system assembled from guaranteed parts inherits the guarantee. Not every property composes so cleanly — a bound on execution time does not, since sequencing and branching combine path lengths in ways simple addition does not capture — and the model exposes the ones that do, leaving the rest to other methods. Purity now composes along the dependency graph: a pass enforces it transitively, so a `pure` behavior that depends on a declared-non-pure one is rejected. One limit, though: the *leaf* purity check rejects a behavior that *declares* `MEMORY` — so the obvious hole, a visibly stateful behavior passing as `pure`, is closed — but it does not force a behavior that merely *uses* a pattern-`MEMORY` handle to declare it, so an undeclared touch of persistent state could still slip through, and transitivity would propagate that. Transitivity is real and the declared-`MEMORY` case is caught; the residual gap is undeclared pattern-`MEMORY` use ([STATUS.md](STATUS.md)).

**Not checkable, and not claimed:** termination (the halting problem is undecidable; a behavior may loop forever and no static check always detects it), complexity bounds, and **correctness** — whether the algorithm computes the intended thing. These remain the province of testing, review, and human judgment.

The second list is as important as the first. On the first list, transitive purity and circular-dependency rejection are enforced by a whole-graph pass; the termination check is a real decidable structural check (every block reachable from entry can reach an exit — not the undecidable semantic property above); and port-level completeness — every output consumed — is enforced both inside behaviors and across a composition ([STATUS.md](STATUS.md)).

## 8. Related work

Every mechanism here has antecedents, and locating them precisely is part of the argument.

- **Contracts** descend from design-by-contract (Eiffel) and Hoare logic — inputs as (roughly) preconditions, outputs as postconditions. The difference is one of emphasis: the contract is treated as the primary artifact, not an annotation on existing code.
- **Declared effects** are an effect system (cf. Koka; Java's checked exceptions) at the behavior level — a closed, finite vocabulary of effects, checked rather than inferred.
- **Output-to-input wiring and completeness** are dataflow and process-calculus ideas; "all outputs consumed" is a relative of deadlock-freedom in process calculi.
- **Resource pairing** (open/close, alloc/free, use-once) is linear and affine typing, as in Rust's ownership model — expressed at the behavior level rather than the type level.
- **Gaps as questions** (§6) has a direct antecedent that should be named: *typed holes* and *hole-driven development* — the `_` of GHC and Agda, the holes of Idris, and Hazel's "live programming with holes," where an incomplete program is a first-class object and the type of each hole tells you what is needed to fill it. The difference here is the level: Sigil's gaps are *structural*, at the graph/requirements layer (an unsourced input, an unconsumed output, an undefined behavior), not type-driven term holes inside an expression — and they are proposed as a requirements-elicitation mechanism rather than an editor affordance. The idea is more a relocation of typed-hole thinking to the composition layer than a new one.

Two things are assembled rather than borrowed: the combination of all of the above for a machine author, and the use of structural incompleteness as a requirements signal at the graph level (§6). Neither is a new account of computation, and the model does not need to be one to be useful.

## 9. Open questions

- **Expressiveness.** How naturally do real systems — a typical web application, a microservice architecture — map onto behavior graphs? The practical limits of expressiveness are not yet characterized.
- **Elicitation in practice.** The termination argument is architectural, but whether gap-driven elicitation produces a *sensible* sequence of questions on real problems — rather than a technically-finite but unhelpful one — is untested. This is the central empirical question the model rests on.
- **Completeness-check complexity.** The completeness conditions are graph properties (path coverage, resource pairing, output consumption); these should be polynomial, but precise bounds matter for tooling at scale.
- **Deadlock.** The model can express circular waits on channels. Could additional structural constraints exclude deadlock by construction, as completeness excludes leaks? This appears hard.
- **Decidability under extension.** The decidability in §7 depends on excluding higher-order functions, recursion, and dynamic dispatch. Which of these could be reintroduced, and how much checking survives, is open.

## 10. Directions

This is a first iteration, and its measurements point the same way the arguments do — these are the next things to build, not gaps in a finished thing:

1. **A denser surface syntax** — expressions and ordinary control flow restored on top of the primitives, lowering unambiguously to the same checked graph. This addresses §2: ergonomics stay gone, compression returns. The measurement discipline proposed for it — density in the authoring model's token space, contracts explicit, implementations dense — is [the V2 density pillar](v2-density-pillar.md).
2. **A graph-native authoring interface** — the model mutating the behavior graph through direct validated steps instead of emitting its text serialization. This changes how the graph is *written*, not what it is: the compiler already builds and verifies the content-addressed graph today (§3). It is an ergonomic for a token-emitting author, not a missing foundation.
3. **Full edge-level gap detection** — the outputs-consumed half already ships (§5.3): a composition's every CALL output must be routed or explicitly discarded (E0511), and inputs are checked via CALL arguments. What remains is materializing the composition's wiring as explicit graph edges so the general gap detector (`graph/gaps.rs`, built and tested) runs over them, rather than the implicit text-reconstructed wiring V1 uses.

Of these, the structural graph and its cross-behavior checks (dependency, purity, cycles, outputs-consumed) ship today; the denser syntax, the graph-native authoring interface, and full edge-materialized gap detection are the next increments — refinements on a working foundation, not the foundation itself.

## 11. Conclusion

The argument is simple to state. Abstraction layers were always shaped by a human author; the author is changing to a machine with different constraints, so the right shape of a language — its abstraction level and its representation — is open again. Sigil's bet is a contract-first model in which programs are graphs of behaviors, structural completeness excludes a class of bugs by construction, and the same incompleteness drives requirements. The bet is bounded: it guarantees structure, not meaning; it enforces a subset of itself today; and it is a programming model built from known parts, not a new theory of computation. What the model adds is the assembly for a machine author and the gap-driven view of requirements — and the most interesting of those is also the least proven.

It is an early bet, and deliberately so. The first test of it was encouraging in the half that has to work first: a model with no training on the language learned it from the specification and wrote it *correctly* — which is what a small, explicit, regular language buys, and which is the harder thing. It was expensive in the half that was never in doubt — the token cost of a language nothing has been trained on. That is the shape of a first iteration: the direction shows signs of working, the cost is the known tradeoff to drive down next, and each pass is less a defense of this version than a way to learn what a language for a machine author should actually be.

The narrative of how the project arrived here, and what testing the bet actually showed, is in [The Sigil Experiment](THE-SIGIL-EXPERIMENT.md).

---

## Glossary

| Term | Meaning |
|------|---------|
| **Behavior** | The unit of the model: a contract paired with an implementation that fulfills it. |
| **Contract** | A behavior's declared interface — inputs, outputs, declared effects/state, and guarantees. |
| **Leaf behavior** | A behavior implemented from primitives; holds actual algorithmic logic. |
| **Composite behavior** | A behavior implemented by wiring other behaviors; contains no new computation. |
| **Structural completeness** | All outputs consumed, inputs sourced, branches handled, resources paired. Distinct from semantic correctness. |
| **Gap** | A structural incompleteness in a graph; corresponds to a requirements question. |
| **Elicitation** | Completing a specification by finding gaps and answering the questions they pose. |
| **Guarantee** | A statically checkable property a behavior declares and the compiler verifies. |
| **Contract hash** | A behavior's identity, derived from its contract; pins dependencies across sessions. |
| **Primitive** | An irreducible operation provided by the environment; the floor of decomposition. |
