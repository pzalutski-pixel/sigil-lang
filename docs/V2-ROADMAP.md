# Sigil V2 — Ideas Under Consideration

This is an **idea list for a possible v2**, not a committed plan. The directions here have been discussed, not finalized; some have partly landed already, several are open questions, and a few may not survive contact with the work. Read it as "where v1 points," not "what v2 will be." The authoritative account of what's *built* is [STATUS.md](STATUS.md); the experiment narrative that motivates the cost direction is [The Sigil Experiment](THE-SIGIL-EXPERIMENT.md).

## The one idea that matters most: a denser surface syntax

Everything in v1 is written at the register level — `LOAD`/`STORE`/`IADD`/`BRANCH` — and the experiments keep landing on the same conclusion: that verbosity, not missing batteries or model skill, is the token-cost lever. So the central v2 idea is a **denser surface syntax** — expressions and ordinary control flow in place of raw load/store/branch — that still lowers, unambiguously, to the *same* content-addressed, checked behavior graph. The bet is that this buys back tokens without giving up the safety or the from-the-spec learnability. Whether it actually does is the thing v2 has to prove.

How this objective is operationalized — density measured in the authoring model's own token space, contracts staying explicit while implementations compress, a numeric target tracked per run, and a control experiment that bounds the training-data confound — is proposed in [v2-density-pillar.md](v2-density-pillar.md).

This is the only direction the measurements directly demand. The rest below is more speculative.

## What carries over, what was oversold, what needs rework

A triage of v1, kept because the distinction is still useful:

**Sound, carries forward as-is**
- The core observation that current languages are shaped around human limits, not AI's. The experiments reinforced it (AI uses human languages because of training data, not because they fit).
- Contract-first: contracts define behaviors, implementations fulfill them.
- Content-addressed identity (SHA-256 contract hashing), the incremental-compilation cache, the lexer→parser→semantic→codegen→linker pipeline, the LLVM backend, handle-based memory. These are working and stay.

**Oversold in early write-ups, dropped**
- Framing Sigil as a "new model of computation." It's a synthesis of existing models (dataflow, actors, design-by-contract, process calculi), not a new one.
- "Ontological inversion" as a contribution — contracts-first goes back to Eiffel (1986); this is application, not discovery.
- "Bugs that cannot exist" / "the compiler proves guarantees for all inputs" — the model enforces *structural* wiring, not semantic correctness; a structurally complete program can still be semantically wrong.

**Three ideas worth keeping and developing**
1. **Gap-driven requirements elicitation.** An incomplete behavior graph generates precise questions — an input with no source, an output nothing consumes. This is the most distinctive idea and the least proven; it needs a real definition and a prototype.
2. **Elicitation terminates on finite primitives.** Decomposition bottoms out at the 43 primitives, so the "how is this built?" descent can't recurse forever. This bounds *depth*, not *breadth* (a system can still need many behaviors) — worth stating precisely, not overclaiming.
3. **One artifact for AI–human collaboration.** A contract is requirement, implementation target, and check at once. Not individually novel (Eiffel again), but the application to AI-assisted development is a real design choice — frame it as synthesis, not invention.

## Candidate features for v2 (ideas, not commitments)

- **The denser syntax above** — the EBNF, and a rule for lowering every v1 construct to the same graph. Candidate syntaxes and the token-cost reasoning are collected in [`docs/v2-grammar-candidates.md`](v2-grammar-candidates.md) — exploratory, not decided.
- **Whole-graph completeness checking** — partly landed: a graph pass (transitive purity + cycle checks) is in `compiler/src/graph/`, and cross-behavior output-consumption (`E0511`) is enforced. The open piece is port-level *input*-sourcing over the wired graph.
- **Guarantee verification, formally scoped** — pick a small set (e.g. `pure`, `writes_output`), define exactly what each means and what's decidable, and only claim what's checked. Some of this is enforced today (see STATUS); the open work is the formal scoping.
- **A gap-elicitation tool (MVP).** Analyze a partial graph, list the structural gaps in plain language, and (stretch) turn each into a question. Likely a separate tool, not part of the compiler.

## Open questions (not yet answered)

- Formal definitions for behavior / contract / composition / structural-completeness, and which checks are decidable at what cost. This is genuine prior-art-adjacent work (session types, effect systems, design-by-contract, refinement-typed languages) — positioning Sigil accurately against them is part of it, not a footnote.
- Whether a denser syntax actually recovers tokens without re-introducing ambiguity ([v2-density-pillar.md](v2-density-pillar.md) defines how to measure both sides of that trade).
- Whether gap-driven elicitation produces *useful* questions on real specs, not toy ones.
- Whether training a model on Sigil closes the cost gap (the experiments can't separate register-level verbosity from the absence of any training signal — see [The Sigil Experiment](THE-SIGIL-EXPERIMENT.md)).

## Experiment ideas to validate a v2

Framed as hypotheses worth testing, not a fixed plan — each needs a control, a measure, and success/failure criteria set in advance:

- **Token efficiency:** does v2 syntax use fewer tokens than v1 for the same programs (vs Python/C++ as references)?
- **AI generation accuracy:** do models generate more correct code in v2 syntax than v1 (compile rate, completeness, iterations)?
- **Completeness-checking value:** does structural completeness catch bugs Rust's types and Python's runtime miss?
- **Gap-elicitation usefulness:** do the generated questions, answered, lead to complete specs?

The standing methodology — fresh AI-coder agents per condition, isolated, built and run end-to-end, byte-exact acceptance — is in [experiments/](../experiments/). A negative result here is a real result, not a failure to report.
