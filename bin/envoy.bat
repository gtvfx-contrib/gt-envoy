@echo off
REM Main entry point for envoy CLI
REM Usage: envoy [command] [args...]
REM        envoy --list
REM        envoy --info <command>

REM Prefer the pre-built standalone executable when available.
if exist "%~dp0..\dist\envoy.exe" (
    "%~dp0..\dist\envoy.exe" %*
    exit /b %errorlevel%
)

REM Fall back to running from source (development mode).
set "PYTHONPATH=%~dp0..\py;%PYTHONPATH%"
python -m envoy %*
exit /b %errorlevel%
