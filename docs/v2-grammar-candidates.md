# Sigil v2 Grammar Candidates

A collection of candidate v2 grammars — explorations of a denser surface syntax (an
expression layer, structured error handling, transitive guarantees) that would still
lower to the same checked graph. **None of these is chosen or built:** the shipped
language is the v1 grammar in [SIGIL-LANGUAGE-REFERENCE.md](SIGIL-LANGUAGE-REFERENCE.md),
and every token count here is an **estimate**, not a measurement. This is the idea
space, not a decision. The criterion any candidate should ultimately be judged by —
program-text tokens measured with real tokenizers, traded against cold-start
learnability — is proposed in [v2-density-pillar.md](v2-density-pillar.md).

---

## Executive Summary

This study designed the Sigil v2 grammar: it adds the missing middle layer between composition and register-level implementation (an expression layer), structured error handling, and transitive guarantees, and produced three complete candidate grammars. A key result is that structured control flow *improves* verification complexity from exponential to polynomial — it keeps the checks decidable.

**Key findings across all candidates:**

| Metric | v1 | All v2 Candidates | Python |
|--------|----|--------------------|--------|
| Token cost (3-behavior pipeline) | ~900 | 280–380 | ~115 |
| Ratio vs Python | 7.8x | 2.4–3.3x | 1.0x |
| Verification complexity | O(2^B × N) exponential | O(d × N × V) polynomial | N/A |
| Guarantee compositionality | None (local only) | Transitive through call graph | N/A |
| Error handling | Ad-hoc negative return values | Structured error ports + `fail` | Exceptions |

**The candidates span a spectrum** — Conservative (A: minimal additions) → Balanced (B: full expression layer, break/continue, pipeline syntax, practical types like string/bool) → Expressive (C: more sugar). All hold every decidability property; they trade token density against grammar surface area. The evaluation below weighs them, but nothing is picked or built.

---

## 1. Shared Foundation (All Candidates)

All three candidates share a common foundation shared across the candidates. The differences between candidates are additive — each builds on the previous.

### 1.1 What All Candidates Share

**From the expression-layer design:**
- Lowercase keywords (evidence: GAD NeurIPS 2024, 5–12% token savings)
- Infix expressions: `result = a + b` replacing `v1 = LOAD a 4; v2 = LOAD b 4; sum = IADD v1 v2 4; STORE result sum 4`
- `if/else/end` replacing BRANCH/LABEL/JUMP (decidability check: unconditionally approved)
- `while/end` replacing LABEL/BRANCH/JUMP loop patterns
- Byte indexing: `data[i]` replacing `LOAD data 1 i`
- String literals in implementation blocks
- `output name = expr` with explicit `output` keyword for writes_output checking
- Bitwise operations as named functions: `bit_and()`, `shl()`, `shr()`
- Type inference from contract declarations (+ maps to iadd or fadd based on declared types)
- Raw primitive escape hatch preserved (load, store, iadd, etc. still valid)
- Desugaring pass: expressions lower to existing primitive IR before semantic analysis

