@echo off
cd /d "%~dp0"
set C=<repo-root>\compiler\target\release\sigil-compiler.exe
set LIB=<repo-root>\lib\release\sigil-stdlib.lib
set NLIB=<repo-root>\lib\release\sigil_runtime.lib
if not exist build mkdir build
echo === Building producer ===
"%C%" producer.sigil --link "%LIB%" --native-lib "%NLIB%" -o build\producer.exe
if errorlevel 1 goto :eof
echo === Building consumer ===
"%C%" consumer.sigil --link "%LIB%" --native-lib "%NLIB%" -o build\consumer.exe
