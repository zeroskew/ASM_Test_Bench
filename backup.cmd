@echo off
rem Zips everything needed to rebuild ASM Test Bench (sources, build scripts, assets, examples, the
rem tutorials) into backups\ASM_Test_Bench_<date>_<time>.zip, or into the folder given as the
rem first argument:
rem     backup.cmd                 -> backups\ASM_Test_Bench_20261113_101500.zip
rem     backup.cmd D:\Backups      -> D:\Backups\ASM_Test_Bench_20261113_101500.zip
rem Skips build output (target\, dist\) and earlier backups.
setlocal
cd /d "%~dp0"

set "OUTDIR=%~1"
if "%OUTDIR%"=="" set "OUTDIR=%~dp0backups"
if not exist "%OUTDIR%" mkdir "%OUTDIR%" || (echo Can't create "%OUTDIR%" & exit /b 1)

for /f %%t in ('powershell -NoProfile -Command "Get-Date -Format yyyyMMdd_HHmmss"') do set "STAMP=%%t"
set "ZIP=%OUTDIR%\ASM_Test_Bench_%STAMP%.zip"

rem Collect only what exists (Cargo.lock / assets / examples may be missing on a fresh tree).
set "FILES=Cargo.toml .cargo src build.cmd dist.cmd build_publish_exe.cmd backup.cmd README.md .gitignore"
for %%f in (Cargo.lock assets examples tutorial advanced build.rs tools) do if exist "%%f" call set "FILES=%%FILES%% %%f"

rem Windows' own tar.exe (bsdtar, ships with Windows 10/11): -a picks zip from the extension.
rem Called by full path so Git's GNU tar on PATH (which can't write zips) isn't used instead.
"%SystemRoot%\System32\tar.exe" -a -c -f "%ZIP%" --exclude "*/target" --exclude "*.tmp" --exclude "*.png" %FILES%
if errorlevel 1 (
    echo Backup FAILED.
    if exist "%ZIP%" del "%ZIP%"
    exit /b 1
)

for %%f in ("%ZIP%") do echo Backup written: %%~ff  (%%~zf bytes)
endlocal
