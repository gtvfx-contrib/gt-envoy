"""Public-API contract test for the ``?=`` (default) operator, run against
the compiled ``envoy-py`` (PyO3) wheel.

Adapted from ``py/envoy/test_bundle/test_default_operator.py``: that file's
other five tests exercised the private ``envoy._environment.EnvironmentManager``
directly (no public equivalent), so only the end-to-end test that goes
through the public ``ApplicationWrapper``/``WrapperConfig`` surface is kept
here. See ``tests/python_contract/README.md``.
"""

import json
import os
import tempfile
from pathlib import Path

from envoy import ApplicationWrapper, WrapperConfig


def _writeEnvFile(tmp_dir: str, data: dict) -> Path:
    """Write a JSON env file to a temp directory and return its path."""
    path = Path(tmp_dir) / "env.json"
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    return path


def test_default_operator_via_wrapper():
    """End-to-end: ?= sets variable via ApplicationWrapper when not in env."""
    # Make sure the variable is absent from the child process.
    os.environ.pop("ENVOY_TEST_E2E_DEFAULT", None)

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"?=ENVOY_TEST_E2E_DEFAULT": "e2e_default"})

        config = WrapperConfig(
            executable="python",
            args=[
                "-c",
                "import os; print(os.environ.get('ENVOY_TEST_E2E_DEFAULT', 'MISSING'))",
            ],
            env_files=str(env_file),
            capture_output=True,
            stream_output=False,
            log_execution=False,
        )

        wrapper = ApplicationWrapper(config)
        result = wrapper.run()

    assert result.success, "Wrapper should execute successfully"
    assert "e2e_default" in result.stdout, (  # type: ignore[operator]
        "Child process should see the default value set by ?="
    )
