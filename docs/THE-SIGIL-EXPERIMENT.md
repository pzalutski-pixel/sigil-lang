# Should AI Have Its Own Programming Language?

A question surfaced during a year of working with AI coding assistants: current programming languages were designed for human cognitive limitations. Variable names exist so humans remember what things are. Syntax sugar organizes code for human brains. High-level abstractions hide complexity humans can't track.

An AI author is different: a large (if bounded) context window, strong in-session recall, no fatigue — and, unlike a person, no years of habit invested in any particular syntax. Yet AI writes Python and C++, languages shaped around human constraints that don't all apply to it.

Should AI have its own programming language? What would one look like? Would it even help? This article documents the journey of exploring that question: the design process, building a language, testing it, and what those efforts revealed.

---

## Part 1: The Vision

Programming languages exist because humans can't work directly with machines. A CPU understands binary. Humans don't. So we built layers of abstraction, each one making computers more accessible to human minds.

| Level | Layer | Why It Exists | Examples |
|:-----:|-------|---------------|----------|
| 5 | High-level / Scripting | Convenience: automatic memory, minimal syntax | Python, JavaScript |
| 4 | Systems | Readability: structured code, named variables | C, Rust |
| 3 | Intermediate Representation | Compiler internal: typed ops, optimization | LLVM IR, GCC GIMPLE |
| 2 | Assembly | Architecture bridge: mnemonics for binary | x86, ARM |
| 1 | Machine Code | What the CPU actually executes | Binary |

Each layer adds abstraction humans need. We can't track thousands of memory addresses, so we invented variables. We can't read binary, so we invented assembly. We can't manage complexity, so we invented functions. Levels 4 and 5 exist because human brains have limits.

Code goes through a pipeline before it runs:

```
[Intent] → [Write Code] → [Parse] → [Structure] → [Optimize] → [Binary] → [Execute]
```

The compiler doesn't run text. It parses text into structure (syntax tree, then intermediate representation), optimizes that structure, and generates binary. Text is just the input format humans use.

For humans, text needs to be readable. Variable names help humans remember what values mean six months later. Formatting helps humans scan code visually. Syntax sugar reduces typing. Comments explain intent to future readers. These exist so humans can work with the code:

```
HUMAN: [Intent] → [Code: names, formatting, sugar, comments] → [Parse] → [Structure] → ...
                                    ↑
                              Human needs all this
```

AI is different. Within a session it holds far more in working memory than a person and doesn't tire; it doesn't lose track of what a variable means across a long file, doesn't need visual formatting to scan code, doesn't need comments to remember intent. Many conveniences built into programming languages exist for human cognitive limits that an AI author feels far less. (Not *all* of them, as the experiment later shows — some "convenience" is really compression, and that turns out to matter — but that is getting ahead of the story.)

What does AI actually need? Structure. Programs are fundamentally graphs: nodes of operations connected by edges of control flow. "If this condition, go here. Otherwise, go there. Load this value. Store that result." That's what compilers work with internally after they parse human-readable text. AI doesn't care about syntax. AI cares about topology: what connects to what.

So why does AI produce human-readable code at all? Why not output structure directly?

```
                         [Write Code] → [Parse]
                               ✗           ✗

AI IDEAL: [Intent] ──────────────────────────→ [Structure] → [Optimize] → [Binary] → [Execute]
```

If AI could skip the text layer entirely, it would output graphs directly. Nodes and edges. Operations and control flow. No parentheses, no semicolons, no variable names, no formatting. Just structure.

This is the vision: AI outputting the same internal representation that compilers use. Skipping the human-readable layer that exists for humans who aren't there.

But where in the stack should this structure sit?

Looking at the layers from an AI perspective:

| Level | For AI? | Why |
|:-----:|:-------:|-----|
| 5 - High-level | ❌ | Too ambiguous, dynamic typing hides errors until runtime |
| 4 - Systems | ⚠️ | Still carries human formatting overhead |
| **3 - Intermediate** | **✅** | **Typed operations, explicit memory, explicit flow, no sugar** |
| 2 - Assembly | ❌ | Too verbose, architecture-specific, wastes context |
| 1 - Machine code | ❌ | Too dense, single bit changes meaning |

