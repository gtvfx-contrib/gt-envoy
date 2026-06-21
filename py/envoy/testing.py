"""envoy.testing -- Test helpers for code that calls the envoy Python API.

Provides lightweight context managers for patching the bundle discovery
environment inside unit tests without touching the live filesystem or the
real ``ENVOY_BNDL_ROOTS`` variable.

Usage example::

    import envoy.testing as envoy_testing

    def test_my_tool():
        with envoy_testing.patchBundleRoots(['/test/fixtures/my_bundle']):
            result = my_code_that_calls_envoy()
            assert result == expected

    # Patch a bare commands.json instead of full bundles
    def test_with_commands_file(tmp_path):
        commands = {'python': {'environment': ['python_env.json']}}
        cf = tmp_path / 'envoyEnv' / 'commands.json'
        cf.parent.mkdir()
        cf.write_text(json.dumps(commands))
        with envoy_testing.patchCommandsFile(cf):
            ...
"""

from __future__ import annotations

import os
from contextlib import contextmanager
from pathlib import Path


@contextmanager
def patchBundleRoots(roots: list[str | Path]):
    """Context manager that temporarily overrides ``ENVOY_BNDL_ROOTS``.

    All bundle discovery performed inside the block (by :mod:`envoy.proc`
    or direct calls to :func:`~envoy._discovery.discoverBundlesAuto`) will
    search *roots* instead of whatever ``ENVOY_BNDL_ROOTS`` is set to in the
    real environment.

    Args:
        roots: Sequence of directory paths to use as bundle search roots.
            Each path should contain at least one subdirectory with an
            ``envoyEnv/`` folder.

    Example::

        with envoy_testing.patchBundleRoots(['/test/bundles']):
            rc = proc.call(['my_tool', '--help'])
    """
    separator = ';' if os.name == 'nt' else ':'
    new_value = separator.join(str(r) for r in roots)

    old_value = os.environ.get('ENVOY_BNDL_ROOTS')
    os.environ['ENVOY_BNDL_ROOTS'] = new_value
    try:
        yield
    finally:
        if old_value is None:
            os.environ.pop('ENVOY_BNDL_ROOTS', None)
        else:
            os.environ['ENVOY_BNDL_ROOTS'] = old_value


@contextmanager
def patchCommandsFile(commands_file: str | Path):
    """Context manager that temporarily points envoy at a specific
    ``commands.json`` file.

    Useful when your test fixture provides a bare ``commands.json`` rather
    than a full bundle directory tree.  Under the hood this clears
    ``ENVOY_BNDL_ROOTS`` and sets ``ENVOY_COMMANDS_FILE`` so that
    :func:`~envoy._commands.findCommandsFile` resolves to the given path.

    .. note::
        This helper is complementary to :func:`patchBundleRoots`.  Use
        :func:`patchBundleRoots` when your fixture looks like a real bundle
        (``my_bundle/envoyEnv/commands.json``), and use this helper when you
        only have a standalone JSON file.

    Args:
        commands_file: Path to the ``commands.json`` to use.

    Example::

        with envoy_testing.patchCommandsFile(tmp_path / 'commands.json'):
            rc = proc.call(['my_cmd'])
    """
    cf_path = Path(commands_file).resolve()
    if not cf_path.is_file():
        raise FileNotFoundError(
            f"patchCommandsFile expected an existing commands.json file, but "
            f"got {cf_path!s}"
        )

    old_roots = os.environ.get('ENVOY_BNDL_ROOTS')
    old_cf = os.environ.get('ENVOY_COMMANDS_FILE')

    # Clear bundle roots so auto-discovery does not interfere, and point at
    # the fixture commands file via the dedicated env var.
    os.environ.pop('ENVOY_BNDL_ROOTS', None)
    os.environ['ENVOY_COMMANDS_FILE'] = str(cf_path)
    try:
        yield
    finally:
        # Restore original state
        if old_roots is None:
            os.environ.pop('ENVOY_BNDL_ROOTS', None)
        else:
            os.environ['ENVOY_BNDL_ROOTS'] = old_roots

        if old_cf is None:
            os.environ.pop('ENVOY_COMMANDS_FILE', None)
        else:
            os.environ['ENVOY_COMMANDS_FILE'] = old_cf
