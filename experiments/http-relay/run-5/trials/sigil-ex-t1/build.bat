@echo off
setlocal
pushd %~dp0

set PATH=C:\LLVM\bin;%PATH%
set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set LIB=<repo-root>\lib\release\sigil-stdlib.lib
set NATIVELIB=<repo-root>\lib\release\sigil_runtime.lib

if not exist build mkdir build

echo Building consumer...
"%COMPILER%" consumer.sigil --link "%LIB%" --native-lib "%NATIVELIB%" -o build\consumer.exe
if %errorlevel% neq 0 (
    echo Consumer build failed
    popd & exit /b 1
)

echo Building producer...
"%COMPILER%" producer.sigil --link "%LIB%" --native-lib "%NATIVELIB%" -o build\producer.exe
if %errorlevel% neq 0 (
    echo Producer build failed
    popd & exit /b 1
)

echo Build complete.
popd
endlocal
