# V2 Design Pillar: Token-Space Density

*A proposed V2 design objective — adopted as direction, with nothing here built.
It operationalizes the cost lever every experiment run pointed at (see
[The Sigil Experiment](THE-SIGIL-EXPERIMENT.md)): the ~5× authoring-token cost
vs. Python is partly a training-data confound and partly intrinsic
register-level verbosity, and this pillar targets the intrinsic component. The
shipped language remains the v1 grammar; [STATUS.md](STATUS.md) is authoritative
for what exists.*

## Statement

> Sigil should maximize verified intent conveyed per token — measured in the
> token space of the models that write it — without sacrificing checkability or
> learnability-from-spec.

Density becomes a first-class design objective alongside checkability, not a
byproduct to recover later.

## The correction this makes to V1's premise

V1 treated human-oriented language features as overhead to strip.
[DESIGN-MODEL §2](DESIGN-MODEL.md) records the first half of the correction:
"high-level" bundles *ergonomics* (serving a reader's eye) with *expressive
density* (compression — the same computation in fewer symbols), and stripping
both minimized ambiguity at the cost of intent-per-token. That is the measured
~5×. This pillar adds the structural rule the model already implies:

**Explicitness and density are orthogonal axes, and they live in different
places.**

- **Contracts stay maximally explicit** — fully stated, hash-pinned, verbose.
  This is where checkability lives, and the experiments suggest it is also
  where cold-start learnability lives: the contracts registry is what an agent
  that has never seen Sigil navigates by.
- **Implementations get maximally dense.** This is where today's token budget
  is burned (register-level LOAD/STORE/BRANCH), and where compression belongs.

The leaf/composite and contract/implementation separations already enforce this
split structurally (E0509/E0510); the pillar applies the same quarantine to
surface syntax.

## Density, operationally — two metrics, two uses

Density is measured, not judged, and the two measurements answer different
questions:

1. **Program-text tokens** — the same benchmark programs written in each
   candidate syntax, run through real tokenizers, normalized against Python on
   the same tasks. This governs **syntax selection** (which grammar, which
   keywords, which operators) — settled empirically *before* grammar freeze.
   The token counts in [v2-grammar-candidates.md](v2-grammar-candidates.md) are
   estimates; this metric supersedes them.
2. **Authoring tokens** — per-trial cost from the experiment harness, using the
   onboarding/authoring split introduced in
   [run-5](../experiments/http-relay/run-5/RESULTS.md). This governs
   **end-to-end evaluation** of a built syntax, because it sees what program
   text cannot: rework, re-reading, and the cost of being wrong.

Tokens — not characters, not lines — because the authoring model's BPE
vocabulary decides what is actually cheap:

- Exotic glyphs (the APL endpoint) are character-dense but token-sparse: they
  fragment into multi-token sequences. Rejected by measurement, not taste.
- Conventional keywords and operators usually tokenize to single tokens,
  because the vocabulary was trained on code that contains them. Preferred —
  which converges with the evidence the grammar candidates already cite
  (lowercase keywords, GAD).
- **Measure against a basket of major tokenizers, not one.** Tokenizers differ
  across model families and drift across versions; a form qualifies when it is
  robustly cheap across the basket, not when it exploits one vocabulary's
  quirk.

This is the genuinely AI-native design criterion — no mainstream language was
ever designed against a tokenizer — and it is a sharper distinctive claim than
the original "translation tax," which testing demoted to a motivating
intuition.

## Design implications for V2

1. **Expressions over three-address code.** `len = find-null(first, 65536)`
   must replace today's multi-line ALLOC/STORE/LOAD/BRANCH pattern. Every
   eliminated mandatory line is direct density gain. (The expression layer in
   [v2-grammar-candidates.md](v2-grammar-candidates.md) is the existing
   exploration of exactly this.)
2. **Scalars are values.** A scalar INPUT should be usable directly — the
   `E0104` friction class that run-5 found is the dominant remaining
   by-design verbosity. Handles only where memory semantics require them.
3. **Immediates wherever a constant is legal — and taught as the idiom.** V1
   has always permitted them, yet AI-written code materialized constants
   through ALLOC/STORE/LOAD/FREE anyway, and that ceremony sat unnoticed in the
   stdlib's oldest string behaviors until an outside review caught it (the
   cleanup cut four behaviors from 90 lines of it to 19, contracts unchanged).
   The lesson is the pillar's premise in miniature: the gate catches
   unsoundness, not waste, so density must be carried by the syntax, the
   catalog, and the authoring skill — there is no compiler error for verbosity.
4. **Control-flow idioms as syntax that lowers to the checked graph.**
   `if`/`while`/early-exit sugar is permitted iff it lowers unambiguously to
   the same verified primitive graph — the candidates' binding constraints
   (C1–C13) are the existing statement of this. Density never bypasses a
   check.
