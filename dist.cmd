@echo off
rem Builds the release binary; build.cmd copies it (and README.md) into dist\.
cd /d "%~dp0"
call "%~dp0build.cmd" build --release || exit /b 1
echo dist\ ready:
dir /b dist
