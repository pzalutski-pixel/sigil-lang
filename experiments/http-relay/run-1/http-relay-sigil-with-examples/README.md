# HTTP Relay - Sigil Test (With Examples)

This variant allows access to examples/ folder, similar to C++/Python having training data with examples.

## Prerequisites

- Sigil compiler built (run `build.bat` from project root)
- Runtime library built (`lib/release/sigil_runtime.lib`)
- Stdlib library built (`lib/release/sigil-stdlib.lib`)

## Setup

1. From project root, run `build.bat` if not already done
2. Navigate to this folder
3. Run `SETUP.bat` to verify environment

## Execute Test

1. Start a **fresh** Claude Code session in this folder:
   ```
   claude
   ```

2. Copy the entire prompt from `PROMPT.md` and paste it

3. Work with Claude until both executables compile and run correctly

4. Validate the result:
   ```
   # Terminal 1
   src\consumer.exe

   # Terminal 2
   echo Hello World > input.txt
   src\producer.exe
   type output.txt
   ```

5. Verify `output.txt` contains timestamp + "PROCESSED BY CONSUMER" + original content

## Record Results

1. Run `/cost` in Claude and paste output into `METRICS.md`
2. Copy the entire session transcript into `SESSION.md`
3. Fill in the execution summary in `METRICS.md`
4. Update results in `../EXPERIMENT.md`
