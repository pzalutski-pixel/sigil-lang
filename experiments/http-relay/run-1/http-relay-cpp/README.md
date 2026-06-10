# HTTP Relay - C++ Test

## Prerequisites

- Visual Studio with C++ workload installed

## Setup

1. Open **Visual Studio Developer Command Prompt**
2. Navigate to this folder:
   ```
   cd path\to\ai-lang\experiments\http-relay-cpp
   ```
3. Run `SETUP.bat` to verify environment

## Execute Test

1. From the **same Developer Command Prompt**, start Claude Code:
   ```
   claude
   ```
   (Claude must run from Developer Command Prompt so `cl.exe` is available)

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
