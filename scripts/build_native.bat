@echo off
REM Build the native envoy executable (Rust), plus the envoy Python
REM wheel (PyO3 extension via maturin).
REM
REM Replaces the old PyInstaller-based scripts\build_exe.bat -- envoy is a
REM native Rust binary now, with no bundled Python interpreter and no
REM PyInstaller step required. See rust\README.md for the full workspace
REM layout.
REM
REM Usage:
REM   scripts\build_native.bat
REM
REM Output:
REM   rust\target\release\envoy.exe
REM   rust\target\wheels\envoy-*.whl   (PyO3 extension wheel, envoy.proc/
REM                                     envoy._api/envoy.exceptions/envoy.testing)
REM
REM Requirements:
REM   - Rust toolchain (rustup) with the MSVC target, and Visual Studio Build
REM     Tools (link.exe) for the x86_64-pc-windows-msvc target.
REM   - Python 3.10+ with `pip install maturin` for the wheel build step.

setlocal

set "REPO_ROOT=%~dp0.."

where cargo >nul 2>nul
if errorlevel 1 (
    echo ERROR: cargo not found on PATH. Install Rust via https://rustup.rs/
    exit /b 1
)

cd /d "%REPO_ROOT%\rust"
echo Building the envoy native binary (cargo build --release)...
cargo build --workspace --exclude envoy-py --release
if %errorlevel% neq 0 (
    echo ERROR: cargo build failed.
    exit /b %errorlevel%
)

where maturin >nul 2>nul
if errorlevel 1 (
    echo NOTE: maturin not found on PATH -- skipping the envoy Python wheel build.
    echo       Install it with: pip install maturin
    goto :done
)

echo.
echo Building the envoy Python wheel (maturin build --release)...
cd /d "%REPO_ROOT%\rust\envoy-py"
maturin build --release
if %errorlevel% neq 0 (
    echo ERROR: maturin build failed.
    exit /b %errorlevel%
)

echo.
echo building local pyd (maturin develop --release)...
maturin develop --release
if %errorlevel% neq 0 (
    echo ERROR: maturin develop failed.
    exit /b %errorlevel%
)

:done
echo.
echo Build complete. Native binaries written to:
echo   %REPO_ROOT%\rust\target\release\envoy.exe
echo.
echo bin\envoy.bat automatically picks this up, falling back to dist\envoy.exe
echo and then to `python -m envoy`.

endlocal
