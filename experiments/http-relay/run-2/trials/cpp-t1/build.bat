@echo off
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"

set ROOT=%~dp0
if not exist "%ROOT%build" mkdir "%ROOT%build"

cl /EHsc /nologo /O2 /Fe:"%ROOT%build\consumer.exe" /Fo:"%ROOT%build\consumer.obj" "%ROOT%src\consumer.cpp" ws2_32.lib
if errorlevel 1 exit /b 1

cl /EHsc /nologo /O2 /Fe:"%ROOT%build\producer.exe" /Fo:"%ROOT%build\producer.obj" "%ROOT%src\producer.cpp" ws2_32.lib
if errorlevel 1 exit /b 1

echo BUILD OK
