# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec for envoy and engit CLI tools.

Build (from the repo root, using the bundled Python 3.14):
    V:\\repo\\gtvfx-contrib\\gt\\python\\prebuilt\\python314\\Scripts\\pyinstaller.exe envoy.spec

Or use the helper script:
    scripts\\build_exe.bat

Outputs:
    dist\\envoy.exe  -- main envoy dispatcher  (en.bat aliases this)
    dist\\engit.exe  -- git/GitHub tooling
"""

from PyInstaller.utils.hooks import copy_metadata

# Include the envoy dist-info so importlib.metadata.version('envoy') resolves
# correctly inside the frozen executables.
_metadata = copy_metadata('envoy')

# ---------------------------------------------------------------------------
# envoy
# ---------------------------------------------------------------------------

a_envoy = Analysis(
    ['scripts/_envoy_entry.py'],
    pathex=['py'],
    datas=_metadata,
    hiddenimports=[],
    hookspath=[],
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
)

# ---------------------------------------------------------------------------
# engit
# ---------------------------------------------------------------------------

a_engit = Analysis(
    ['scripts/_engit_entry.py'],
    pathex=['py'],
    datas=_metadata,
    hiddenimports=[],
    hookspath=[],
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
)

# ---------------------------------------------------------------------------
# Frozen archives
# ---------------------------------------------------------------------------

pyz_envoy = PYZ(a_envoy.pure)
pyz_engit = PYZ(a_engit.pure)

# ---------------------------------------------------------------------------
# One-file executables
# Passing binaries + datas into EXE (rather than COLLECT) triggers one-file
# mode: everything is bundled into a single self-extracting executable.
# ---------------------------------------------------------------------------

exe_envoy = EXE(
    pyz_envoy,
    a_envoy.scripts,
    a_envoy.binaries,
    a_envoy.datas,
    [],
    name='envoy',
    console=True,
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    target_arch=None,
)

exe_engit = EXE(
    pyz_engit,
    a_engit.scripts,
    a_engit.binaries,
    a_engit.datas,
    [],
    name='engit',
    console=True,
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    target_arch=None,
)

