@echo off
REM Sigil Runtime Build Script for Windows
REM Builds sigil_runtime.lib using MSVC
REM
REM Uses SIGIL_BUILD_MODE environment variable (default: release)
REM Outputs to lib/debug/ or lib/release/

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

REM Find Visual Studio via vswhere (the official locator — works for any
REM edition/year/path), falling back to a few common install paths.
set "VCVARS="
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" set "VSWHERE=%ProgramFiles%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
    for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do (
        if exist "%%i\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%i\VC\Auxiliary\Build\vcvars64.bat"
    )
)
if not defined VCVARS if exist "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvars64.bat"
if defined VCVARS (
    call "%VCVARS%" > nul 2>&1
) else (
    echo Error: Visual Studio not found ^(tried vswhere and common paths^).
    echo Install the VS Build Tools with the C++ workload, or run from a Developer shell.
    popd
    exit /b 1
)

REM Create build directory
if not exist build mkdir build

REM Set compiler flags based on build mode
if "%SIGIL_BUILD_MODE%"=="debug" (
    set CL_FLAGS=/c /Od /Zi /W3 /nologo /Iinclude
) else (
    set CL_FLAGS=/c /O2 /W3 /nologo /Iinclude
)

echo Compiling Sigil Runtime (%SIGIL_BUILD_MODE%)...

REM Compile each source file
cl %CL_FLAGS% ^
    src\runtime.c ^
    src\task.c ^
    src\scheduler.c ^
    src\worker.c ^
    src\channel.c ^
    src\net.c ^
    src\file.c ^
    src\console.c ^
    src\time.c ^
    src\memory.c ^
    src\os.c ^
    /Fobuild\

if %errorlevel% neq 0 (
    echo Compilation failed
    exit /b 1
)

REM Create static library
lib /nologo /OUT:build\sigil_runtime.lib ^
    build\runtime.obj ^
    build\task.obj ^
    build\scheduler.obj ^
    build\worker.obj ^
    build\channel.obj ^
    build\net.obj ^
    build\file.obj ^
    build\console.obj ^
    build\time.obj ^
    build\memory.obj ^
    build\os.obj

if %errorlevel% neq 0 (
    echo Library creation failed
    exit /b 1
)

REM Cleanup object files
del build\*.obj 2>nul

REM Copy to lib/<mode>/ for unified access
set LIB_DIR=..\lib\%SIGIL_BUILD_MODE%
if not exist "%LIB_DIR%" mkdir "%LIB_DIR%"
copy /Y build\sigil_runtime.lib "%LIB_DIR%\" >nul

echo.
echo Successfully built: build\sigil_runtime.lib
echo Copied to: lib\%SIGIL_BUILD_MODE%\sigil_runtime.lib
echo.

popd
endlocal
