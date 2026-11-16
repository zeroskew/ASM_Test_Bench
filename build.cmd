@echo off
rem Builds the workspace inside a Visual Studio 2022 developer environment.
rem After every successful release build the exe and README are copied into dist\.
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>nul
cd /d "%~dp0"
cargo %*
if errorlevel 1 exit /b 1

set "RELEASE="
for %%a in (%*) do if /i "%%~a"=="--release" set "RELEASE=1"
if not defined RELEASE exit /b 0
if not exist target\release\asm_test_bench.exe exit /b 0

if not exist dist mkdir dist
copy /y target\release\asm_test_bench.exe dist\ >nul || (echo Could not copy into dist\ - is asm_test_bench.exe still running? & exit /b 1)
copy /y README.md dist\ >nul
rem The examples are embedded in the exe (build.rs), so no examples\ folder is needed.
echo dist\ updated.
