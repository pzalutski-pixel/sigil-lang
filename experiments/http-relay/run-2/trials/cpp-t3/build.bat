@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not exist build mkdir build
cl /nologo /EHsc /Fe:build\consumer.exe /Fo:build\ src\consumer.cpp ws2_32.lib
if errorlevel 1 exit /b 1
cl /nologo /EHsc /Fe:build\producer.exe /Fo:build\ src\producer.cpp ws2_32.lib
if errorlevel 1 exit /b 1
echo BUILD OK
