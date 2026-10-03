@echo off
REM ---------------------------------------------------------------------------
REM PackInspect - run the desktop app directly with cargo (ASCII only on purpose)
REM
REM Why not `tauri dev`: that CLI insists on running build.beforeDevCommand
REM (the npm shim), which is unreliable in non-interactive shells. The Tauri
REM binary itself does not need it: it reads build.devUrl from tauri.conf.json,
REM so pointing it at an already-running Vite server is equivalent.
REM Start the frontend first with tauri-vite.cmd.
REM ---------------------------------------------------------------------------
set "CARGO_HOME=%USERPROFILE%\.cargo"
set "RUSTUP_HOME=%USERPROFILE%\.rustup"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set "HTTP_PROXY=http://127.0.0.1:7890"
set "HTTPS_PROXY=http://127.0.0.1:7890"
call "D:\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1

cd /d "%~dp0src-tauri"
echo [PackInspect] compiling and running the desktop app, log: app-run.log
cargo run --no-default-features --color never > "..\app-run.log" 2>&1
echo [PackInspect] exited, EXITCODE=%ERRORLEVEL%
