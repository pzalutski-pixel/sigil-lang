@echo off
setlocal
set ROOT=<repo-root>
set COMPILER=%ROOT%\compiler\target\release\sigil-compiler.exe
set LIB=%ROOT%\lib\release

if not exist build mkdir build

echo Building consumer...
"%COMPILER%" consumer\main.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\consumer.exe
if %errorlevel% neq 0 ( echo CONSUMER BUILD FAILED & exit /b 1 )

echo Building producer...
"%COMPILER%" producer\main.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\producer.exe
if %errorlevel% neq 0 ( echo PRODUCER BUILD FAILED & exit /b 1 )

echo Build OK
endlocal
