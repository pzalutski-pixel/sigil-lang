@echo off
REM HTTP Relay Experiment - C++ Environment Setup
REM Run this BEFORE starting the experiment session

echo === C++ Experiment Setup ===
echo.

where cl >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] C++ compiler not found
    echo [ERROR] Run from Visual Studio Developer Command Prompt
    exit /b 1
)
echo [OK] C++ compiler available
echo.
echo Ready for experiment.
