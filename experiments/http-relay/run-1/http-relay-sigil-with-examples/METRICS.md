# Metrics - Sigil Test (With Examples)

## Context

**This test used an UNTRAINED language WITH example access.** Sigil does not exist in any LLM training data. The AI had to learn from documentation, but could also explore working examples in `../../examples/`.

## At End of Session

```
> /context
  ⎿
      Context Usage
     ⛁ ⛀ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛀   claude-opus-4-5-20251101 · 109k/200k tokens (55%)
     ⛀ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁   ⛁ System prompt: 3.4k tokens (1.7%)
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁   ⛁ System tools: 15.3k tokens (7.6%)
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁   ⛁ Memory files: 860 tokens (0.4%)
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛶ ⛶ ⛶ ⛶   ⛁ Messages: 90.0k tokens (45.0%)
     ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶   ⛶ Free space: 91k (45.3%)
```

## Execution Summary

- **Total Prompts:** 2 (1 initial + 1 user intervention)
- **Rework Cycles:** 4 total
- **First Compile Success:** No
- **First Run Success:** No
- **Passed Acceptance Criteria:** Yes
- **Messages Tokens:** 90.0k

## Token Breakdown

| Category | Tokens | Notes |
|----------|--------|-------|
| Reading grammar | ~25k | SIGIL-LANGUAGE-REFERENCE.md (1416 lines) |
| Reading contracts.registry | ~8k | stdlib contracts (514 lines) |
| Reading examples | ~15k | http-server, file-copy, hello-world |
| **Reference subtotal** | **~48k** | More than no-examples due to example reading |
| Writing/debugging code | ~42k | .beh, .sigil files, fixing errors |
| **Total messages** | **90.0k** | |

## Rework Breakdown - Language vs AI Mistakes

| Issue | Category | Notes |
|-------|----------|-------|
| OUTPUT not written on all paths | **Language** | Compiler strictness caught real issue |
| Entry point format (.sigil vs .beh) | **AI Mistake** | Grammar and examples clearly show this |
| Producer IP bug (0.0.0.0 vs 127.0.0.1) | **AI Mistake** | Pure logic error - used bind address for connect |
| Debugging loop (~10 repeated tests) | **AI Mistake** | Repeated tests without analyzing root cause |

**Summary:**
- **Language issues:** 1 (compiler caught missing OUTPUT write)
- **AI mistakes:** 3 (entry point, IP address, debugging discipline)

## Comparison: With vs Without Examples

| Metric | No Examples | With Examples | Notes |
|--------|-------------|---------------|-------|
| Messages Tokens | 70.8k | 90.0k | With examples used MORE |
| Reference Reading | ~33k | ~48k | +15k for examples |
| Coding Tokens | ~38k | ~42k | Similar |
| Language Issues | 5 | 1 | Examples helped with syntax |
| AI Mistakes | 0 | 3 | Examples led to pattern-copying without understanding |
| Tooling (hash) | 6 | 0 | Used Python correctly |

## Key Observations

1. **Examples didn't reduce total effort** - Used MORE tokens than no-examples variant
2. **Examples reduced language learning issues** - 5 → 1 (syntax was clearer)
3. **Examples increased AI mistakes** - 0 → 3 (pattern-copying without understanding)
4. **Hash computation worked** - Used Python to compute hashes correctly
5. **Debugging discipline poor** - Repeated tests without root cause analysis
6. **User intervention required** - Had to prompt AI to actually debug

## Notable Bug

The producer used `make-addr-9753` (sets IP to 0.0.0.0 for server binding) to connect as a client. Connecting to 0.0.0.0 doesn't work - needed `make-connect-addr-9753` with 127.0.0.1.

This is a pure AI logic error - the examples showed the pattern but the AI didn't understand that bind addresses differ from connect addresses.
