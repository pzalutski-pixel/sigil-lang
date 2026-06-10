@echo off
setlocal
set ROOT=<repo-root>
set COMPILER=%ROOT%\compiler\target\release\sigil-compiler.exe
set LIB=%ROOT%\lib\release
set DIR=%~dp0

if not exist "%DIR%build" mkdir "%DIR%build"

echo Building consumer...
"%COMPILER%" "%DIR%consumer\main.sigil" --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o "%DIR%build\consumer.exe"
if %errorlevel% neq 0 (
    echo Consumer build failed
    exit /b 1
)
echo Built consumer.exe

echo Building producer...
"%COMPILER%" "%DIR%producer\main.sigil" --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o "%DIR%build\producer.exe"
if %errorlevel% neq 0 (
    echo Producer build failed
    exit /b 1
)
echo Built producer.exe
endlocal
