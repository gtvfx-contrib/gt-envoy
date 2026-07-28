@echo off

@REM Short alias for envoy.
if exist "%~dp0envoy.exe" (
    "%~dp0envoy.exe" %*
    exit /b %errorlevel%
)
call %~dp0envoy.bat %*
