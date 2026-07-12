@echo off
REM Main entry point for envoy CLI
REM Usage: envoy [command] [args...]
REM        envoy --list
REM        envoy --info <command>

REM Prefer the pre-built standalone executable when available (production /
REM published bundle layout -- see .github/workflows/build-release.yml).
if exist "%~dp0..\dist\envoy.exe" (
    "%~dp0..\dist\envoy.exe" %*
    exit /b %errorlevel%
)

REM Local dev build: native Rust binary built via `cargo build --release`
REM (or the debug profile) from rust/envoy-cli, before a dist/ copy exists.
if exist "%~dp0..\rust\target\release\envoy.exe" (
    "%~dp0..\rust\target\release\envoy.exe" %*
    exit /b %errorlevel%
)
if exist "%~dp0..\rust\target\debug\envoy.exe" (
    "%~dp0..\rust\target\debug\envoy.exe" %*
    exit /b %errorlevel%
)

REM Fall back to running from source (development mode, pure Python).
set "PYTHONPATH=%~dp0..\py;%PYTHONPATH%"
python -m envoy %*
exit /b %errorlevel%
