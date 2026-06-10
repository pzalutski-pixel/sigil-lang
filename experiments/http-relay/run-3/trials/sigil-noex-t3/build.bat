@echo off
set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set NATIVE=<repo-root>\lib\release\sigil_runtime.lib
if not exist build mkdir build
echo === building producer ===
"%COMPILER%" producer.sigil --link "%STDLIB%" --native-lib "%NATIVE%" -o build\producer.exe
echo producer exit: %ERRORLEVEL%
echo === building consumer ===
"%COMPILER%" consumer.sigil --link "%STDLIB%" --native-lib "%NATIVE%" -o build\consumer.exe
echo consumer exit: %ERRORLEVEL%
