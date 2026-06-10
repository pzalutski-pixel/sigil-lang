@echo off
setlocal

echo Cleaning Sigil build artifacts...
echo.

call compiler\clean.bat
call runtime\clean.bat
call stdlib\clean.bat

call examples\hello-world\clean.bat
call examples\file-copy\clean.bat
call examples\http-server\clean.bat
call examples\kv-store\clean.bat
call examples\channel-test\clean.bat
call examples\parallel-test\clean.bat
call examples\producer-consumer\clean.bat
call examples\spawn-test\clean.bat
call examples\test-lib\clean.bat

echo.
echo Clean complete.
