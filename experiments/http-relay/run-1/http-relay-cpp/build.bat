@echo off
setlocal

echo Building HTTP Relay System...
echo.

:: Create build directory
if not exist build mkdir build

:: Try to find Visual Studio compiler
where cl >nul 2>&1
if %errorlevel% equ 0 (
    echo Using Visual Studio compiler...
    goto :vs_build
)

:: Try to use g++ (MinGW)
where g++ >nul 2>&1
if %errorlevel% equ 0 (
    echo Using g++ compiler...
    goto :gcc_build
)

:: No compiler found, try to set up VS environment
if exist "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat" (
    echo Setting up Visual Studio 2022 environment...
    call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
    goto :vs_build
)

if exist "C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvarsall.bat" (
    echo Setting up Visual Studio 2019 environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
    goto :vs_build
)

echo Error: No C++ compiler found.
echo Please install Visual Studio or MinGW, or run from a Developer Command Prompt.
exit /b 1

:vs_build
echo Compiling consumer.cpp...
cl /nologo /EHsc /O2 /Fe:build\consumer.exe src\consumer.cpp Ws2_32.lib /link /OUT:build\consumer.exe
if %errorlevel% neq 0 (
    echo Error compiling consumer.cpp
    exit /b 1
)

echo Compiling producer.cpp...
cl /nologo /EHsc /O2 /Fe:build\producer.exe src\producer.cpp Ws2_32.lib /link /OUT:build\producer.exe
if %errorlevel% neq 0 (
    echo Error compiling producer.cpp
    exit /b 1
)

:: Clean up .obj files
del /q consumer.obj producer.obj 2>nul

goto :done

:gcc_build
echo Compiling consumer.cpp...
g++ -o build/consumer.exe src/consumer.cpp -lws2_32 -O2
if %errorlevel% neq 0 (
    echo Error compiling consumer.cpp
    exit /b 1
)

echo Compiling producer.cpp...
g++ -o build/producer.exe src/producer.cpp -lws2_32 -O2
if %errorlevel% neq 0 (
    echo Error compiling producer.cpp
    exit /b 1
)

:done
echo.
echo Build successful!
echo.
echo Executables:
echo   build\consumer.exe - HTTP server (listens on port 9753)
echo   build\producer.exe - HTTP client (reads input.txt, sends to server)
echo.
echo Usage:
echo   1. Start consumer:  build\consumer.exe
echo   2. Create input:    echo Hello World ^> input.txt
echo   3. Run producer:    build\producer.exe
echo   4. Check output:    type output.txt
echo.

endlocal
