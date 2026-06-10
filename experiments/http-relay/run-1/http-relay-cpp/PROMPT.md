# Initial Prompt for C++ Test

Copy this prompt to start the experiment session:

---

Build an HTTP file relay system in C++ with two components:

**Consumer:**
- Listen on port 9753 for HTTP POST requests
- When request received, prepend timestamp and "PROCESSED BY CONSUMER\n" to the body
- Write result to `output.txt`

**Producer:**
- Read `input.txt`
- Send content via HTTP POST to `localhost:9753`

**Requirements:**
- Use standard C++ and Windows APIs only (no external libraries)
- Create `producer.exe` and `consumer.exe`
- Code goes in `src/` directory

**Test:**
```
# Start consumer, then in another terminal:
echo Hello World > input.txt
producer.exe
type output.txt
```

**Acceptance Criteria:**
`output.txt` should contain:
```
2025-01-13 10:30:00 PROCESSED BY CONSUMER
Hello World
```

---
