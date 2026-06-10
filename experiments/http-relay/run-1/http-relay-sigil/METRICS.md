# Metrics - Sigil Test (No Examples)

## Context

**This test used an UNTRAINED language.** Sigil does not exist in any LLM training data. The AI had to learn the language entirely from documentation provided during the session. No examples were accessed - only the grammar specification and stdlib contracts registry.

## At End of Session

```
> /context
  ⎿
      Context Usage
     ⛁ ⛀ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛀   claude-opus-4-5-20251101 · 90k/200k tokens (45%)
     ⛀ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁   ⛁ System prompt: 3.3k tokens (1.7%)
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛁   ⛁ System tools: 15.3k tokens (7.6%)
     ⛁ ⛁ ⛁ ⛁ ⛁ ⛁ ⛶ ⛶ ⛶ ⛶   ⛁ Memory files: 860 tokens (0.4%)
     ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶   ⛁ Messages: 70.8k tokens (35.4%)
     ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶ ⛶   ⛶ Free space: 110k (54.9%)
```

## Execution Summary

- **Total Prompts:** 1
- **Rework Cycles:** 12 (hash fixes, compiler errors, memory management)
- **First Compile Success:** No
- **First Run Success:** Yes (after successful compile)
- **Passed Acceptance Criteria:** Yes
- **Messages Tokens:** 70.8k

## Token Breakdown

| Category | Tokens | Notes |
|----------|--------|-------|
| Reading grammar | ~25k | SIGIL-LANGUAGE-REFERENCE.md (1416 lines) |
| Reading contracts.registry | ~8k | stdlib contracts (514 lines) |
| **Reference subtotal** | **~33k** | Required because language is untrained |
| Writing/debugging code | ~38k | .beh, .sigil files, fixing errors |
| **Total messages** | **70.8k** | |

## Fair Comparison with C++

C++ is extensively represented in LLM training data. Sigil has zero training data - the AI learned it entirely from in-context documentation during this session.

| Metric | C++ | Sigil (raw) | Sigil (coding only) |
|--------|-----|-------------|---------------------|
| Messages Tokens | 10.4k | 70.8k | ~38k |
| Reference Reading | 0 | ~33k | (excluded) |
| **Fair Ratio** | 1x | 6.8x | **3.6x** |
| Rework Cycles | 0 | 12 | 12 |
| First Compile | Yes | No | No |

**Key insight:** When excluding reference reading (which C++ gets "for free" from training), Sigil required 3.6x more tokens for actual coding work.

## Rework Breakdown

| Issue | Count | Category | Notes |
|-------|-------|----------|-------|
| Hash mismatches | 6 | Tooling | Computed wrong hashes, fixed by compiler feedback |
| OUTPUT not written | 3 | Language | Static analysis requires all paths write to OUTPUT |
| Memory management | 2 | Language | ALLOC/FREE must be balanced on all paths |
| Bash command issues | 1 | Environment | Path escaping |

**Rework by category:**
- **Tooling friction:** 6 cycles (hash computation - mechanical, could be automated)
- **Language learning:** 5 cycles (OUTPUT rules, memory management - actual language semantics)
- **Environment:** 1 cycle (bash/Windows path issues)

**Note:** Hash computation is deterministic and follows a spec. The `--fix-hashes` compiler flag exists but wasn't used. A standalone hash utility would eliminate this friction entirely.

## Files Created

- `src/consumer.sigil` - HTTP server entry point
- `src/producer.sigil` - HTTP client entry point
- `src/make-server-addr.beh` - sockaddr_in for 0.0.0.0:9753
- `src/make-client-addr.beh` - sockaddr_in for 127.0.0.1:9753
- `src/extract-http-body.beh` - Extracts body after \r\n\r\n
- `src/build-http-post.beh` - Builds HTTP POST request
- `src/add-int.beh` - Integer addition helper
- `src/sub-int.beh` - Integer subtraction helper
- `build.bat` - Build script

## Key Observations

1. **Zero training data:** AI learned Sigil entirely from docs - no prior exposure
2. **No examples accessed:** Per experiment rules, only grammar and contracts were read
3. **Hash friction:** Computing hashes manually is error-prone even with instructions
4. **Stdlib underutilization:** Didn't discover/use `htons`, `format-time`, `int-to-string` - reimplemented manually
5. **Static analysis:** Compiler strictness caught real bugs but required iteration
6. **COMPOSITION abandoned:** OUTPUT write requirements pushed toward verbose IMPLEMENTATION
