@echo off
REM Main entry point for envoy CLI
REM Usage: envoy [command] [args...]
REM        envoy --list
REM        envoy --info <command>


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

REM Fall back to `python -m envoy` (pip-installed `envoy` package, built
REM from rust/envoy-py via maturin -- see pyproject.toml).
python -m envoy %*
exit /b %errorlevel%
