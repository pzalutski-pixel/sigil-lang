@echo off
setlocal
set SC=<repo-root>\compiler\target\release\sigil-compiler.exe
set LIB=<repo-root>\lib\release\sigil-stdlib.lib
set NLIB=<repo-root>\lib\release\sigil_runtime.lib
if not exist build mkdir build

echo === Building consumer ===
"%SC%" consumer.sigil --link "%LIB%" --native-lib "%NLIB%" -o build\consumer.exe
if errorlevel 1 goto fail

echo === Building producer ===
"%SC%" producer.sigil --link "%LIB%" --native-lib "%NLIB%" -o build\producer.exe
if errorlevel 1 goto fail

echo === Build OK ===
exit /b 0

:fail
echo === Build FAILED ===
exit /b 1