5. **Inference into the implementation — yes; inference out of the contract —
   OPEN.** The uncontroversial half: a size or type stated once in the CONTRACT
   should not be restated in expression code (the candidates' type-inference
   design); that flows *from* the contract *into* the body and thins nothing.
   The contested half is the contract itself, and this document deliberately
   does not decide it. The original draft of this pillar proposed inferring
   "common GUARANTEES" to save authored tokens — but guarantees *are* contract
   content and are hashed, so this collides with the pillar's own
   contracts-stay-explicit rule. Concretely: deriving guarantees from the body
   would let an implementation edit move a contract hash (today, body-only
   edits never force dependents to re-pin), and it reverses the contract-first
   direction for that slice of the contract — the contract stops being purely a
   promise and becomes partly a description. How to handle this — full
   declaration as today, tool-suggested-but-author-written, or defaults with
   opt-out for near-universal guarantees — is an **open design question for the
   V2 design sessions, recorded here only as a question.** Two fixed points any
   resolution can build on: hash pins stay deliberately authored even though
   the value is tool-computed (the run-3 decision — auto-writing pins would
   silently re-pin the drift the hash exists to catch), and `string`'s no-size
   rule (Reference §2.5) shows when a contract token may legitimately go: only
   when the information remains fully present.
6. **Tokenizer-aware keyword and operator selection.** Candidates tested
   against the tokenizer basket; single-token forms preferred — while staying
   CFG-expressible, since the grammar must still publish as EBNF/GBNF for
   constrained decoding ([graph-native-vision](graph-native-vision.md)).

## What this does not change

The behavior graph, the contract-hash discipline, and every enforced check are
untouched — the acceptance bar below requires zero reduction versus v1. It is
also orthogonal to the authoring *mode*: the graph-native study found graph
text and v2 text token-neutral (~385 tokens either way), so the density lever
is the expression layer, which both modes share for implementation bodies.

## What this pillar does NOT decide

This document sets the objective and the measurement discipline. It does not
make the design decisions; those happen in design sessions, judged against the
measurements above. Open as of this writing:

- **Guarantee handling** (implication 5 — declared vs. tool-suggested vs.
  defaulted): the first session topic.
- **The grammar itself**: [v2-grammar-candidates.md](v2-grammar-candidates.md)
  collects the options; none is chosen.
- **Scalars-as-values semantics** (implication 2): direction named by the
  experiments, design not done.
- **The tokenizer basket composition** and **the numeric target** (the ≤2× is
  provisional, not derived).

## The governing tension — and the frontier

Density trades against the project's validated result: learnability-from-spec
(12/12 byte-exact in the rigorous re-run, rework ≈ 2 cycles). More grammar
means more ways to be wrong; APL is the cautionary endpoint. Both sides are
measurable with the existing harness — density as the token ratio, learnability
as cold-start rework and correctness — and the grammar candidates' evaluation
matrix is the prior instance of weighing exactly this trade (its sensitivity
analysis flips the winner on these weights). The design target is the frontier
point, not maximum density.

## The decisive experiment

Add a control condition to the harness: the same task, authored cold, in a
**denser but equally never-trained** syntax. Today's ~5× conflates two terms
the runs could not separate; this condition bounds the split — if the dense
untrained syntax closes most of the gap, intrinsic verbosity dominated; if it
doesn't, training data did. It bounds rather than fully resolves (the dense
syntax still pays the zero-training tax), but it is the first design-actionable
decomposition, and it drops straight into the existing protocol: same
isolation, same byte-exact acceptance, same per-condition n.

## Acceptance criteria

- [ ] Benchmark suite defined — at least three tasks beyond http-relay (the
      relay alone is a near-worst case for a register-level language and a best
      case for trained ones) — with a Python baseline measured per task.
- [ ] Tokenizer basket chosen; candidate-syntax **program-text** measurement
      performed and documented before grammar freeze.
- [ ] Density target set and tracked per run. Proposed: **≤2× Python
      program-text tokens** — provisional and deliberately aggressive: the
      candidates' own estimates land at 2.4–3.3×, so hitting it means going
      beyond candidate B/C or discovering the estimates were pessimistic.
      Revised from measurement, not kept from hope.
- [ ] The dense-untrained control condition run; the density/learnability
      frontier (token ratio vs. rework) located.
- [ ] Every density feature lowers to the same checked behavior graph — zero
      reduction in enforced checks vs. v1 (the C1–C13 constraints hold).

## Summary

V1 proved a fully explicit language is checkable and learnable from the spec;
V2's objective is to make it dense — verified intent per token, measured in the
writing model's own token space — with contracts staying explicit,
implementations compressing, and every syntax decision settled by tokenizer
measurement rather than intuition.
