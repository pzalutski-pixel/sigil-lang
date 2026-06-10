# Initial Prompt for Sigil Test

Copy this prompt to start the experiment session:

---

Build an HTTP file relay system in Sigil with two components:

**Consumer:**
- Listen on port 9753 for HTTP POST requests
- When request received, prepend timestamp and "PROCESSED BY CONSUMER\n" to the body
- Write result to `output.txt`

**Producer:**
- Read `input.txt`
- Send content via HTTP POST to `localhost:9753`

**Reference Files (read these first):**
- Grammar: `../../docs/SIGIL-LANGUAGE-REFERENCE.md`
- Stdlib contracts: `../../lib/release/.generated/contracts.registry`

**CRITICAL: Follow the grammar EXACTLY.**
- Every construct must match the specification precisely
- Do NOT guess or assume syntax from other languages
- Use `.beh` extension for behaviors, `.pattern` for patterns, `.sigil` for entry points
- When unsure, re-read the relevant grammar section
- The grammar is complete and authoritative - if it's not in the grammar, don't use it

**HASH COMPUTATION (Section 5.7 of grammar):**
- Every behavior MUST have a valid HASH computed per the algorithm in Section 5.7
- Read Section 5.7 "HASH" carefully - it specifies exactly how to compute the hash:
  1. Build data string: "INPUTS:" + (for each input: name + type_byte + size_le64)
  2. Append "OUTPUTS:" + (for each output: name + type_byte + size_le64)
  3. Append "REQUIRES:" + (for each ref: name + "@" + hash)
  4. Append "GUARANTEES:" + (for each guarantee: guarantee_byte)
  5. Hash = first 4 bytes of SHA-256(data) as 8-char lowercase hex
- Type bytes: int=0x01, bytes=0x02. Guarantee bytes: pure=0x01, no_alloc=0x02, writes_output=0x04
- Do NOT use placeholder hashes like 00000000 - compute the actual hash
- The compiler will reject incorrect hashes

**Build:**
- Compiler: `../../compiler/target/release/sigil-compiler.exe`
- Sigil stdlib: `--link ../../lib/release/sigil-stdlib.lib`
- C runtime: `--native-lib ../../lib/release/sigil_runtime.lib`
- Create `build.bat` in this directory to compile from `src/` to `build/`
- **To run batch files:** After creating build.bat, run with `cmd.exe /c build.bat` from this directory (not `./build.bat`, not `cd && build.bat`)

**Important:** Do NOT read or explore any files outside this directory except the two reference files listed above. Do NOT look at examples.

**Test:**
```
# Start consumer, then in another terminal:
echo Hello World > input.txt
build\producer.exe
type output.txt
```

**Acceptance Criteria:**
`output.txt` should contain:
```
2025-01-13 10:30:00 PROCESSED BY CONSUMER
Hello World
```

---
