# HTTP Relay - Python Test

## Prerequisites

- Python 3.x installed and in PATH

## Setup

1. Navigate to this folder
2. Run `SETUP.bat` to verify environment

## Execute Test

1. Start a **fresh** Claude Code session in this folder:
   ```
   claude
   ```

2. Copy the entire prompt from `PROMPT.md` and paste it

3. Work with Claude until both scripts run correctly

4. Validate the result:
   ```
   # Terminal 1
   python src\consumer.py

   # Terminal 2
   echo Hello World > input.txt
   python src\producer.py
   type output.txt
   ```

5. Verify `output.txt` contains timestamp + "PROCESSED BY CONSUMER" + original content

## Record Results

1. Run `/cost` in Claude and paste output into `METRICS.md`
2. Copy the entire session transcript into `SESSION.md`
3. Fill in the execution summary in `METRICS.md`
4. Update results in `../EXPERIMENT.md`
