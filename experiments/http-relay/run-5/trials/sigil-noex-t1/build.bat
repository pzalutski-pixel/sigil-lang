@echo off
setlocal
set PATH=C:\LLVM\bin;%PATH%
set SIGILC=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib

if not exist build mkdir build

echo === Refreshing leaf library sources ===
if not exist libsrc mkdir libsrc
copy /Y build-consumer-output.beh libsrc\ >nul
copy /Y build-http-post.beh libsrc\ >nul

echo === Building leaf library (mylib.lib) ===
"%SIGILC%" --lib libsrc -o build\mylib.lib
if errorlevel 1 goto :fail

echo === Building consumer ===
"%SIGILC%" consumer.sigil -o build\consumer.exe --link build\mylib.lib --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 goto :fail

echo === Building producer ===
"%SIGILC%" producer.sigil -o build\producer.exe --link build\mylib.lib --link "%STDLIB%" --native-lib "%RUNTIME%"
if errorlevel 1 goto :fail

echo === Build OK ===
exit /b 0

:fail
echo === Build FAILED ===
exit /b 1
