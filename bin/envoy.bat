@echo off
REM Main entry point for gt.envoy CLI
REM Usage: do [command] [args...]
REM        do --list
REM        do --info <command>

REM Set PYTHONPATH to find the gt module
set "PYTHONPATH=%~dp0..\py;%PYTHONPATH%"

REM Execute the envoy CLI module
python -m gt.envoy %*

REM Exit with the same code as the Python process
exit /b %errorlevel%
