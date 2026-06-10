@echo off
setlocal
set PATH=C:\LLVM\bin;%PATH%

set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib

if not exist build mkdir build

echo === Building local behavior library (src) ===
"%COMPILER%" --lib src -o build\local.lib
if errorlevel 1 exit /b 1

echo === Building consumer ===
"%COMPILER%" consumer.sigil -o build\consumer.exe --link build\local.lib --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 exit /b 1

echo === Building producer ===
"%COMPILER%" producer.sigil -o build\producer.exe --link build\local.lib --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 exit /b 1

echo === Build complete ===
endlocal
