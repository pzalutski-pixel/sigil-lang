@echo off
REM Build script for hello-world example
REM Uses SIGIL_BUILD_MODE environment variable (default: release)
setlocal

pushd %~dp0

if "%SIGIL_BUILD_MODE%"=="" set SIGIL_BUILD_MODE=release

set ROOT=%~dp0..\..
set COMPILER=%ROOT%\compiler\target\%SIGIL_BUILD_MODE%\sigil-compiler.exe
set LIB=%ROOT%\lib\%SIGIL_BUILD_MODE%

if not exist "%COMPILER%" (
    echo Error: Compiler not built for %SIGIL_BUILD_MODE% mode.
    echo Run: compiler\build.bat
    popd & exit /b 1
)

if not exist "%LIB%\sigil_runtime.lib" (
    echo Error: Runtime not built for %SIGIL_BUILD_MODE% mode.
    echo Run: runtime\build.bat
    popd & exit /b 1
)

if not exist "%LIB%\sigil-stdlib.lib" (
    echo Error: Stdlib not built for %SIGIL_BUILD_MODE% mode.
    echo Run: stdlib\build.bat
    popd & exit /b 1
)

if not exist build mkdir build

echo Building hello-world (%SIGIL_BUILD_MODE%)...
"%COMPILER%" main.sigil --link "%LIB%\sigil-stdlib.lib" --native-lib "%LIB%\sigil_runtime.lib" -o build\hello-world.exe

if %errorlevel% neq 0 (
    echo Build failed
    popd & exit /b 1
)

echo Successfully built: build\hello-world.exe
popd
endlocal
