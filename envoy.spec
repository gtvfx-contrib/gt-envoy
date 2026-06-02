# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec for envoy and engit CLI tools.

Build (from the repo root, using system Python install):
    C:\\Users\\gf_th\\AppData\\Local\\Python\\bin\\python.exe

Or use the helper script:
    scripts\\build_exe.bat

Outputs:
    dist\\envoy.exe  -- main envoy dispatcher  (en.bat aliases this)
    dist\\engit.exe  -- git/GitHub tooling
    
"""

import atexit
import os
import shutil
import tempfile

from PyInstaller.utils.hooks import copy_metadata

# Include the envoy dist-info so importlib.metadata.version('envoy') resolves
# correctly inside the frozen executables.
#
# When envoy is not pip-installed (no dist-info in site-packages), generate a
# minimal dist-info on-the-fly from _version.py so the spec stays self-sufficient
# and the build never requires a prior `pip install`.
try:
    _metadata = copy_metadata('envoy')
except Exception:
    _ver_ns: dict = {}
    with open(os.path.join(SPECPATH, 'py', 'envoy', '_version.py')) as _f:
        exec(_f.read(), _ver_ns)
    _version: str = _ver_ns['__version__']
    _di_name = f'envoy-{_version}.dist-info'
    _tmp_dir = tempfile.mkdtemp(prefix='envoy_build_')
    _di_path = os.path.join(_tmp_dir, _di_name)
    os.makedirs(_di_path)
    with open(os.path.join(_di_path, 'METADATA'), 'w') as _f:
        _f.write(
            f'Metadata-Version: 2.3\n'
            f'Name: envoy\n'
            f'Version: {_version}\n'
            f'Summary: Environment orchestration for managed application execution.\n'
        )
    with open(os.path.join(_di_path, 'INSTALLER'), 'w') as _f:
        _f.write('build\n')
    atexit.register(shutil.rmtree, _tmp_dir, ignore_errors=True)
    _metadata = [
        (os.path.join(_di_path, 'METADATA'), _di_name),
        (os.path.join(_di_path, 'INSTALLER'), _di_name),
    ]

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

