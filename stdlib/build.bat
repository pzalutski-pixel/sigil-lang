@echo off
REM Sigil Standard Library Build Script
REM Compiles units/*.beh to lib/<mode>/sigil-stdlib.lib
REM
REM Uses SIGIL_BUILD_MODE environment variable (default: release)

setlocal

REM Change to script directory
pushd %~dp0

REM Default to release if SIGIL_BUILD_MODE not set
if "%SIGIL_BUILD_MODE%"=="" set SIGIL_BUILD_MODE=release

REM Validate build mode
if not "%SIGIL_BUILD_MODE%"=="debug" if not "%SIGIL_BUILD_MODE%"=="release" (
    echo Error: Invalid SIGIL_BUILD_MODE=%SIGIL_BUILD_MODE%
    echo Must be 'debug' or 'release'
    popd
    exit /b 1
)

set ROOT=%~dp0..
set COMPILER=%ROOT%\compiler\target\%SIGIL_BUILD_MODE%\sigil-compiler.exe
set LIB_DIR=%ROOT%\lib\%SIGIL_BUILD_MODE%

if not exist "%COMPILER%" (
    echo Error: Compiler not built for %SIGIL_BUILD_MODE% mode.
    echo Run: compiler\build.bat
    echo   or: set SIGIL_BUILD_MODE=%SIGIL_BUILD_MODE% ^&^& compiler\build.bat
    popd
    exit /b 1
)

REM Create lib directory if needed
if not exist "%LIB_DIR%" mkdir "%LIB_DIR%"

echo Compiling Sigil Standard Library (%SIGIL_BUILD_MODE%)...

"%COMPILER%" --lib units -o "%LIB_DIR%\sigil-stdlib.lib"

if %errorlevel% neq 0 (
    echo.
    echo Stdlib compilation failed
    popd
    exit /b 1
)

echo.
echo Successfully built: lib\%SIGIL_BUILD_MODE%\sigil-stdlib.lib
echo.

popd
endlocal
