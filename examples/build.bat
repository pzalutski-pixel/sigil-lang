@echo off
REM Master build script for all Sigil examples
setlocal

pushd %~dp0

echo ========================================
echo Building all Sigil examples
echo ========================================
echo.

set FAILED=0

REM hello-world (no stdlib)
echo [1/8] Building hello-world...
call hello-world\build.bat
if %errorlevel% neq 0 set FAILED=1

REM channel-test
echo [2/8] Building channel-test...
call channel-test\build.bat
if %errorlevel% neq 0 set FAILED=1

REM file-copy
echo [3/8] Building file-copy...
call file-copy\build.bat
if %errorlevel% neq 0 set FAILED=1

REM spawn-test
echo [4/8] Building spawn-test...
call spawn-test\build.bat
if %errorlevel% neq 0 set FAILED=1

REM parallel-test
echo [5/8] Building parallel-test...
call parallel-test\build.bat
if %errorlevel% neq 0 set FAILED=1

REM kv-store
echo [6/8] Building kv-store...
call kv-store\build.bat
if %errorlevel% neq 0 set FAILED=1

REM producer-consumer
echo [7/8] Building producer-consumer...
call producer-consumer\build.bat
if %errorlevel% neq 0 set FAILED=1

REM http-server
echo [8/8] Building http-server...
call http-server\build.bat
if %errorlevel% neq 0 set FAILED=1

echo.
echo ========================================
if %FAILED% neq 0 (
    echo Some examples failed to build
    popd & exit /b 1
) else (
    echo All examples built successfully
)
echo ========================================

popd
endlocal
