@echo off
REM Build standalone executables for envoy and engit using PyInstaller.
REM
REM Usage:
REM   scripts\build_exe.bat
REM
REM Output: dist\envoy.exe, dist\engit.exe
REM   (en.exe is NOT built -- en.bat already aliases envoy.bat -> envoy.exe)
REM
REM Requirements:
REM   PyInstaller must be installed in the Python 3.14 bundle:
REM     V:\repo\gtvfx-contrib\gt\python\prebuilt\python314\Scripts\pip install pyinstaller

setlocal

set "REPO_ROOT=%~dp0.."
set "PY314=%REPO_ROOT%\..\python\prebuilt\python314\python.exe"
set "PYINSTALLER=%REPO_ROOT%\..\python\prebuilt\python314\Scripts\pyinstaller.exe"

REM Verify the Python 3.14 interpreter is present.
if not exist "%PY314%" (
    echo ERROR: Python 3.14 not found at %PY314%
    echo        Run 'en python' or check the python bundle.
    exit /b 1
)

REM Verify PyInstaller is installed.
if not exist "%PYINSTALLER%" (
    echo ERROR: PyInstaller not found. Install it with:
    echo        %PY314% -m pip install pyinstaller
    exit /b 1
)

REM Clear PYTHONPATH / PYTHONHOME so the 3.14 interpreter isn't contaminated
REM by any system Python environment variables.
set PYTHONPATH=
set PYTHONHOME=

cd /d "%REPO_ROOT%"
echo Building envoy and engit executables...
"%PYINSTALLER%" envoy.spec --noconfirm
if %errorlevel% neq 0 (
    echo ERROR: PyInstaller build failed.
    exit /b %errorlevel%
)

echo.
echo Build complete. Executables written to:
echo   %REPO_ROOT%\dist\envoy.exe
echo   %REPO_ROOT%\dist\engit.exe
echo.
echo Add %REPO_ROOT%\dist to your PATH, or the bin\ scripts will pick them up automatically.

endlocal
