@echo off
REM ---------------------------------------------------------------------------
REM PackInspect - Vite dev server launcher (ASCII only on purpose)
REM
REM Keep this file pure ASCII: cmd.exe parses .cmd using the console OEM code
REM page, so non-ASCII comments corrupt line structure on non-UTF8 systems.
REM Fixed port 1420 matches build.devUrl in tauri.conf.json.
REM ---------------------------------------------------------------------------
cd /d "%~dp0"
echo [PackInspect] starting Vite devServer on 127.0.0.1:1420, log: vite-dev.log
node "node_modules\vite\bin\vite.js" > "vite-dev.log" 2>&1
echo [PackInspect] Vite exited, EXITCODE=%ERRORLEVEL%
