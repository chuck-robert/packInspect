@echo off
REM ===========================================================================
REM  PackInspect one-click launcher  (keep this file tiny and pure ASCII)
REM
REM  All real logic lives in scripts\launch.ps1. Reasons:
REM    1. cmd.exe parses .cmd byte by byte, so the file MUST use CRLF line
REM       endings and ASCII only. Any editor that rewrites it with LF endings
REM       corrupts it into "not recognized" fragments. Keeping it minimal
REM       limits the blast radius.
REM    2. PowerShell has no such constraints, so the logic stays readable and
REM       gets proper error handling.
REM ===========================================================================
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\launch.ps1"
