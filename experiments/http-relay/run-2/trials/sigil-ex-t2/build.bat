@echo off
REM Build script for the HTTP file relay (consumer + producer)
setlocal

pushd %~dp0

set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib

if not exist build mkdir build

echo Building consumer...
"%COMPILER%" consumer\consumer.sigil --link "%STDLIB%" --native-lib "%RUNTIME%" -o build\consumer.exe
if %errorlevel% neq 0 (
    echo Consumer build failed
    popd & exit /b 1
)

echo Building producer...
"%COMPILER%" producer\producer.sigil --link "%STDLIB%" --native-lib "%RUNTIME%" -o build\producer.exe
if %errorlevel% neq 0 (
    echo Producer build failed
    popd & exit /b 1
)

echo Successfully built build\consumer.exe and build\producer.exe
popd
endlocal