**From the contract redesign:**
- `error` clause for declaring failure outputs
- `fail` statement: writes all error ports and exits on error path
- `.failed` field auto-generated on calls to failable behaviors
- `pure` guarantee made transitive (fixes compositionality gap)
- `writes_output` extended: success paths write all outputs, error paths write all errors
- `no_alloc` dropped (misleading name; conflicts with expression temporaries — the v1 check bug once cited as a third reason has since been fixed, see [STATUS.md](STATUS.md))
- `string` carried over from v1, which has since made it a first-class self-describing interpretation with its own hash byte 0x04 (Reference §2.5) — for v2 it is inherited, not added
- `memory` clause moved from contract to implementation block
- Hash algorithm extended with ERRORS section
- `hash` line remains explicit — compiler computes the hash and provides it; AI copies it into source (AI can't compute SHA-256 but can copy a string)
- Clause ordering enforced: input, output, error, requires, guarantees

**From the decidability check (binding constraints):**
- C1: Expressions are pure computation — ALLOC/FREE/STORE-to-non-local are statement-level only
- C2: Output writes inside loop body alone do NOT satisfy writes_output
- C3: ALLOC inside loop body must be paired within same iteration
- C5: Transitive guarantee checking via callee contract inspection
- C6: Error outputs are part of structural completeness
- C7: Error propagation must be explicit (no implicit effect inference)
- C11: No general recursion (preserved, non-negotiable)
- C12: No higher-order behaviors (preserved, non-negotiable)
- C13: No pointer arithmetic (preserved, non-negotiable)

### 1.2 Integration Decisions (Resolving Tensions)

**Tension 1: Type inference identity change.**
The expression-layer design chose implicit inference from contract types (`a + b` infers IADD from `input a int 4`). The decidability check flagged this as an identity change from "data is bytes; operations interpret them."

*Resolution:* Accept the inference. The principle weakens to "data is bytes at the machine level; the compiler tracks interpretations from contracts." This is pragmatically necessary — without it, expressions would need typed operators (`a +i b`) which defeats the token savings purpose. The contract remains the single source of type truth; the compiler propagates it. Mixed-type expressions are compile errors requiring explicit `itof()`/`ftoi()`.

**Tension 2: `fail` in implementations vs compositions.**
The contract redesign used `fail` in both implementations and compositions. The expression-layer design's if/else works in both.

*Resolution:* `fail` is valid wherever `output` writes are valid — in both implementation and composition blocks. It desugars to: write all error port values, then jump to the error-exit block. The decidability check's analysis confirms this is a standard CFG edge that doesn't affect decidability.

**Tension 3: Loop variable mutation and auto-allocation.**
The expression-layer design proposed heap-promoting mutable loop variables. The decidability check's C3 requires loop-body ALLOC to be paired per iteration.

*Resolution:* Consistent. The compiler-generated ALLOC for loop variables is scoped to the loop body — the variable handle is allocated before the loop header, freed after loop exit. This is a single allocation covering all iterations, NOT per-iteration allocation. The decidability check's C3 applies to *user-written* ALLOC inside loops, not compiler-generated loop variable storage.

**Tension 4: `no_alloc` — dropped vs transitive.**
The contract redesign dropped it. The decidability check approved transitive `no_alloc` if kept.

*Resolution:* Dropped in all candidates. Rationale: (1) the name is misleading, (2) expression temporaries may require compiler-generated allocation. (A third reason recorded at the time — a v1 bug in the check — no longer holds: the scoped-ALLOC bug was fixed and `no_alloc` is enforced; see [STATUS.md](STATUS.md).) If needed later, re-add as `stack_only` with correct semantics once the expression model stabilizes.

**Tension 5: CALL-in-expressions.**
The expression-layer design didn't include it. The decidability check conditionally vetoed it.

*Resolution:* Excluded from Candidates A and B. Candidate C allows it for infallible calls only (no error ports to handle). The decidability check's analysis shows this is decidable for infallible calls because there are no error paths to verify — the call behaves like a pure function returning a value.

---

## 2. Candidate A: Conservative

*Maximum safety, minimum new concepts. Accepts all decidability check constraints strictly.*

### 2.A.1 Design Philosophy

- No break/continue (single loop exit through condition only)
- No CALL in expressions (statement-level only)
- No new types beyond v1's {int, float, bytes, string}
- No pipeline syntax
- No for loops (while only)
- All decidability check conditions met trivially

### 2.A.2 EBNF Grammar

```ebnf
(* === Top Level === *)
source_file     = { behavior | pattern | library | executable } ;

(* === Behavior === *)
behavior        = "behavior" identifier
                  [ description ]
                  contract
                  hash
                  ( implementation | composition | "native" ) ;

description     = "description" { text_line } ;
hash            = "hash" hex_string ;

(* === Contract === *)
contract        = "contract" { contract_clause } ;
contract_clause = input_decl | output_decl | error_decl | requires_decl | guarantees_decl ;
input_decl      = "input" identifier interpretation size ;
output_decl     = "output" identifier interpretation size ;
error_decl      = "error" identifier interpretation size ;
requires_decl   = "requires" behavior_ref { behavior_ref } ;
behavior_ref    = identifier "@" hex_string ;
guarantees_decl = "guarantees" guarantee { guarantee } ;
guarantee       = "pure" | "writes_output" ;
interpretation  = "int" | "float" | "bytes" | "string" ;
size            = integer_lit ;

(* === Implementation Block === *)
implementation  = "implementation" [ memory_decl ] { impl_stmt } "end" ;
memory_decl     = "memory" identifier { identifier } ;

impl_stmt       = assignment
                | output_write
                | if_stmt
                | while_stmt
                | alloc_stmt
                | free_stmt
                | scope_block
                | call_stmt
                | bare_call
                | fail_stmt
                | raw_primitive ;

assignment      = identifier "=" expression ;
output_write    = "output" identifier "=" expression ;
output_index    = "output" identifier "[" expression "]" "=" expression ;
if_stmt         = "if" expression { impl_stmt } [ "else" { impl_stmt } ] "end" ;
while_stmt      = "while" expression [ "limit" integer_lit ] { impl_stmt } "end" ;
alloc_stmt      = identifier "=" "alloc" integer_lit interpretation ;
free_stmt       = "free" identifier ;
scope_block     = "scope" { impl_stmt } "end_scope" ;
call_stmt       = identifier "=" "call" identifier { argument } ;
bare_call       = "call" identifier { argument } [ "->" identifier { "," identifier } ] ;
fail_stmt       = "fail" error_assign { error_assign } ;
error_assign    = identifier "=" expression ;

(* Raw primitive escape hatch *)
raw_primitive   = raw_assign | raw_standalone ;
raw_assign      = identifier "=" raw_op { argument } ;
raw_standalone  = raw_op { argument } ;
raw_op          = "load" | "store" | "iadd" | "isub" | "imul" | "idiv" | "imod" | "ineg"
               | "ieq" | "ine" | "ilt" | "igt" | "ile" | "ige"
               | "fadd" | "fsub" | "fmul" | "fdiv" | "fneg"
               | "feq" | "fne" | "flt" | "fgt" | "fle" | "fge"
               | "ftoi" | "itof"
               | "and" | "or" | "xor" | "not" | "shl" | "shr" | "sar" ;

(* === Expressions === *)
expression      = or_expr ;
or_expr         = and_expr { "||" and_expr } ;
and_expr        = cmp_expr { "&&" cmp_expr } ;
cmp_expr        = add_expr [ cmp_op add_expr ] ;
add_expr        = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr        = unary_expr { ( "*" | "/" | "%" ) unary_expr } ;
unary_expr      = [ "-" | "!" ] primary ;
primary         = identifier
               | identifier "[" expression "]"
               | identifier "." identifier
               | identifier "." "failed"
               | identifier "." "error" "." identifier
               | integer_lit
               | string_lit
               | "(" expression ")"
               | builtin_call ;
builtin_call    = builtin_name "(" expression { "," expression } ")" ;
builtin_name    = "bit_and" | "bit_or" | "bit_xor" | "bit_not"
               | "shl" | "shr" | "sar"
               | "itof" | "ftoi" ;

(* === Composition Block === *)
composition     = "composition" { comp_stmt } "end" ;
comp_stmt       = comp_call
               | set_stmt
               | if_stmt
               | while_stmt
               | fail_stmt
               | output_write ;

comp_call       = [ identifier "=" ] "call" identifier { argument } [ "->" identifier { "," identifier } ] ;
set_stmt        = "set" identifier "=" expression ;

(* === Pattern === *)
pattern         = "pattern" identifier [ pattern_memory ] [ on_create ] { behavior } [ on_destroy ] "end_pattern" ;
pattern_memory  = "memory" { mem_field } ;
mem_field       = identifier interpretation size ;
on_create       = "on_create" { impl_stmt } ;
on_destroy      = "on_destroy" { impl_stmt } ;

(* === Library === *)
library         = "library" identifier { behavior } "end_library" ;

(* === Executable === *)
executable      = "executable" identifier [ description ] [ uses ] entry "end_executable" ;
uses            = "uses" identifier ".pattern" ;
entry           = "entry" { comp_stmt } ;

(* === Operators === *)
cmp_op          = "==" | "!=" | "<" | ">" | "<=" | ">=" ;

(* === Lexical === *)
identifier      = letter { letter | digit | "-" } ;
hex_string      = hex_digit { hex_digit } ;
integer_lit     = [ "-" ] digit { digit } ;
string_lit      = '"' { string_char } '"' ;
string_char     = (* any char except '"' and newline *) | escape ;
escape          = "\\" ( "n" | "r" | "t" | "\\" | '"' ) ;
text_line       = (* any line not starting with a keyword *) ;
argument        = expression ;
letter          = "A".."Z" | "a".."z" | "_" ;
digit           = "0".."9" ;
hex_digit       = digit | "A".."F" | "a".."f" ;
```

### 2.A.3 Program 1: Hello World

```
behavior hello-world

description
  Prints a greeting to stdout.

contract
  output status int 8
  requires println@5cb2c2f1
  guarantees writes_output

hash 0281265c

composition
  call println "Hello, World!" 13 -> status
  output status = 0
end
```

**Token count:** ~40 tokens

**Python equivalent:**
```python
def hello_world():
    print("Hello, World!")
    return 0
```
~15 tokens. **Ratio: 2.7x**

### 2.A.4 Program 2: Three-Behavior Pipeline with Error Handling

**validate-input (leaf):**
```
behavior validate-input

description
  Validates raw input bytes. Checks non-empty, within bounds, valid header.

contract
  input raw bytes 4096
  input raw_length int 8
  output cleaned bytes 4096
  output cleaned_length int 8
  error code int 4
  error message string 256
  guarantees pure writes_output

hash a3f7c910

implementation
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
  output cleaned_length = raw_length
  i = 0
  while i < raw_length limit 4096
    output cleaned[i] = raw[i]
    i = i + 1
  end
end
```

**transform-data (leaf):**
```
behavior transform-data

description
  Transforms validated data. XORs each byte with 0xAA, increments by 1.

contract
  input data bytes 4096
  input length int 8
  output result bytes 4096
  output result_length int 8
  error code int 4
  error message string 256
  guarantees pure writes_output

hash b40e82d1

implementation
  output result_length = length
  i = 0
  while i < length limit 4096
    b = data[i]
    if b == 255
      fail code = 10 message = "Invalid byte 0xFF in data"
    end
    output result[i] = bit_xor(b, 170) + 1
    i = i + 1
  end
end
```

**process-pipeline (composition):**
```
behavior process-pipeline

description
  Wires validate-input into transform-data with error propagation.

contract
  input raw bytes 4096
  input raw_length int 8
  output processed bytes 8192
  output processed_length int 8
  error stage int 4
  error code int 4
  error message string 256
  requires validate-input@a3f7c910 transform-data@b40e82d1
  guarantees writes_output

hash e5d019f3

composition
  validated = call validate-input raw raw_length
  if validated.failed
    fail stage = 1 code = validated.error.code message = validated.error.message
  end

  transformed = call transform-data validated.cleaned validated.cleaned_length
  if transformed.failed
    fail stage = 2 code = transformed.error.code message = transformed.error.message
  end

  set processed = transformed.result
  output processed_length = transformed.result_length
end
```

### 2.A.5 Token Count Summary

| Component | Candidate A | v1 (est.) | Python | A/Python | v1/Python |
|-----------|-------------|-----------|--------|----------|-----------|
| hello-world | ~40 | ~65 | ~15 | 2.7x | 4.3x |
| validate-input | ~170 | ~480 | ~50 | 3.4x | 9.6x |
| transform-data | ~100 | ~300 | ~25 | 4.0x | 12.0x |
| process-pipeline | ~120 | ~150 | ~40 | 3.0x | 3.8x |
| **Total pipeline** | **~390** | **~930** | **~115** | **3.4x** | **8.1x** |

### 2.A.6 Compiler Verification

| Check | Complexity | Constraints Met |
|-------|-----------|----------------|
| Contract validation | O(I+O+E+R) | — |
| Port type/size matching | O(N×C) | — |
| Requires satisfaction | O(N×R) | — |
| Output consumption | O(d×N×O) | C2 |
| Input sourcing | O(d×N×V) | — |
| Path termination | O(V+E) | — |
| Resource pairing | O(d×N×H) | C1, C3 |
| Error path completeness | O(Calls×ErrOut) | C6, C7 |
| `pure` (transitive) | O(N+D) | C5 |
| `writes_output` (with errors) | O(d×N×(O+E)) | C2 |
| Hash integrity | O(contract_size) | — |
| Dependency acyclicity | O(N+D) | C11 |

All polynomial. Total: O(d × |Program| × |Vars|).

### 2.A.7 What's Lost

- Unstructured control flow (LABEL/JUMP between arbitrary points) — replaced by if/else + while
- `no_alloc` guarantee — dropped
- Explicit register naming for every intermediate — replaced by expressions (raw escape hatch still available)
- No break/continue — loops must exit through condition only, which can be verbose for search patterns

---

## 3. Candidate B: Balanced

*Adds practical expressiveness: break/continue, pipeline syntax, bool type.*

### 3.B.1 Differences from Candidate A

| Feature | Candidate A | Candidate B |
|---------|-------------|-------------|
| `break` | Not available | Available in while loops |
| `continue` | Not available | Available in while loops |
| `bool` type | Not available | Alias for `int 1` |
| Pipeline syntax | Not available | `x \|> f \|> g` desugars to sequential calls |
| `for` loop | Not available | Not available (while only) |
| CALL in expressions | Not available | Not available |

### 3.B.2 Additional EBNF (extends Candidate A)

```ebnf
(* Additional statements in implementation and composition blocks *)
break_stmt      = "break" ;
continue_stmt   = "continue" ;

(* break and continue are valid only inside while blocks *)
impl_stmt       = ... | break_stmt | continue_stmt ;
comp_stmt       = ... | break_stmt | continue_stmt ;

(* Pipeline syntax in composition blocks *)
pipeline        = expression { "|>" identifier } ;
(* Desugars: x |> f |> g  =>  _t1 = call f x; _t2 = call g _t1 *)

(* Additional type *)
interpretation  = "int" | "float" | "bytes" | "string" | "bool" ;
(* bool is an alias for int 1. Values: 0 = false, nonzero = true *)
```

### 3.B.3 Program 1: Hello World

Same as Candidate A (no difference for this simple program).

### 3.B.4 Program 2: Three-Behavior Pipeline with Error Handling

**validate-input (leaf):**
```
behavior validate-input

description
  Validates raw input bytes. Checks non-empty, within bounds, valid header.

contract
  input raw bytes 4096
  input raw_length int 8
  output cleaned bytes 4096
  output cleaned_length int 8
  error code int 4
  error message string 256
  guarantees pure writes_output

hash a3f7c910

implementation
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
  output cleaned_length = raw_length
  i = 0
  while i < raw_length limit 4096
    output cleaned[i] = raw[i]
    i = i + 1
  end
end
```

*(Identical to A — break/continue not needed here.)*

**transform-data (leaf):**
```
behavior transform-data

description
  Transforms validated data. XORs each byte with 0xAA, increments by 1.

contract
  input data bytes 4096
  input length int 8
  output result bytes 4096
  output result_length int 8
  error code int 4
  error message string 256
  guarantees pure writes_output

hash b40e82d1

implementation
  output result_length = length
  i = 0
  while i < length limit 4096
    b = data[i]
    if b == 255
      fail code = 10 message = "Invalid byte 0xFF in data"
    end
    output result[i] = bit_xor(b, 170) + 1
    i = i + 1
  end
end
```

**process-pipeline (composition) — with pipeline syntax:**
```
behavior process-pipeline

description
  Wires validate-input into transform-data with error propagation.

contract
  input raw bytes 4096
  input raw_length int 8
  output processed bytes 8192
  output processed_length int 8
  error stage int 4
  error code int 4
  error message string 256
  requires validate-input@a3f7c910 transform-data@b40e82d1
  guarantees writes_output

hash e5d019f3

composition
  validated = call validate-input raw raw_length
  if validated.failed
    fail stage = 1 code = validated.error.code message = validated.error.message
  end

  transformed = call transform-data validated.cleaned validated.cleaned_length
  if transformed.failed
    fail stage = 2 code = transformed.error.code message = transformed.error.message
  end

  set processed = transformed.result
  output processed_length = transformed.result_length
end
```

### 3.B.5 Bonus Example: Where break Helps

The hash-bytes behavior demonstrates break's value:

```
behavior find-byte

description
  Finds first occurrence of a target byte in data. Returns index or -1.

contract
  input data bytes 4096
  input length int 8
  input target int 1
  output index int 8
  guarantees pure writes_output

hash 91ca33e7

implementation
  output index = -1
  i = 0
  while i < length limit 4096
    if data[i] == target
      output index = i
      break
    end
    i = i + 1
  end
end
```

Without `break` (Candidate A), this requires a boolean flag:
```
  found = 0
  i = 0
  while i < length && found == 0 limit 4096
    if data[i] == target
      output index = i
      found = 1
    end
    i = i + 1
  end
```

The break version saves ~25 tokens and is more idiomatic for LLM generation.

### 3.B.6 Token Count Summary

| Component | Candidate B | Candidate A | Python | B/Python |
|-----------|-------------|-------------|--------|----------|
| hello-world | ~40 | ~40 | ~15 | 2.7x |
| validate-input | ~170 | ~170 | ~50 | 3.4x |
| transform-data | ~100 | ~100 | ~25 | 4.0x |
| process-pipeline | ~115 | ~120 | ~40 | 2.9x |
| find-byte (with break) | ~65 | ~80 | ~25 | 2.6x |
| **Total pipeline** | **~385** | **~390** | **~115** | **3.3x** |

Token improvement from break/continue is modest for the standard pipeline (~5 tokens) but significant for search/filter patterns (~15–25 tokens per behavior).

### 3.B.7 Compiler Verification

Same as Candidate A, plus:

| Check | Complexity | Constraints Met |
|-------|-----------|----------------|
| break/continue resolution | O(|Stmts|) | C9: static resolution to enclosing loop |
| break scope cleanup | O(|break| × |scope_depth|) | C9: implicit SCOPE cleanup on break |
| Output writes on break path | O(d×N×O) | C2: break path checked for output writes |

All polynomial. The decidability check approved break/continue with conditions C9 (static resolution, implicit scope cleanup). The `find-byte` example demonstrates correct interaction: `output index = -1` before the loop ensures writes_output is satisfied on both the break path and the normal exit path.

### 3.B.8 What's Lost (vs Candidate A)

Nothing lost. break/continue and bool are strictly additive. Pipeline syntax is sugar over sequential calls.

**What's gained:** Search patterns (find-byte, find-first-match) become 20–30% more token-efficient. Pipeline syntax reduces composition verbosity for linear processing chains.

---

## 4. Candidate C: Expressive

*Pushes toward maximum expressiveness within decidability bounds.*

### 4.C.1 Differences from Candidate B

| Feature | Candidate B | Candidate C |
|---------|-------------|-------------|
| `for` loop | Not available | `for i in 0..n` bounded iteration |
| CALL in expressions | Not available | Allowed for infallible calls only |
| `try` expression | Not available | `try call f x else fail ...` |
| `return` | Not available | Early exit from behavior |

### 4.C.2 Additional EBNF (extends Candidate B)

```ebnf
(* for loop — bounded integer iteration *)
for_stmt        = "for" identifier "in" expression ".." expression { impl_stmt } "end" ;
(* Desugars to: i = start; while i < end limit (end-start) { ... i = i + 1 } end *)

(* CALL in expressions — ONLY for infallible behaviors *)
primary         = ... | inline_call ;
inline_call     = "call" identifier "(" expression { "," expression } ")" ;
(* Compile error if the called behavior has error ports *)

(* try expression — sugar for call + error check *)
try_stmt        = "try" identifier "=" "call" identifier { argument }
                  "else" fail_stmt ;
(* Desugars to: _r = call f args; if _r.failed { fail ... } end; name = _r *)

(* return — early exit, all outputs must be written before this point *)
return_stmt     = "return" ;
```

### 4.C.3 Program 1: Hello World

Same as Candidates A/B.

### 4.C.4 Program 2: Three-Behavior Pipeline

**validate-input (leaf) — using for loop:**
```
behavior validate-input

description
  Validates raw input bytes. Checks non-empty, within bounds, valid header.

contract
  input raw bytes 4096
  input raw_length int 8
  output cleaned bytes 4096
  output cleaned_length int 8
  error code int 4
  error message string 256
  guarantees pure writes_output

hash a3f7c910

implementation
  if raw_length <= 0
    fail code = 1 message = "Input is empty"
  end
  if raw_length > 4096
    fail code = 2 message = "Input exceeds maximum size"
  end
  if raw[0] != 1
    fail code = 3 message = "Invalid header byte"
  end
  output cleaned_length = raw_length
  for i in 0..raw_length
    output cleaned[i] = raw[i]
  end
end
```

**transform-data (leaf) — using for loop:**
```
behavior transform-data

description
  Transforms validated data. XORs each byte with 0xAA, increments by 1.

contract
  input data bytes 4096
  input length int 8
  output result bytes 4096
  output result_length int 8
  error code int 4
  error message string 256
  guarantees pure writes_output

hash b40e82d1

implementation
  output result_length = length
  for i in 0..length
    b = data[i]
    if b == 255
      fail code = 10 message = "Invalid byte 0xFF in data"
    end
    output result[i] = bit_xor(b, 170) + 1
  end
end
```

**process-pipeline (composition) — using try:**
```
behavior process-pipeline

description
  Wires validate-input into transform-data with error propagation.

contract
  input raw bytes 4096
  input raw_length int 8
  output processed bytes 8192
  output processed_length int 8
  error stage int 4
  error code int 4
  error message string 256
  requires validate-input@a3f7c910 transform-data@b40e82d1
  guarantees writes_output

hash e5d019f3

composition
  try validated = call validate-input raw raw_length
    else fail stage = 1 code = validated.error.code message = validated.error.message

  try transformed = call transform-data validated.cleaned validated.cleaned_length
    else fail stage = 2 code = transformed.error.code message = transformed.error.message

  set processed = transformed.result
  output processed_length = transformed.result_length
end
```

### 4.C.5 Token Count Summary

| Component | Candidate C | Candidate B | Python | C/Python |
|-----------|-------------|-------------|--------|----------|
| hello-world | ~40 | ~40 | ~15 | 2.7x |
| validate-input | ~145 | ~170 | ~50 | 2.9x |
| transform-data | ~80 | ~100 | ~25 | 3.2x |
| process-pipeline | ~100 | ~115 | ~40 | 2.5x |
| **Total pipeline** | **~325** | **~385** | **~115** | **2.8x** |

The `for` loop saves ~15 tokens per loop (no manual counter init/increment). `try` saves ~15 tokens per error-handling block. Combined: ~60 token savings on the pipeline (16% reduction vs Candidate B).

### 4.C.6 Compiler Verification

Same as Candidate B, plus:

| Check | Complexity | Constraints Met |
|-------|-----------|----------------|
| for loop bounds | O(|ForLoops|) | C4: bounded iteration, no hidden allocation |
| Inline CALL type check | O(|InlineCalls|) | Verify callee has no error ports |
| try desugaring | O(|TryStmts|) | Sugar over call + if/fail |
| return output check | O(|ReturnPoints| × |O|) | C10: all outputs written before return |
| return resource check | O(|ReturnPoints| × |H|) | C10: all allocations freed before return |

All polynomial. Key restriction: inline CALL (`call f(x)` in expression position) is only valid when `f` has zero error ports. The compiler checks this at the call site by inspecting the callee's contract.

### 4.C.7 What's Lost (vs Candidate B)

**Added risk:**
- `for i in 0..n` where `n` is a runtime value — the loop bound is not statically known. The decidability check's C4 recommends bounded integer ranges. `for i in 0..n` desugars to `while i < n`, so it is not more dangerous than while, but it looks more "bounded" than it is.
- Inline CALL for infallible behaviors adds expression-tree complexity to purity checking and type inference. The decidability check accepted this conditionally.
- `try` is syntactic sugar that could be desugared at parse time. No decidability impact, but it is a second form for error handling (violating "one canonical form").
- `return` creates multiple exit points. The compiler must check outputs/resources at each exit. This is decidable (C10) but adds verification surface.

**Grammar complexity:** Candidate C has ~15 more productions than A. More productions = more parse states = slightly more opportunity for LLM generation errors.

---

## 5. Evaluation Matrix

### 5.1 Scoring (1–5 scale, 5 = best)

| Criterion | Weight | Candidate A | Candidate B | Candidate C |
|-----------|--------|-------------|-------------|-------------|
| Token efficiency | 30% | 3 (3.4x Python) | 4 (3.3x Python) | 5 (2.8x Python) |
| LLM generation accuracy | 25% | 5 (simplest grammar) | 4 (break/continue well-known) | 3 (more forms, inline CALL) |
| Decidability margin | 15% | 5 (maximally safe) | 5 (all conditions met) | 4 (inline CALL adds edge cases) |
| Implementation effort | 15% | 4 (expression parser + desugarer) | 4 (+ break/continue handling) | 3 (+ for, inline CALL, try, return) |
| Error handling clarity | 10% | 4 (if/fail explicit) | 4 (same) | 5 (try is cleaner) |
| Migration from v1 | 5% | 4 (mechanical + manual error restructuring) | 4 (same) | 4 (same) |

### 5.2 Weighted Scores

| Candidate | Score |
|-----------|-------|
| **A: Conservative** | 0.30×3 + 0.25×5 + 0.15×5 + 0.15×4 + 0.10×4 + 0.05×4 = **4.10** |
| **B: Balanced** | 0.30×4 + 0.25×4 + 0.15×5 + 0.15×4 + 0.10×4 + 0.05×4 = **4.15** |
| **C: Expressive** | 0.30×5 + 0.25×3 + 0.15×4 + 0.15×3 + 0.10×5 + 0.05×4 = **3.90** |

### 5.3 Sensitivity Analysis

The ranking is sensitive to the weight on token efficiency vs LLM accuracy:

- If token efficiency weight increases to 40%: C wins (4.10 vs B at 4.05)
- If LLM accuracy weight increases to 35%: A wins (4.30 vs B at 4.15)
- At current weights: **B wins by a narrow margin over A**

The key insight: Candidate B captures most of C's token savings (break/continue help the most common patterns) without C's grammar complexity. The gap between B and C (3.3x vs 2.8x Python) comes primarily from `for` loops and `try`, which are syntactic sugar that can be added later without grammar-breaking changes.

---

## 6. Assessment

### The balanced middle — the case for Candidate B

This is the study's own reading of the trade-offs, **not a project decision** —
nothing here is chosen or built. If one *were* choosing, the balanced middle (B) is
the natural default, for these reasons:

1. **Token efficiency is sufficient.** The gap from v1 (8x Python) to Candidate B (3.3x Python) is a 58% reduction. The remaining gap vs Candidate C (3.3x vs 2.8x) is 15% — achievable by adding `for` and `try` as sugar later.

2. **Grammar simplicity matters for zero-shot LLM generation.** The retrospective showed Sigil has zero training data. Every additional grammar form is an additional error source. Candidate B adds only break/continue (universal across languages) and bool (trivial). Candidate C adds four new constructs.

3. **Candidate B is a natural stepping stone to C.** If validation experiments (Phase 4) show that LLMs struggle with `while` loop patterns, `for` can be added. If `if validated.failed / fail ...` patterns are too verbose, `try` can be added. These are backward-compatible additions. Starting with B and evolving to C is risk-free; starting with C and removing features is impossible.

4. **Break/continue is the highest-value addition.** Search patterns (find-byte, find-first-match, early-exit validation) are common in real programs. Without break, these require boolean flags that add 15–25 tokens per loop. Break is universally understood by LLMs, has zero decidability cost, and the decidability check approved it unconditionally (with C9 conditions).

5. **All decidability properties are maintained or improved.** Verification complexity drops from exponential to polynomial. Guarantees become transitive. Error handling becomes structural. The decidability check signed off on all features in Candidate B.

### A B → C evolution path (hypothetical)

If B were the starting point, these are the C features that could be added later, each backward-compatible:

| Feature | Add if... | Complexity |
|---------|-----------|-----------|
| `for i in 0..n` | while loops cause frequent LLM errors | Low (desugars to while) |
| `try` expression | if/fail pattern is too verbose | Low (desugars to if/fail) |
| `return` | early exit patterns are common | Medium (multiple exit point analysis) |
| Inline CALL | infallible call verbosity is a problem | Medium (expression-level call resolution) |

Each addition is backward-compatible with Candidate B's grammar.

---

## 7. Comparison with Previous Grammar Design Report

An earlier grammar study recommended lowercase keywords + inferred sizes + auto-computed hashes (an ~18% token savings). This study **pivoted on one of those**: it rejects auto-computed hashes — hashes are always explicit in source, computed by the compiler and copied by the AI, which preserves content-addressed version pinning. (That earlier survey's other finding — *why* a flat, line-based, contract-first form beats JSON / Python / LLVM-IR / S-expressions — is recorded in [DESIGN-MODEL §2](DESIGN-MODEL.md).)

This study's Candidate B includes those changes AND solves the core architectural gap:

| Change | Earlier study | This study | Token Impact |
|--------|---------------|-----------|-------------|
| Lowercase keywords | Yes | Yes | 5–12% |
| Inferred sizes | Yes | Yes | 5–10% |
| Hash always explicit | No (manual) | Yes (compiler-provided, AI copies) | ~0% (error elimination) |
| **Expression layer** | **No** | **Yes** | **50–70%** |
| **Structured control flow** | **No** | **Yes** | **20–30%** |
| **Error handling** | **No** | **Yes** | **+10% (contracts) but catches real bugs** |
| **break/continue** | **No** | **Yes** | **5–15% for search patterns** |

The earlier study's recommendation was correct for their scope (syntax-only changes). This study went further and solved the expression-level gap that the retrospective identified as the primary problem.

---

## 8. Hash Algorithm v2 (Updated Specification)

All candidates use this hash algorithm:

1. Let *data* be the empty byte sequence.
2. Append `"INPUTS:"` as UTF-8 bytes.
3. For each `input` in declaration order: append name (UTF-8) + type_byte + size (8-byte LE).
4. Append `"OUTPUTS:"` as UTF-8 bytes.
5. For each `output` in declaration order: append name (UTF-8) + type_byte + size (8-byte LE).
6. Append `"ERRORS:"` as UTF-8 bytes.
7. For each `error` in declaration order: append name (UTF-8) + type_byte + size (8-byte LE).
8. Append `"REQUIRES:"` as UTF-8 bytes.
9. For each behavior reference in declaration order: append name (UTF-8) + `"@"` + hash (UTF-8).
10. Append `"GUARANTEES:"` as UTF-8 bytes.
11. For each guarantee in declaration order: append guarantee_byte.
12. Compute SHA-256(*data*). Return first 4 bytes as 8-char lowercase hex.

| Interpretation | type_byte |
|----------------|-----------|
| `int` | 0x01 |
| `float` | 0x02 |
| `bytes` | 0x03 |
| `string` | 0x04 (first-class in v1) |
| `bool` | 0x01 (alias for int) |

| Guarantee | guarantee_byte |
|-----------|----------------|
| `pure` | 0x01 |
| `writes_output` | 0x03 |

---

## 9. Formal Model Update

The v2 behavior definition (all candidates):

```
B = (I, O, E, R, G, Impl)
```

where:
- **I** = ordered input ports (name, interpretation, size)
- **O** = ordered output ports
- **E** = ordered error ports (NEW)
- **R** ⊂ Name × Hash = required behavior references
- **G** ⊂ {pure, writes_output} = guarantees
- **Impl** ∈ {Leaf(P*), Composite(C), Native}

Contract: `contract(B) = (I, O, E, R, G)`

Session type for failable behaviors:
```
?I₁...?Iₘ.(!O₁...!Oₙ.end ⊕ !E₁...!Eₖ.end)
```

Structural completeness:
```
SC(B) = OutputConsumption(B) ∧ InputSourcing(B) ∧ PathTermination(B)
      ∧ ResourcePairing(B) ∧ ErrorHandling(B)
```

---

## 10. Binding Constraints (from Completeness Checker)

These constraints apply to ALL candidates and MUST be enforced by the v2 compiler:

| ID | Constraint | Applies To |
|----|-----------|------------|
| C1 | Expressions are pure — no ALLOC/FREE/STORE-to-non-local in expressions | All |
| C2 | Output writes inside loop body alone don't satisfy writes_output | All |
| C3 | ALLOC inside loop body must be paired within same iteration | All |
| C5 | Transitive guarantee checking via callee contract inspection | All |
| C6 | Error outputs are part of structural completeness | All |
| C7 | Error propagation is explicit (no implicit effect inference) | All |
| C8 | No dependent types, no polymorphism, no subtyping, compile-time sizes | All |
| C9 | break/continue: static resolution, implicit scope cleanup | B, C |
| C10 | return: all outputs written, all allocations freed before exit | C only |
| C11 | **No general recursion** (NON-NEGOTIABLE) | All |
| C12 | **No higher-order behaviors** (NON-NEGOTIABLE) | All |
| C13 | **No pointer arithmetic** (NON-NEGOTIABLE) | All |

---

## 11. Open Questions for Phase 2

1. **Should `fail` require ALL error ports in one statement?** The contract redesign says yes. This simplifies verification (single error-exit point per fail) but may be verbose for behaviors with many error ports.

2. **How does `fail` inside a SCOPE interact with scope cleanup?** The compiler must insert implicit END_SCOPE cleanup before the error exit jump. This is a compiler implementation detail, not a grammar question, but it needs specification.

3. **Should `output data[i] = expr` be a single statement or two?** Currently `output data[i] = raw[i]` is one line. If output indexing is not supported, it becomes `store data expr 1 i` (raw primitive) — losing the expression benefit for the most common pattern.

4. **What about multi-output behaviors?** If a call returns multiple outputs, how are they accessed? Currently via field access: `result.field1`, `result.field2`. This works but the decidability check notes that all output fields must be consumed.

5. **Hash workflow for AI.** The compiler computes hashes and outputs them. The AI copies hashes into `requires` clauses. This is a tight compile-check-reference loop: AI writes behavior → compiles → gets hash → uses hash in dependents. Tooling should make this seamless (compiler outputs hash on successful compilation, AI captures it). No `auto` anywhere — all hashes in source are explicit, machine-verified version pins.

---

## Sources

### Working Notes (not in this repository)
The study's scratch documents — an expression-layer design, a contract redesign, and
a completeness review — were working files and are not shipped; everything from them
that survived is folded into this document.

### Research Inputs
- V2 Roadmap: `docs/V2-ROADMAP.md`
- Theoretical Foundations: `docs/theoretical-foundations.md` (24 references)
- The experiment narrative (absorbs the earlier Retrospective): `docs/THE-SIGIL-EXPERIMENT.md`
- Language Reference v1: `docs/SIGIL-LANGUAGE-REFERENCE.md`

### Key Evidence
1. SynCode (arXiv:2403.01632): 96% syntax error reduction with grammar-constrained decoding
2. GAD (NeurIPS 2024): Lowercase keywords reduce distribution distortion
3. Anka DSL (arXiv:2512.23214): One canonical form → 99.9% parse success
4. Lost in the Middle (arXiv:2307.03172): LLMs attend poorly to middle-context information
5. MoonBit (LLM4Code 2024): Flat structures improve KV-cache efficiency

---


*This study explored three candidate v2 grammars and read the balanced middle (B) as the natural default of the three. None is chosen or built — the shipped language remains the v1 grammar.*
