@echo off
REM Build the HTTP file relay (producer + consumer) for the Sigil stdlib trial.
setlocal
set PATH=C:\LLVM\bin;%PATH%
set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib
set HERE=%~dp0
set OUT=%HERE%build

if not exist "%OUT%" mkdir "%OUT%"

echo === Building consumer ===
"%COMPILER%" "%HERE%consumer.sigil" -o "%OUT%\consumer.exe" --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 (echo CONSUMER BUILD FAILED & exit /b 1)

echo === Building producer ===
"%COMPILER%" "%HERE%producer.sigil" -o "%OUT%\producer.exe" --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 (echo PRODUCER BUILD FAILED & exit /b 1)

echo === Build complete ===
endlocal
