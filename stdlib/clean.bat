@echo off
echo Cleaning stdlib...
set ROOT=%~dp0..
if exist "%ROOT%\lib" rmdir /s /q "%ROOT%\lib"
