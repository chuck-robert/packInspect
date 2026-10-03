@echo off
REM ---------------------------------------------------------------------------
REM PackInspect - Tauri desktop dev launcher (ASCII only on purpose)
REM
REM Keep this file pure ASCII: cmd.exe parses .cmd using the console OEM code
REM page, so non-ASCII comments corrupt line structure on non-UTF8 systems.
REM
REM --no-dev-server skips build.beforeDevCommand (it depends on the npm shim,
REM which is unreliable in non-interactive shells). Start the frontend dev
REM server separately with tauri-vite.cmd.
REM ---------------------------------------------------------------------------
set "CARGO_HOME=%USERPROFILE%\.cargo"
set "RUSTUP_HOME=%USERPROFILE%\.rustup"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

REM Enable when the network is restricted (local proxy on port 7890)
set "HTTP_PROXY=http://127.0.0.1:7890"
set "HTTPS_PROXY=http://127.0.0.1:7890"

REM Inject the MSVC linker environment (cargo needs link.exe)
call "D:\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1

cd /d "%~dp0"
echo [PackInspect] building and launching the desktop app, log: tauri-dev.log
node "node_modules\@tauri-apps\cli\tauri.js" dev --no-dev-server > "tauri-dev.log" 2>&1
echo [PackInspect] exited, EXITCODE=%ERRORLEVEL%
