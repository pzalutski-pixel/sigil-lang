@echo off
setlocal
set PATH=C:\LLVM\bin;%PATH%
set COMPILER=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib

if not exist build mkdir build

rem Stage producer sources
if not exist producer mkdir producer
copy /Y producer.sigil producer\ >nul
copy /Y build-request.beh producer\ >nul

rem Stage consumer sources
if not exist consumer mkdir consumer
copy /Y consumer.sigil consumer\ >nul
copy /Y build-output.beh consumer\ >nul

echo === Compiling consumer ===
"%COMPILER%" consumer\consumer.sigil -o build\consumer.exe --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 goto :error

echo === Compiling producer ===
"%COMPILER%" producer\producer.sigil -o build\producer.exe --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 goto :error

echo === Build OK ===
exit /b 0

:error
echo === Build FAILED ===
exit /b 1
