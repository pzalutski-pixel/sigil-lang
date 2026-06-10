@echo off
REM HTTP Relay Experiment - Sigil Environment Setup
REM Run this BEFORE starting the experiment session

echo === Sigil Experiment Setup ===
echo.

if not exist "..\..\compiler\target\release\sigil-compiler.exe" (
    echo [ERROR] Sigil compiler not found
    echo [ERROR] Run build.bat from project root first
    exit /b 1
)
echo [OK] Sigil compiler available

if not exist "..\..\lib\release\sigil_runtime.lib" (
    echo [ERROR] Runtime library not found
    echo [ERROR] Run build.bat from project root first
    exit /b 1
)
echo [OK] Runtime library available

if not exist "..\..\lib\release\sigil-stdlib.lib" (
    echo [ERROR] Stdlib library not found
    echo [ERROR] Run build.bat from project root first
    exit /b 1
)
echo [OK] Stdlib library available
echo.
echo Ready for experiment.
