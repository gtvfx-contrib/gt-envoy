"""Contract tests for the ``ENVOY_PYINIT`` extension point.

``envoy._run_pyinit_scripts`` only runs automatically once, at ``import
envoy`` time. These tests call it directly -- it is still the exact same
function object bound on the already-imported module -- so each test can
exercise a different ``ENVOY_PYINIT`` value without needing a fresh Python
process per case.
"""

import sys

import envoy


def test_unset_pyinit_is_a_noop(monkeypatch):
    monkeypatch.delenv("ENVOY_PYINIT", raising=False)

    envoy._run_pyinit_scripts()  # should not raise


def test_runs_py_files_from_a_directory_in_sorted_order(tmp_path, monkeypatch):
    order_log = tmp_path / "order.log"
    (tmp_path / "01_first.py").write_text(
        f"open({str(order_log)!r}, 'a').write('first\\n')"
    )
    (tmp_path / "02_second.py").write_text(
        f"open({str(order_log)!r}, 'a').write('second\\n')"
    )

    monkeypatch.setenv("ENVOY_PYINIT", str(tmp_path))
    envoy._run_pyinit_scripts()

    assert order_log.read_text().splitlines() == ["first", "second"]


def test_a_failing_script_does_not_block_the_rest(tmp_path, monkeypatch, capsys):
    ran_marker = tmp_path / "ran.log"
    (tmp_path / "01_fails.py").write_text("raise RuntimeError('boom')")
    (tmp_path / "02_runs.py").write_text(f"open({str(ran_marker)!r}, 'w').write('ran')")

    monkeypatch.setenv("ENVOY_PYINIT", str(tmp_path))
    envoy._run_pyinit_scripts()  # should not raise despite 01_fails.py

    assert ran_marker.read_text() == "ran"
    assert "boom" in capsys.readouterr().err


def test_a_script_can_import_and_use_the_full_envoy_api(tmp_path, monkeypatch):
    result_log = tmp_path / "result.log"
    (tmp_path / "uses_api.py").write_text(
        f"import envoy\nopen({str(result_log)!r}, 'w').write(envoy.__version__)"
    )

    monkeypatch.setenv("ENVOY_PYINIT", str(tmp_path))
    envoy._run_pyinit_scripts()

    assert result_log.read_text() == envoy.__version__


def test_multiple_directories_are_platform_separated(tmp_path, monkeypatch):
    first_dir = tmp_path / "first"
    second_dir = tmp_path / "second"
    first_dir.mkdir()
    second_dir.mkdir()
    marker = tmp_path / "marker.log"
    (first_dir / "a.py").write_text(f"open({str(marker)!r}, 'a').write('a')")
    (second_dir / "b.py").write_text(f"open({str(marker)!r}, 'a').write('b')")

    separator = ";" if sys.platform == "win32" else ":"
    monkeypatch.setenv(
        "ENVOY_PYINIT", separator.join([str(first_dir), str(second_dir)])
    )
    envoy._run_pyinit_scripts()

    assert marker.read_text() == "ab"


def test_nonexistent_directory_is_silently_skipped(tmp_path, monkeypatch):
    monkeypatch.setenv("ENVOY_PYINIT", str(tmp_path / "does-not-exist"))

    envoy._run_pyinit_scripts()  # should not raise


def test_non_py_files_are_ignored(tmp_path, monkeypatch):
    marker = tmp_path / "marker.log"
    (tmp_path / "readme.txt").write_text("not a script")
    (tmp_path / "runs.py").write_text(f"open({str(marker)!r}, 'w').write('ran')")

    monkeypatch.setenv("ENVOY_PYINIT", str(tmp_path))
    envoy._run_pyinit_scripts()

    assert marker.read_text() == "ran"
