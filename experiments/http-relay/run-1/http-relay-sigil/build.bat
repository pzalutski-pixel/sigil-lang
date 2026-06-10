@echo off
setlocal

REM HTTP Relay Build Script
REM Compiles consumer and producer from src/ to build/

set COMPILER=..\..\compiler\target\release\sigil-compiler.exe
set STDLIB=..\..\lib\release\sigil-stdlib.lib
set RUNTIME=..\..\lib\release\sigil_runtime.lib

REM Create build directory if needed
if not exist build mkdir build

echo Building consumer...
%COMPILER% src\consumer.sigil -o build\consumer.exe --link %STDLIB% --native-lib %RUNTIME%
if errorlevel 1 (
    echo Consumer build failed
    exit /b 1
)

echo Building producer...
%COMPILER% src\producer.sigil -o build\producer.exe --link %STDLIB% --native-lib %RUNTIME%
if errorlevel 1 (
    echo Producer build failed
    exit /b 1
)

echo Build complete!
echo   build\consumer.exe
echo   build\producer.exe
