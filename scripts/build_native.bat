@echo off
REM Windows convenience wrapper for the cross-platform Python build driver.
python "%~dp0build_native.py" %*
exit /b %errorlevel%
