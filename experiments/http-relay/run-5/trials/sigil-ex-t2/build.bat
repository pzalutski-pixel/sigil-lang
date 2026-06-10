@echo off
REM Build script for the HTTP file relay (consumer + producer).
setlocal

pushd %~dp0

set ROOT=<repo-root>
set COMPILER=%ROOT%\compiler\target\release\sigil-compiler.exe
set LIB=%ROOT%\lib\release

REM Ensure the LLVM linker (link.exe) is discoverable and not shadowed by any
REM Unix-style 'link' on an inherited PATH. Use a clean Windows PATH.
set PATH=C:\LLVM\bin;C:\Windows\System32;C:\Windows

if not exist build mkdir build

echo Building consumer...
"%COMPILER%" consumer\main.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\consumer.exe
if %errorlevel% neq 0 (
    echo Consumer build failed
    popd & exit /b 1
)

echo Building producer...
"%COMPILER%" producer\main.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\producer.exe
if %errorlevel% neq 0 (
    echo Producer build failed
    popd & exit /b 1
)

echo Successfully built: build\consumer.exe and build\producer.exe
popd
endlocal