Level 3 is the target. Typed operations. Explicit memory. Explicit control flow. No syntactic sugar. No formatting conventions. This is what compilers work with internally after parsing, and it's where AI-native code should aim.

That's the vision: AI outputting graph structures at Level 3. Nodes and edges instead of text and syntax. The same representation compilers use internally, produced directly by AI without the human-readable detour.

What would it take to build something like this? Is it even possible with current LLM-based AI agents that generate text token by token?

---

## Part 2: What Would the Structure Look Like?

If AI outputs graphs instead of text, what are the nodes?

When engineers describe systems, they don't explain steps. They explain obligations. A web server "takes requests, returns responses." A compiler "takes source, produces binaries." A database "takes queries, returns results." Input, output, guarantees. That's how we naturally think about computation.

Requirements work the same way. "When the user clicks submit, the form saves." Input: click. Output: saved form. "The system handles 1000 requests per second." That's a guarantee. We already think behaviorally. We just call it something else.

So the graph nodes are **behaviors**: units that declare inputs, outputs, and guarantees.

```
┌─────────────────────────────────┐
│           BEHAVIOR              │
├─────────────────────────────────┤
│  CONTRACT                       │
│    Inputs:     what it needs    │
│    Outputs:    what it produces │
│    Guarantees: what it promises │
├─────────────────────────────────┤
│  IMPLEMENTATION                 │
│    How it computes              │
└─────────────────────────────────┘
```

The contract isn't a description attached to the behavior. The contract IS the behavior. Implementation merely fulfills it.

This matters because incompleteness becomes visible. A behavior with undefined inputs isn't "missing documentation." It's an incomplete structure that cannot exist as a valid program. The gap is structural, not semantic.

Traditional development separates specification, code, and tests into three artifacts that drift apart over time:

```
TRADITIONAL:  [Specification] → [Code] → [Tests]

              Written separately. By different people.
              At different times. They drift. Bugs hide in the gaps.
```

With behaviors, the contract is the specification, and the compiler checking the implementation against it does some of the work a separate test suite usually does — the part a compiler can actually check (types, sizes, outputs-on-all-paths, declared guarantees), not whether the logic means the right thing. One artifact instead of three:

```
BEHAVIOR = CONTRACT + IMPLEMENTATION

           Contract declares what it must do (interface + guarantees)
           Compiler checks the implementation against it (the parts it can verify)
           Implementation is how it does it (the code)

           One artifact — so the contract and the code can't drift apart.
```

