@echo off
set COMP=<repo-root>\compiler\target\release\sigil-compiler.exe
set STDLIB=<repo-root>\lib\release\sigil-stdlib.lib
set RUNTIME=<repo-root>\lib\release\sigil_runtime.lib
"%COMP%" %1 --link "%STDLIB%" --native-lib "%RUNTIME%" -o %2
