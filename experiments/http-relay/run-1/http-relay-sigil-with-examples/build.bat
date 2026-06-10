@echo off
setlocal

set COMPILER=..\..\compiler\target\release\sigil-compiler.exe
set STDLIB=..\..\lib\release\sigil-stdlib.lib
set RUNTIME=..\..\lib\release\sigil_runtime.lib

if not exist build mkdir build

echo Building consumer...
%COMPILER% src\consumer.sigil src\consumer.beh src\bind-9753.beh src\make-addr-9753.beh src\extract-http-body.beh --link %STDLIB% --native-lib %RUNTIME% -o build\consumer.exe
if errorlevel 1 (
    echo Consumer build failed!
    exit /b 1
)

echo Building producer...
%COMPILER% src\producer.sigil src\producer.beh src\make-connect-addr-9753.beh src\build-http-post.beh --link %STDLIB% --native-lib %RUNTIME% -o build\producer.exe
if errorlevel 1 (
    echo Producer build failed!
    exit /b 1
)

echo Build complete!
echo.
echo To test:
echo   1. Start consumer:  build\consumer.exe
echo   2. In another terminal:
echo      echo Hello World ^> input.txt
echo      build\producer.exe
echo      type output.txt