(What a compiler can't check — whether the algorithm is *correct* — still needs tests. The claim is narrower than "no tests," and the structural-vs-semantic line is drawn carefully in [the design model](DESIGN-MODEL.md).)

Behaviors compose into larger behaviors. Leaf behaviors implement computation using primitive operations. Composite behaviors wire other behaviors together:

```
┌─────────────────────────────────────────────────────────────┐
│  LEAF BEHAVIOR                                              │
│    Contract + Implementation (primitive operations)         │
│    Does actual computation                                  │
├─────────────────────────────────────────────────────────────┤
│  COMPOSITE BEHAVIOR                                         │
│    Contract + Composition (wiring other behaviors)          │
│    Connects behaviors, no new primitives                    │
└─────────────────────────────────────────────────────────────┘
```

The entire program becomes a graph:

```
                    ┌─────────────┐
                    │  COMPOSITE  │
                    └──────┬──────┘
                     ┌─────┴─────┐
              ┌──────┴──┐    ┌───┴───────┐
              │  LEAF   │    │ COMPOSITE │
              └─────────┘    └─────┬─────┘
                              ┌────┴────┐
                         ┌────┴──┐  ┌───┴────┐
                         │ LEAF  │  │  LEAF  │
                         └───────┘  └────────┘

              Leaf behaviors at the bottom do actual work.
              Composite behaviors above wire them together.
              The graph IS the program.
```

Completeness is structural. Every output must connect to an input. Every input must have a source. Every path must terminate. An incomplete graph isn't a buggy program. It's not a program at all. Like a sentence with missing words, it doesn't express anything.

This creates a division of labor. AI can traverse graphs, find structural gaps, verify completeness. Humans provide meaning: is this the right algorithm? Does this behavior do what was intended? AI handles structure. Humans handle semantics.

---

## Part 3: The Limitation and the Compromise

The vision is clear: AI outputting graphs directly at Level 3. Behaviors with contracts. Nodes and edges. Structure without text.

But current LLMs can't do this. LLMs generate text token by token. They predict the next word, the next symbol, the next character. They can't output graph structures directly. This is a fundamental limitation of transformer architecture today.

So *AI constructing the graph directly* isn't possible yet — the model emits tokens, not graph-construction steps. But text can be a faithful **serialization** of the graph rather than an imitation of one: each line a node, its references the edges. The compiler parses that serialization back into the actual behavior graph and builds, checks, and compiles *that*. So what's compromised is the authoring *modality* — the model types the graph's text form instead of mutating the graph through direct validated steps — not the artifact. The program is a graph either way; the text is just how a token-emitting author writes it down.

That became the goal: a line-based serialization of the behavior graph, targeting Level 3 — explicit structure, minimal overhead, as close to the compiler's internal representation as a text form gets.

Building this required examining every language feature and asking: does this exist for humans, for machines, or for both?

| Feature | For Humans | For Correctness | Design Choice |
|---------|:----------:|:---------------:|---------------|
| Variable names | ✅ Remember what things are | ❌ | Minimal identifiers |
| Syntax sugar | ✅ Readability | ❌ | Removed |
| Comments | ✅ Future readers | ❌ | Removed |
| Formatting | ✅ Visual structure | ❌ | Not significant |
| Type safety | ✅ Catch errors | ✅ | `int`, `float`, `bytes` (+ a self-describing `string` added later) |
| Memory safety | ✅ Prevent bugs | ✅ | Handles, not pointers |
| Error handling | ✅ Graceful failure | ✅ | Required outputs |

AI doesn't need human conveniences. AI does need correctness guarantees. Strip the first, keep the second.

**Design choices that emerged:**

**Operations:** What's truly irreducible? 43 primitives: load, store, arithmetic, comparison, branching. The atoms from which everything else composes. No higher-level operations that could be built from these.

**Structure:** Functions exist for human organization. Compilers inline them anyway. Alternative: behaviors with explicit contracts declaring inputs, outputs, and guarantees. The contract is the specification. The implementation fulfills it. The compiler verifies the match.

**Types:** Traditional type systems carry enormous complexity. Stripped to fundamentals: `int`, `float`, `bytes` — data is bytes, and operations interpret them. (A fourth, self-describing `string` was added later, for variable-length text where a fixed `bytes` size was the wrong fit.) The compiler enforces consistency.

**Memory:** Pointers are dangerous, the source of most C/C++ security vulnerabilities. Alternative: handles. Opaque references that can't be forged or manipulated arithmetically. Single ownership. Compile-time bounds verification.

By the time implementation began, the specification had grown to several thousand lines covering type system rationale, memory model, concurrency primitives, and safety rules. The [design documents](https://github.com/pzalutski-pixel/sigil-lang/tree/main/docs/spec) capture the reasoning behind each decision: what was considered, what was rejected, and why.

---

## Part 4: What Emerged

The result was Sigil, an experimental language with a compiler built in Rust with an LLVM backend, 43 primitive operations, contract-based behaviors, and handle-based memory with compile-time safety.

The compiler checks that implementations match their contracts. If outputs are declared, it proves every output is written on every execution path — a real check, done with control-flow analysis. Other guarantees are checked too: a `pure` behavior is verified not to touch state directly, *and* a whole-graph pass propagates that through its CALL chain — a `pure` behavior that (transitively) depends on a non-pure one is rejected (the residual gap is *undeclared* pattern-`MEMORY` access — declared `MEMORY` is caught; see [STATUS.md](STATUS.md)). The compiler is a quality gate for *structure*: AI proposes code, and the compiler rejects what it can prove is structurally unsound — not whether the logic is *correct*, which stays the work of tests.

Behaviors form a hierarchy. Leaf behaviors have implementations as graphs of primitives; they do the actual computation. Composite behaviors have implementations as wiring of other behaviors, no new primitives, just connections. Complex behaviors are just wiring of simpler behaviors. Implementation only exists at the leaf level. The wiring diagram is the program.

The design process forced rigor that crystallized into something beyond the language itself: a way of thinking where contracts aren't descriptions of code but the computational entities themselves. An incomplete graph isn't a bug; it's a requirements specification with gaps that generate the exact questions needed to complete it.

The compiler rejecting an incomplete behavior is a real result on its own. When it refuses to build because outputs aren't written on all paths, or memory isn't freed, or contracts don't match, the node-level model is doing exactly what it was designed to. But that is a narrow claim — it shows the *checks* work, not that an AI-native language *helps anyone*. That second question is what the experiment tests, and it is the harder and more important one. (As it turns out, the checks held up and the help did not — see below.)

---

## Part 5: The Experiment

Building a language is one thing. Testing whether it helps is another.

The instrument is direct: put the language in front of its intended user — an **AI coding agent** — and watch it work. Each trial is **one independent agent**, run fresh, given the task plus a **condition** that fixes what it may read (a trained baseline, Sigil grammar-only, or Sigil grammar + examples); it **authors the whole program itself**, with Sigil seen **cold** — never in training. The output is then built, run, and checked byte-for-byte, and the rework, friction, and tokens are recorded. The measurement is how an AI coder *fares writing under each condition*, not whether a hand-polished program works.

The premise behind Sigil was that explicit, unambiguous grammar would help AI generate correct code. But several assumptions needed testing. How much does training data matter versus explicit specifications? Can AI learn a completely new language from documentation alone? Does providing example code improve or degrade output?

The task chosen was building an HTTP file relay system: a producer that reads a file and POSTs its contents over HTTP, and a consumer that receives the data, timestamps it, and writes it to a file. This requires real capabilities: file I/O, HTTP networking, string handling, error paths. Complex enough to reveal learning differences, simple enough that trained languages handle it trivially.

Four conditions were tested. Python and C++ served as baselines, languages the AI already knew from training data, with extensive examples of HTTP servers, file operations, and socket patterns. Sigil was tested in two conditions: one with only the [grammar specification](https://github.com/pzalutski-pixel/sigil-lang/blob/main/docs/SIGIL-LANGUAGE-REFERENCE.md) and stdlib contracts, another with grammar plus [example code](https://github.com/pzalutski-pixel/sigil-lang/tree/main/examples).

The critical difference: Python and C++ have millions of examples in AI training data. Sigil has zero. The AI had never encountered Sigil before this experiment.

The hypothesis was that explicit, unambiguous grammar would result in fewer errors despite token overhead.

**Results** (read these as a rough first pass, not a measurement — see the caveats below):

| Condition | Tokens | Rework Cycles | Logic Bugs | Success |
|-----------|-------:|:-------------:|:----------:|:-------:|
| Python | 3,400 | 0 | 0 | ✅ First attempt |
| C++ | 10,400 | 0 | 0 | ✅ First attempt |
| Sigil (no examples) | 70,800 | 12 | 0 | ✅ No intervention |
| Sigil (with examples) | 90,000 | 4 | 3 | ⚠️ Required intervention |

The hypothesis was not confirmed: the trained languages were far cheaper, by very roughly an order of magnitude.

> **Treat this table as suggestive only.** This was **n = 1** per condition; the token figures were read off a `/context` display, not measured uniformly; and the Sigil programs were **not** compiled and verified on the test machine at the time. The order-of-magnitude gap is real and directionally robust, but the *specific numbers* and the "11×" are not a clean measurement — which is exactly why the whole thing was re-run, more carefully, in [Part 8](#part-8-revisiting-it-with-a-stronger-model). The re-run's figures are measured differently and are **not** comparable to these; only the *shape* (large multiple, correctness vs. cost) carries across.

But raw comparison misses the point. The interesting findings emerged from the details.

---

## Part 6: What the Experiment Revealed

**📚 Training data dominance is real and quantifiable.** Python's efficiency (3,400 tokens with zero rework) demonstrates what training data provides. The AI didn't learn Python during the session; it already knew HTTP server patterns from training. The efficiency gap was approximately 11x between trained languages and untrained.

**✅ Explicit grammar does enable learning.** Both Sigil conditions successfully completed the task. The AI learned the language entirely from in-context documentation, with no training and no fine-tuning. This proves that unambiguous specification can enable code generation for unknown languages. The AI made mistakes, the compiler caught them, corrections followed. The system worked as designed.

**❓ One observation was unexpected.** The condition with examples performed worse than the condition without examples. More tokens consumed. More logic bugs: three in the examples condition versus zero without examples. The condition without examples required user intervention at no point; the condition with examples did.

Why? Unknown. Several possible explanations exist. The examples might have been poor quality. There might not have been enough examples. More context might introduce more variability in AI exploration. With only one test, this is an observation that needs further research, not a conclusion.

**🛡️ Compiler strictness proved its value.** Several Sigil "errors" were the compiler catching real problems: missing output writes, unhandled execution paths. The same bugs in Python would have been runtime failures discovered during testing or production. The compiler functioned as designed: reject invalid code before it runs.

---

## Part 7: What Was Learned

The efficiency gap was expected. An unknown language with zero training data competing against Python with millions of examples was never going to win on tokens. That wasn't the point.

The point was whether the model worked at all. Could AI read a grammar specification and produce valid code? Could AI read behavior contracts from a stdlib and wire them into compositions? Could AI work with raw primitives when high-level abstractions weren't enough?

The session transcript shows what happened. AI read the grammar reference and the stdlib contracts. From those contracts, AI understood what behaviors were available: socket operations, file I/O, string manipulation. Each contract declared inputs, outputs, and guarantees. That was enough. AI composed them into new behaviors that fulfilled the task requirements.

When composition wasn't sufficient, AI switched to implementation. The `extract-http-body` behavior started as a composition calling `find` and `substring`. When the compiler rejected it because outputs weren't written on all paths, AI rewrote it using raw primitives: LOAD, STORE, IADD, BRANCH. AI didn't need the high-level abstractions. The primitives worked.

The compiler feedback loop proved essential. Hash mismatches got corrected because the compiler reported the expected hash. Missing output writes got fixed because the compiler identified which paths failed to write. Memory allocation issues got resolved because the compiler tracked allocations across branches. Each error pointed to a specific problem. AI fixed it and tried again. Twelve rework cycles, but each one addressed a real issue.

The stdlib contracts served as the interface. When AI needed to create a socket, the contract showed: input domain, input type, output fd. When AI needed to format a timestamp, the contract showed the required inputs and outputs. Contracts were the documentation. AI read them and used them correctly.

What remained challenging was the lack of training. AI made syntax errors that experience would have prevented. Keyword placement, structure ordering, naming conventions: all learned through trial and error. The 11x token overhead reflects this learning curve.

What remains untested is whether training changes the equation. With training data for Sigil, would the efficiency gap close? Would explicit grammar plus training outperform traditional languages? That's the experiment worth running next.

---

## Part 8: Revisiting It With a Stronger Model

The first experiment ran in January 2026 on the best model available at the time. Models improve quickly, and that raises a fair doubt: was the 11x gap a fact about Sigil, or a fact about that particular model's ceiling? A weaker model might fumble an unfamiliar language in ways a stronger one wouldn't. So in June 2026 the whole thing was run again — same task, same four conditions — on a newer, more capable model (Claude Opus 4.8), and this time it was built to be harder to argue with.

Every change to the method pushed toward rigour:

- **Three trials per condition instead of one.** The original n=1 left every number exposed to the variance of a single stochastic run. The re-run does three of each — twelve trials in total.
- **Every solution was compiled and run end-to-end**, then checked against the required output byte-for-byte — modulo the run-specific timestamp the task injects (so "byte-exact" here means the body and framing matched the acceptance check exactly, only the timestamp varies between runs). The first run couldn't fully build the Sigil programs on the test machine; by June the toolchain built and linked cleanly, so all four conditions — Python, C++, and both Sigil variants — were verified to actually work, not merely to look finished.
- **Tokens were measured uniformly** as each agent's total consumption, rather than read off a context display. That makes the figures internally consistent — though, for that same reason, not directly comparable to the first run's numbers. What compares across the two runs is the shape: rework, bugs, and the size of the multiple.
- **Each trial's full session transcript was kept** as evidence of how the run actually went, alongside the source it produced.

**Results — means across three trials each:**

| Condition | Mean tokens | vs. Python | Mean rework | Logic bugs | Success |
|-----------|------------:|:----------:|:-----------:|:----------:|:-------:|
| Python | 18.5k | 1.0× | 0.0 | 0 | 3/3 |
| C++ | 23.3k | 1.3× | 0.3 | 0 | 3/3 |
| Sigil (grammar only) | 91.3k | 4.9× | 2.3 | 0 | 3/3 |
| Sigil (grammar + examples) | 86.8k | 4.7× | 3.3 | 0 | 3/3 |

The result split cleanly: **a stronger model fixed the correctness problem, but not the cost problem.**

Correctness improved across the board. Every one of the twelve trials produced byte-exact output with zero logic bugs — including the with-examples Sigil condition that, in the first run, had shipped three logic bugs and needed a person to step in. Rework on the grammar-only condition fell from twelve cycles to roughly two. The language stopped being something the model stumbled over; it read the grammar, wrote valid code, and fixed the compiler's complaints without flailing.

Cost did not move. Sigil still ran several times more expensive than Python — about 5x within the run — and the better model wrote almost exactly as much register-level code as the worse one. That isolates one thing cleanly: model *skill* is not the lever. A year of model progress left the token count essentially unchanged, so "wait for a smarter model" will not fix it.

But it would be wrong to round that up to "the language is 5x worse," and the reason is the confound sitting under the whole experiment: **neither model had ever been trained on Sigil.** Both learned the entire language cold — from the spec, in-context, every single run — and were measured against Python and C++, which the model has effectively *pre-compressed* through massive pre-training (this is the flip side of "syntax is a compression algorithm": a trained language ships its compression in the weights and the tokenizer; a brand-new one ships none). So the ~5x is a mix of two things the experiment cannot separate: genuine register-level verbosity (real, and partly the language's fault) and the total absence of any training signal (not the language's fault, and possibly recoverable). The cleanest evidence that training would help is in the friction itself — most Sigil agents hand-reimplemented the SHA-256 contract-hash routine (four of the six; the other two improvised a placeholder-and-read-back workaround), exactly the kind of thing a model trained on Sigil would already "know" and never re-derive. Whether training closes the gap is the obvious next experiment, and it is **untested here.** If anything is "the finding," it is the narrower one: *for a model that has never seen it, an unsugared language is several times more expensive — and a better-skilled model does not change that; only training data, untested, might.*

Two things came into sharper focus. First, the dominant remaining friction was pinned down precisely: contract hashing tripped up all six Sigil trials — four reimplemented the SHA-256 algorithm by hand, two worked around it by writing placeholder hashes and reading back the compiler's computed value — because there was no usable tool for it (the compiler's `--fix-hashes` flag mishandled placeholder hashes). That is a tooling gap, not a flaw in the language — and the cheapest, highest-value thing to fix. Second, the "examples made it worse" oddity from the first run held up without the noise: with examples cost as much as without, and this time there were none of the pattern-copying bugs that had muddied the original observation. The examples supplied socket boilerplate and nothing that mattered for the cost.

The full methodology, every per-trial number, and the raw session transcripts for both runs live under [`experiments/`](../experiments/) — the January study in [`experiments/http-relay/run-1/`](../experiments/http-relay/run-1/EXPERIMENT.md), the June re-run in [`experiments/http-relay/run-2/`](../experiments/http-relay/run-2/RESULTS.md).

---

## A Follow-up: Did Fixing the Friction Help? (June 2026, tooling re-run)

The re-run above found a single dominant friction — **contract hashing** — that tripped all six Sigil trials: four reimplemented the SHA-256 contract-hash algorithm by hand, the other two worked around it with placeholders. Two things were added afterward to remove it: a `sigil-compiler --hash` command that computes a behavior's contract hash, and a `sigil-authoring` skill that carries the hashing workflow. So a narrow follow-up question: with those in place, does the friction actually disappear when an agent works cold?

Same task and protocol, the two Sigil conditions only, **n = 3** each. One deliberate, disclosed change: the prompt no longer tells the agent *how* to get a hash (the June prompt instructed hand-computation per the grammar's HASH section); it now only requires that the hash be valid. The skill was available as a real capability but was **not** mentioned in the prompt, so adoption is observed, not forced. (Trials were spawned as subagents, so token totals are measured differently and are **not** comparable to the runs above — only the friction and correctness signals carry across.) Full write-up: [`experiments/http-relay/run-3/RESULTS.md`](../experiments/http-relay/run-3/RESULTS.md).

The result was clean. **The dominant friction disappeared: zero of six trials hand-rolled the hash** (down from six of six) — every agent reached for `--hash` on its own, corroborated by the absence of the hand-rolled hash scripts the earlier trials had left behind. Correctness held: all six built and ran with no logic bugs (four byte-exact; two added a single trailing newline, an ambiguous-acceptance artifact that *independent* byte-checking caught — not a relay bug, but a reason to pin the exact output bytes in future runs). And, as expected, **token cost did not move** — nothing about the language changed, so the register-level verbosity is unchanged, and the agents said as much directly. With hashing out of the way, the friction relocated to what it had been masking: the leaf/composite split (all byte-work forced into leaf behaviors). The residual hash friction is just the agent's ordinary authoring loop — run `--hash`, write the pin — which the build already validates (a wrong or stale hash is a compile error); no new tool is needed.

What this shows: the fix did exactly the narrow thing it was meant to — kill the dominant tooling friction — and nothing it wasn't. It did not, and could not, move the token cost; that still waits on a denser surface syntax.

---

## A Second Follow-up: Does a Standard Library Remove the Hand-Rolling? (June 2026, stdlib re-run)

With hashing gone, the tooled run had relocated the friction to **byte-assembly** — every Sigil trial hand-built a `sockaddr_in`, framed HTTP by hand, and one trial even reimplemented `make-sockaddr` because the library didn't carry it. That is a missing-batteries confound, not a property of the language: an agent hand-rolling what Python imports inflates both the measured cost and the rework. So the standard library was built out — ~92 behaviors across math, strings, hashing (incl. SHA-256), encoding, collections, file I/O, time, and socket-address construction — each built and byte-tested, and the example programs were refactored to use it.

A third run tested whether agents actually reach for it: same relay, the two Sigil conditions, **n = 3** each, with the `sigil-authoring` skill available as a real capability (not prompt-fed) and the output **pinned to exact bytes** to remove the trailing-newline ambiguity. Result: **6/6 byte-exact; 0/6 hand-rolled the hash; and 0/6 hand-rolled the sockaddr, file I/O, or timestamp** — every trial used `make-sockaddr`, `read-file-all`/`write-file-all`, and `now`/`format-time`. The only hand-rolled code left was HTTP framing, which has no library equivalent by design. The library got discovered and used, and the byte-assembly friction the tooled run named is gone. Full write-up: [`experiments/http-relay/run-4/RESULTS.md`](../experiments/http-relay/run-4/RESULTS.md).

Two things did not change. **Token cost stayed flat** — the library removes rework and hand-rolling, not the volume of register-level wiring; consistent with the standing result that the verbosity is the language, not the missing batteries. And the run kept earning its keep as **free QA**: the trials reported compiler issues serious enough to put the run's own numbers in question — what came of investigating them is the next section. The pattern, meanwhile, holds across all three runs: fill the named friction, and the next one underneath it shows — hashing → byte-assembly → the language's own register-level verbosity, which is the one tooling can't touch.

---

## A Third Follow-up: Bugs in the Instrument (June 2026, bug-fix re-run)

Why do compiler bugs threaten the measurement? Because a trial whose rework is driven by a compiler defect isn't measuring "can an AI write Sigil" — it's measuring "can an AI fight a broken compiler," a confound the Python and C++ baselines never face. So before that run's results could stand, each report had to be confirmed through code, the real defects fixed, and the experiment repeated on the fixed compiler.

Investigation confirmed most of the reports as real compiler defects, since fixed. The one that had looked most serious turned out to be a misdiagnosis — the trial hit real friction but attributed it to a compiler mechanism that, on inspection of the runtime, does not exist. That is a methodological finding in its own right: an agent's bug report is a lead, not a finding, until reproduced through code.

The identical protocol was then run again on the fixed compiler — same task, same two Sigil conditions, three trials each, output pinned to the same exact bytes. All six trials were byte-exact, none hit any of the reported issues, and grammar-only rework fell from 5.0 cycles to 2.0 — the earlier mean had been carried by trials fighting compiler defects from first principles. The friction that remains is the language working as designed — chiefly the leaf/composite split and the register-level explicitness — which is what the denser-syntax direction already targets. The run also introduced a cleaner token accounting, separating the one-time cost of reading the grammar into context (a trained language pays that in its weights, at zero inference tokens) from the per-program authoring cost. The figures, the caveats, and the defect-by-defect record live in the write-up: [`experiments/http-relay/run-5/RESULTS.md`](../experiments/http-relay/run-5/RESULTS.md).

That is the loop working as intended: the experiment surfaced defects in its own instrument, code-level investigation separated the real from the misattributed, and the data that stands was produced on the clean compiler.

---

## What Remains

The compiler validates that each behavior is internally complete. Memory allocated is freed on all paths. Outputs declared are written on all paths. Contracts match their hashes. These are node-level validations: is this behavior structurally sound?

Since the experiment ran, edge-level validation has largely landed. In a composition, every output a `CALL` produces must be consumed — routed to another `CALL`, used as a `BRANCH` condition, or written to an `OUTPUT` — or explicitly `DISCARD`ed; an unconsumed output is a compile error (`E0511`). Inputs are sourced via `CALL` argument count/type checking. Across behaviors, the compiler also enforces transitive purity, rejects circular dependencies, and verifies every `REQUIRES name@hash` pin against the dependency's current contract hash (including linked stdlib deps). So a graph that drops a result or pins a stale dependency no longer compiles.

What's still not built is the *full graph-native* port model. A composition's wiring is real (output→input), but V1 keeps it *implicit* in the CALL references rather than materializing it as explicit `Edge` values. The general gap detector (`graph/gaps.rs::find_gaps`, which checks unsourced inputs / unconsumed outputs over a fully wired graph) is built and tested but needs those materialized edges to run without over-reporting — so materializing the wiring as explicit edges is the remaining graph-native increment.

This doesn't change the experiment's findings. The bugs it caught were node-level (outputs not written, memory not freed) and those stand; the output-consumption and dependency-hash enforcement above were added afterward and are what the current compiler ships.

---

## Closing

The journey started with a question: Should AI have its own programming language?

That question led to a vision of AI constructing graphs directly, a compromise in *authoring modality* when LLMs could only emit text, a language whose text is a faithful serialization of that graph, a compiler that builds and verifies the behavior graph (completeness within a behavior, plus cross-behavior purity, dependency-hash, and output-consumption checks), and an experiment that tested the whole thing.

The original question doesn't have a clean answer. Training data dominance is real. The efficiency gap is substantial. But the experiment also showed something works: AI can learn new languages from specifications alone. Compilers can catch bugs before runtime. The contract-first model holds up in practice.

Is it worth building AI-native languages? The answer: still exploring. The language exists. The compiler works. The questions that remain are worth pursuing.

The code is at [github.com/pzalutski-pixel/sigil-lang](https://github.com/pzalutski-pixel/sigil-lang).

---

### Resources

- 📖 [Language Reference](SIGIL-LANGUAGE-REFERENCE.md) — complete grammar and semantics
- 📋 [Implementation Status](STATUS.md) — what is built, partial, or design-only
- 🔬 [Design specifications](spec/) — the reasoning behind each decision
- 🧪 [Experiments](../experiments/) — methodology, every run, and per-trial transcripts
- 💻 [Examples](../examples/) — working Sigil programs

---

*First experiment: January 2026 · Re-runs: June 2026*
