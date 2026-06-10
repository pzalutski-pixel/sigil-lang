@echo off
setlocal

if "%SIGIL_BUILD_MODE%"=="" set SIGIL_BUILD_MODE=release
echo Building Sigil (%SIGIL_BUILD_MODE% mode)
echo.

:: Build order: compiler -> runtime -> stdlib -> examples

echo [1/4] Building compiler...
call compiler\build.bat
if errorlevel 1 (
    echo ERROR: Compiler build failed
    exit /b 1
)
echo.

echo [2/4] Building runtime...
call runtime\build.bat
if errorlevel 1 (
    echo ERROR: Runtime build failed
    exit /b 1
)
echo.

echo [3/4] Building stdlib...
call stdlib\build.bat
if errorlevel 1 (
    echo ERROR: Stdlib build failed
    exit /b 1
)
echo.

echo [4/4] Building examples...

echo   Building hello-world...
call examples\hello-world\build.bat
if errorlevel 1 (echo ERROR: hello-world failed & exit /b 1)

echo   Building file-copy...
call examples\file-copy\build.bat
if errorlevel 1 (echo ERROR: file-copy failed & exit /b 1)

echo   Building http-server...
call examples\http-server\build.bat
if errorlevel 1 (echo ERROR: http-server failed & exit /b 1)

echo   Building kv-store...
call examples\kv-store\build.bat
if errorlevel 1 (echo ERROR: kv-store failed & exit /b 1)

echo   Building channel-test...
call examples\channel-test\build.bat
if errorlevel 1 (echo ERROR: channel-test failed & exit /b 1)

echo   Building parallel-test...
call examples\parallel-test\build.bat
if errorlevel 1 (echo ERROR: parallel-test failed & exit /b 1)

echo   Building producer-consumer...
call examples\producer-consumer\build.bat
if errorlevel 1 (echo ERROR: producer-consumer failed & exit /b 1)

echo   Building spawn-test...
call examples\spawn-test\build.bat
if errorlevel 1 (echo ERROR: spawn-test failed & exit /b 1)

echo   Building test-lib...
call examples\test-lib\build.bat
if errorlevel 1 (echo ERROR: test-lib failed & exit /b 1)

echo.
echo Build complete.
