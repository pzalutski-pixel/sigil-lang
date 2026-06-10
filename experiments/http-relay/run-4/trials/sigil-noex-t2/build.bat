@echo off
setlocal
set PATH=C:\LLVM\bin;%PATH%
set HERE=%~dp0
set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib

if not exist "%HERE%build" mkdir "%HERE%build"

echo === Building helper library (t2helpers.lib) ===
"%COMPILER%" --lib "%HERE%src" -o "%HERE%build\t2helpers.lib"
if errorlevel 1 goto :error

echo === Building consumer.exe ===
"%COMPILER%" "%HERE%consumer.sigil" -o "%HERE%build\consumer.exe" --link "%HERE%build\t2helpers.lib" --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 goto :error

echo === Building producer.exe ===
"%COMPILER%" "%HERE%producer.sigil" -o "%HERE%build\producer.exe" --link "%HERE%build\t2helpers.lib" --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 goto :error

echo === Build OK ===
exit /b 0

:error
echo === Build FAILED ===
exit /b 1
