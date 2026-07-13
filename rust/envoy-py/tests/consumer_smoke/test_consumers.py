"""Consumer smoke tests for ``envoy-py``, exercising the real API call
patterns used by ``gt/globals``, ``gt/devtools``, ``gt/krita``, and
``gt/unreal`` against the compiled wheel. See ``README.md`` in this
directory for what is/isn't feasible to test standalone in this repo, and
for the pre-existing (non-regression) issues discovered along the way.
"""

import json
import sys
import threading
from pathlib import Path

import pytest

import envoy


@pytest.fixture(autouse=True)
def _clear_envoy_bndl_roots(monkeypatch):
    # See module docstring / README: a real ENVOY_BNDL_ROOTS on the dev
    # machine would make bundle auto-discovery shadow these tests' fake
    # commands.json/bundle fixtures.
    monkeypatch.delenv("ENVOY_BNDL_ROOTS", raising=False)


def test_globals_vscode_wrapper_write_local_bundles(tmp_path, monkeypatch):
    """``gt/globals/py/gt/vscode/wrapper/_wrapper.py``'s real code path:
    ``envoy.discoverBundlesAuto()`` (via ``write_local_bundles()``) followed
    by ``envoy.proc.spawn()``.

    Skipped when ``gt/globals`` isn't checked out as a sibling directory
    (e.g. in the standalone ``envoy`` repo's own CI, which only checks out
    ``envoy`` itself, not the full ``gtvfx-contrib/gt`` monorepo layout).
    """
    globals_py = Path(__file__).parents[5] / "globals" / "py"
    if not (globals_py / "gt" / "vscode" / "wrapper" / "_wrapper.py").is_file():
        pytest.skip("gt/globals is not checked out as a sibling directory")

    sys.path.insert(0, str(globals_py))
    try:
        from gt.vscode.wrapper import _wrapper
    finally:
        sys.path.remove(str(globals_py))

    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))
    monkeypatch.setenv("ENVOY_BNDL_ROOTS", str(Path(__file__).parents[5]))

    bundles_path = _wrapper.write_local_bundles(force=True)

    assert bundles_path.exists()
    data = json.loads(bundles_path.read_text())
    assert len(data["bundles"]) > 0

    # launch()'s actual spawn call, minus the real VS Code executable: a raw
    # absolute path is passed directly (no envoy flags), exactly like
    # `envoy.proc.spawn([code_exe] + list(extra_args))`.
    proc = envoy.proc.spawn(
        [sys.executable, "-c", "pass"],
        stdout=envoy.proc.PIPE,
    )
    proc.communicate()
    assert proc.returncode == 0


def test_devtools_cleanup_branches_spawn_kwargs_are_a_pre_existing_bug():
    """``gt/devtools/py/cleanup_branches.py`` calls
    ``envoy.proc.spawn(cmd, pipeline='build', inheritenv=False, ...)``.
    Neither ``pipeline`` nor ``inheritenv`` are real ``subprocess.Popen``
    kwargs (the free ``spawn()`` function forwards ``**kwargs`` straight to
    ``subprocess.Popen`` in both ``py/envoy`` and this wheel), so this call
    raises ``TypeError`` in *both* implementations -- a pre-existing bug in
    the consumer script, confirmed unrelated to and unaffected by this
    migration. Full end-to-end execution of ``cleanup_branches.py`` isn't
    otherwise possible in this checkout: it imports ``gt.gitutils`` and
    ``gt.repl``, neither of which exist in this repository.
    """
    with pytest.raises(TypeError):
        envoy.proc.spawn(
            ["engit", "cleanup"],
            pipeline="build",
            inheritenv=False,
            stdout=envoy.proc.PIPE,
            stderr=envoy.proc.STDOUT,
        )


def test_krita_wrapper_spawn_stream_pattern(tmp_path):
    """``gt/krita/wrapper/py/gt/krita/wrapper/__main__.py``'s real pattern:
    ``envoy.proc.spawn(cmd, env_override=..., stdout=PIPE, stderr=PIPE,
    creationflags=0)``, then streaming ``.stdout``/``.stderr`` line-by-line
    and checking ``.wait()``/``.returncode``.

    Full standalone import of ``gt.krita.wrapper`` isn't feasible in this
    checkout: ``_initialize.py`` imports ``gt.pycore``, which doesn't exist
    here (unrelated to this migration) -- so this test reproduces the exact
    ``envoy.proc`` call pattern directly instead of importing the package.
    """
    envoy_dir = tmp_path / ".envoy"
    envoy_dir.mkdir()
    (envoy_dir / "commands.json").write_text(
        json.dumps({"krita": {"environment": ["krita_env.json"], "alias": [sys.executable]}}),
        encoding="utf-8",
    )
    (envoy_dir / "krita_env.json").write_text("{}", encoding="utf-8")

    proc = envoy.proc.spawn(
        [
            "-cf",
            str(envoy_dir / "commands.json"),
            "krita",
            "-c",
            "import sys; print('out-line'); print('err-line', file=sys.stderr)",
        ],
        stdout=envoy.proc.PIPE,
        stderr=envoy.proc.PIPE,
        creationflags=0,
    )

    captured = {"out": [], "err": []}

    def _stream(stream, bucket):
        for line in iter(stream.readline, b""):
            bucket.append(line.decode(errors="replace"))
        stream.close()

    threads = [
        threading.Thread(target=_stream, args=(proc.stdout, captured["out"])),
        threading.Thread(target=_stream, args=(proc.stderr, captured["err"])),
    ]
    for thread in threads:
        thread.start()
    proc.wait()
    for thread in threads:
        thread.join()

    assert proc.returncode == 0
    assert any("out-line" in line for line in captured["out"])
    assert any("err-line" in line for line in captured["err"])


def test_unreal_wrapper_environment_build_then_spawn_pattern(tmp_path):
    """``gt/unreal/wrapper/py/gt/unreal/wrapper/__main__.py``'s real pattern:
    ``envoy.proc.Environment(cmd, env_override=...)`` then ``.build()``
    (inspected before launch) followed by ``.spawn(args, stdout=PIPE,
    stderr=PIPE, creationflags=0)`` reusing the already-built environment,
    then ``.wait()``/``.returncode``.

    Full standalone import of ``gt.unreal.wrapper`` isn't feasible in this
    checkout: ``_initialize.py`` imports ``gt.winreg``/``gt.win32``, neither
    of which exist here (unrelated to this migration) -- so this test
    reproduces the exact ``envoy.proc`` call pattern directly instead.
    """
    envoy_dir = tmp_path / ".envoy"
    envoy_dir.mkdir()
    (envoy_dir / "commands.json").write_text(
        json.dumps(
            {"UnrealEditor": {"environment": ["ue_env.json"], "alias": [sys.executable]}}
        ),
        encoding="utf-8",
    )
    (envoy_dir / "ue_env.json").write_text(
        json.dumps({"UE_PYTHONPATH": "fake_ue_pythonpath"}), encoding="utf-8"
    )

    ue_env = envoy.proc.Environment(
        sys.executable,
        env_override="UnrealEditor",
        commands_file=envoy_dir / "commands.json",
    )
    built_env = ue_env.build()
    assert built_env.get("UE_PYTHONPATH") == "fake_ue_pythonpath"

    proc = ue_env.spawn(
        ["-c", "import sys; sys.exit(7)"],
        stdout=envoy.proc.PIPE,
        stderr=envoy.proc.PIPE,
        creationflags=0,
    )
    proc.wait()
    assert proc.returncode == 7
