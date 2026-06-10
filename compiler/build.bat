@echo off
REM Sigil Compiler Build Script for Windows
REM Builds the compiler using Cargo
REM
REM Uses SIGIL_BUILD_MODE environment variable (default: release)
REM Command line --debug or --release overrides environment variable

setlocal

REM Change to script directory
pushd %~dp0

echo.
echo === Sigil Compiler Build ===
echo.

REM Check for cargo
where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo Error: cargo not found. Please install Rust from https://rustup.rs
    exit /b 1
)

REM Default to release if SIGIL_BUILD_MODE not set
if "%SIGIL_BUILD_MODE%"=="" set SIGIL_BUILD_MODE=release

REM Parse arguments (override environment variable)
set RUN_TESTS=0

:parse_args
if "%1"=="" goto :done_args
if "%1"=="--release" set SIGIL_BUILD_MODE=release
if "%1"=="-r" set SIGIL_BUILD_MODE=release
if "%1"=="--debug" set SIGIL_BUILD_MODE=debug
if "%1"=="-d" set SIGIL_BUILD_MODE=debug
if "%1"=="--test" set RUN_TESTS=1
if "%1"=="-t" set RUN_TESTS=1
shift
goto :parse_args
:done_args

REM Validate build mode
if not "%SIGIL_BUILD_MODE%"=="debug" if not "%SIGIL_BUILD_MODE%"=="release" (
    echo Error: Invalid SIGIL_BUILD_MODE=%SIGIL_BUILD_MODE%
    echo Must be 'debug' or 'release'
    popd
    exit /b 1
)

REM Build
echo Building %SIGIL_BUILD_MODE%...
if "%SIGIL_BUILD_MODE%"=="release" (
    cargo build --release
) else (
    cargo build
)

if %errorlevel% neq 0 (
    echo.
    echo Build FAILED
    popd
    exit /b 1
)

REM Run tests if requested
if "%RUN_TESTS%"=="1" (
    echo.
    echo Running tests...
    cargo test
    if %errorlevel% neq 0 (
        echo.
        echo Tests FAILED
        popd
        exit /b 1
    )
)

echo.
echo Build successful!
echo Output: target\%SIGIL_BUILD_MODE%\sigil-compiler.exe
echo.

popd
endlocal
