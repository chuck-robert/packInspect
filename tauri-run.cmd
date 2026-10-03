@echo off
REM ---------------------------------------------------------------------------
REM PackInspect - run the desktop app directly with cargo (ASCII only on purpose)
REM
REM Why not `tauri dev`: that CLI insists on running build.beforeDevCommand
REM (the npm shim), which is unreliable in non-interactive shells. The Tauri
REM binary itself does not need it: it reads build.devUrl from tauri.conf.json,
REM so pointing it at an already-running Vite server is equivalent.
REM
REM ?????? scripts\build.ps1 run????????? PackInspect.cmd??
REM ---------------------------------------------------------------------------
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\build.ps1" run
