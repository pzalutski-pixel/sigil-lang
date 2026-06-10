@echo off
REM Build script for the HTTP file relay (consumer + producer).
setlocal

pushd %~dp0

set PATH=C:\LLVM\bin;%PATH%

set ROOT=<repo-root>
set COMPILER=%ROOT%\compiler\target\release\sigil-compiler.exe
set LIB=%ROOT%\lib\release

if not exist build mkdir build

echo Building consumer...
"%COMPILER%" consumer\consumer.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\consumer.exe
if %errorlevel% neq 0 (
    echo Consumer build failed
    popd & exit /b 1
)

echo Building producer...
"%COMPILER%" producer\producer.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\producer.exe
if %errorlevel% neq 0 (
    echo Producer build failed
    popd & exit /b 1
)

echo Successfully built: build\consumer.exe and build\producer.exe
popd
endlocal
