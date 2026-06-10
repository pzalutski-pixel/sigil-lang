@echo off
setlocal
set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib

if not exist build mkdir build

echo === Building consumer ===
"%COMPILER%" consumer.sigil --link "%STDLIB%" --native-lib "%RUNTIME%" -o build\consumer.exe
if errorlevel 1 (echo CONSUMER BUILD FAILED & exit /b 1)

echo === Building producer ===
"%COMPILER%" producer.sigil --link "%STDLIB%" --native-lib "%RUNTIME%" -o build\producer.exe
if errorlevel 1 (echo PRODUCER BUILD FAILED & exit /b 1)

echo === BUILD OK ===
