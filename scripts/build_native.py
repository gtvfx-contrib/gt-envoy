"""Build Envoy's native CLIs and optional Python extension wheel."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
RUST_ROOT = REPO_ROOT / "rust"
ENVOY_PY_MANIFEST = RUST_ROOT / "envoy-py" / "Cargo.toml"


def runCommand(arguments: list[str], working_directory: Path = REPO_ROOT) -> None:
    """Run one build command and stop if it fails.

    Args:
        arguments: Command and arguments to execute.
        working_directory: Directory in which to execute the command.

    Raises:
        subprocess.CalledProcessError: If the command exits unsuccessfully.

    """
    print(f"+ {' '.join(arguments)}", flush=True)
    subprocess.run(arguments, cwd=working_directory, check=True)


def parseArguments() -> argparse.Namespace:
    """Parse command-line arguments for the build driver.

    Returns:
        Parsed command-line options.

    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Build the native binaries with Cargo's debug profile.",
    )
    parser.add_argument(
        "--skip-wheel",
        action="store_true",
        help="Build only the native envoy and engit binaries.",
    )
    parser.add_argument(
        "--develop",
        action="store_true",
        help="Install the Python extension into the active environment after building it.",
    )
    parser.add_argument(
        "--target",
        help="Optional Rust target triple for native CLI cross-compilation.",
    )
    return parser.parse_args()


def main() -> int:
    """Build the requested Envoy artifacts.

    Returns:
        Process exit status.

    """
    options = parseArguments()
    cargo_arguments = [
        "cargo",
        "build",
        "--workspace",
        "--exclude",
        "envoy-py",
    ]
    if not options.debug:
        cargo_arguments.append("--release")
    if options.target:
        cargo_arguments.extend(["--target", options.target])

    try:
        runCommand(cargo_arguments, RUST_ROOT)
        if options.skip_wheel:
            return 0

        maturin_arguments = [
            sys.executable,
            "-m",
            "maturin",
            "build",
            "--manifest-path",
            str(ENVOY_PY_MANIFEST),
        ]
        if not options.debug:
            maturin_arguments.append("--release")
        runCommand(maturin_arguments)

        if options.develop:
            develop_arguments = [
                sys.executable,
                "-m",
                "maturin",
                "develop",
                "--manifest-path",
                str(ENVOY_PY_MANIFEST),
            ]
            if not options.debug:
                develop_arguments.append("--release")
            runCommand(develop_arguments)
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"Build failed: {error}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
