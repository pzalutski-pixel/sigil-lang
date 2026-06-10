@echo off
REM HTTP Relay Experiment - Python Environment Setup
REM Run this BEFORE starting the experiment session

echo === Python Experiment Setup ===
echo.

where python >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Python not found in PATH
    exit /b 1
)
echo [OK] Python available
echo.
echo Ready for experiment.
