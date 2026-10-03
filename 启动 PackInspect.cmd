@echo off
REM ===========================================================================
REM  PackInspect one-click launcher  (ASCII only on purpose)
REM
REM  Keep this file pure ASCII: cmd.exe parses .cmd files using the console OEM
REM  code page, so non-ASCII comments break line structure on non-UTF8 systems.
REM
REM  What it does:
REM    1. installs node_modules if missing
REM    2. starts the Vite dev server in a minimized window
REM    3. waits for port 1420 to accept connections
REM    4. builds and launches the desktop app (scripts\build.ps1 run)
REM  Close the app window to exit.
REM ===========================================================================
setlocal
cd /d "%~dp0"

if not exist "node_modules\vite\bin\vite.js" (
  echo [PackInspect] node_modules missing - running "npm install" first...
  call npm install
  if errorlevel 1 (
    echo [PackInspect] npm install failed.
    pause
    exit /b 1
  )
)

echo [PackInspect] starting dev server...
start "PackInspect dev server" /min cmd /c "node node_modules\vite\bin\vite.js > vite-dev.log 2>&1"

REM Wait for the port to accept connections (max ~40s)
set /a tries=0
:waitloop
set /a tries+=1
node -e "const s=require('net').connect(1420,'127.0.0.1');s.on('connect',()=>{s.end();process.exit(0)});s.on('error',()=>process.exit(1))" >nul 2>&1
if not errorlevel 1 goto ready
if %tries% geq 40 goto timeout
timeout /t 1 /nobreak >nul
goto waitloop

:timeout
echo [PackInspect] dev server did not start in time. See vite-dev.log
pause
exit /b 1

:ready
echo [PackInspect] dev server ready on http://localhost:1420
echo [PackInspect] building and launching the app (first build can take minutes)...
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\build.ps1" run
echo [PackInspect] app exited.
pause
