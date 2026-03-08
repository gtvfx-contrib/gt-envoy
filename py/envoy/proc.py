"""envoy.proc -- Process execution with pre-built command environments.

This module is the primary Python API for launching managed subprocesses
through envoy's environment system.

Usage examples::

    import envoy.proc as proc

    # One-shot blocking call
    rc = proc.call(['maya', 'myfile.ma'])

    # Raise on failure
    proc.check_call(['nuke', '-x', 'comp.nk'])

    # Capture output
    output = proc.check_output(['python', '-c', 'print("hello")'])

    # Fire-and-forget (non-blocking)
    p = proc.spawn(['houdini', 'shot.hip'])
    # ... do other work ...
    p.wait()

    # Bake the environment once, launch multiple processes cheaply
    env = proc.Environment('maya')
    env.build()                             # optional explicit pre-build
    env.spawn(['scene_a.ma'])
    env.spawn(['scene_b.ma'])

Constants:
    PIPE    -- use as stdout/stderr kwarg to capture a stream (mirrors subprocess.PIPE)
    STDOUT  -- use as stderr kwarg to merge stderr into stdout (mirrors subprocess.STDOUT)
    DEVNULL -- use as stdout/stderr kwarg to discard output (mirrors subprocess.DEVNULL)

"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

from ._commands import CommandDefinition, CommandRegistry, find_commands_file
from ._discovery import BundleInfo, discover_bundles_from_roots, discover_bundles_auto
from ._environment import EnvironmentManager
from ._exceptions import (
    CalledProcessError,
    CommandNotFoundError,
    EnvironmentBuildError,
    WrapperError,
)
from ._executor import ProcessExecutor


# ---------------------------------------------------------------------------
# Re-exported subprocess constants
# ---------------------------------------------------------------------------

#: Pass as the ``stdout`` or ``stderr`` kwarg to :func:`spawn` /
#: :meth:`Environment.spawn` to capture a stream (mirrors
#: :data:`subprocess.PIPE`).
PIPE = subprocess.PIPE

#: Pass as the ``stderr`` kwarg to :func:`spawn` /
#: :meth:`Environment.spawn` to merge stderr into stdout (mirrors
#: :data:`subprocess.STDOUT`).
STDOUT = subprocess.STDOUT

#: Pass as the ``stdout`` or ``stderr`` kwarg to suppress output entirely
#: (mirrors :data:`subprocess.DEVNULL`).
DEVNULL = subprocess.DEVNULL


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _is_raw_path(spec: str) -> bool:
    """Return ``True`` when *spec* should be treated as a direct executable
    path rather than an envoy command name.

    A spec is considered a raw path when it is absolute (``C:\\...``,
    ``/usr/bin/...``) or contains an explicit directory separator
    (``./code``, ``../bin/code``).  Plain names like ``'maya'``, ``'python'``
    are envoy command names and return ``False``.

    """
    p = Path(spec)
    return p.is_absolute() or (os.sep in spec) or ('/' in spec)


def _raw_popen(
    executable: str,
    extra_args: list[str],
    env: dict[str, str],
    **kwargs,
) -> subprocess.Popen:
    """Spawn *executable* directly, bypassing envoy command resolution.

    Unlike :func:`_popen` this does not look up a registry entry — the
    caller is responsible for passing the full path.  The same
    ``CREATE_NO_WINDOW`` / ``cmd /c`` treatment is applied on Windows.

    """
    full_cmd: list[str] = [executable] + list(extra_args)

    if os.name == 'nt':
        if 'creationflags' not in kwargs:
            kwargs['creationflags'] = subprocess.CREATE_NO_WINDOW
        if Path(executable).suffix.lower() in ('.bat', '.cmd'):
            full_cmd = ['cmd', '/c'] + full_cmd

    return subprocess.Popen(full_cmd, env=env, **kwargs)


def _load_registry(
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
) -> tuple[CommandRegistry, list[BundleInfo] | None]:
    """Discover bundles and return a populated :class:`~._commands.CommandRegistry`.

    Discovery priority:

    1. ``bundle_roots`` — explicit list of root directories, bypassing
       ``ENVOY_BNDL_ROOTS`` entirely.
    2. ``ENVOY_BNDL_ROOTS`` environment variable (standard auto-discovery).
    3. ``commands_file`` — explicit path to a bare ``commands.json``.
    4. Upward search from CWD for ``envoy_env/commands.json``.

    Returns:
        ``(registry, bundles_or_None)``.  *bundles* is ``None`` when the
        registry was seeded from a bare ``commands.json`` rather than bundles.

    """
    registry = CommandRegistry()
    bundles: list[BundleInfo] | None = None

    if bundle_roots is not None:
        # Caller supplied explicit roots — use them directly.
        discovered = discover_bundles_from_roots([str(r) for r in bundle_roots])
        if discovered:
            registry.load_from_bundles(discovered)
            bundles = discovered
    else:
        # Fall back to ENVOY_BNDL_ROOTS auto-discovery.
        discovered = discover_bundles_auto()
        if discovered:
            registry.load_from_bundles(discovered)
            bundles = discovered

    if not registry:
        # Neither bundle path worked — try a plain commands.json.
        cf = commands_file or find_commands_file()
        if cf:
            registry.load_from_file(cf)

    return registry, bundles


def _collect_env_files(
    command_name: str,
    registry: CommandRegistry,
    bundles: list[BundleInfo] | None,
) -> list[str | Path]:
    """Build the ordered list of env file paths for *command_name*.

    Mirrors the collection logic inside :func:`~._cli.run_command`.

    Raises:
        CommandNotFoundError: Command not registered.
        EnvironmentBuildError: Env resolution failed or a required file is absent.

    """
    cmd = registry.get(command_name)
    if cmd is None:
        raise CommandNotFoundError(
            f"Command '{command_name}' is not registered. "
            "Run 'envoy --list' to see available commands."
        )

    try:
        resolved_env = registry.resolve_environment(command_name)
    except WrapperError as e:
        raise EnvironmentBuildError(
            f"Failed to resolve environment for '{command_name}': {e}"
        ) from e

    env_files: list[str | Path] = []

    if bundles:
        # Multi-bundle mode: use the pre-indexed env_files mapping.
        for bundle in bundles:
            if 'global_env.json' in bundle.env_files:
                env_files.append(bundle.env_files['global_env.json'])
        for env_file_name, _env_dir in resolved_env:
            for bundle in bundles:
                if env_file_name in bundle.env_files:
                    env_files.append(bundle.env_files[env_file_name])
    else:
        # Legacy (single commands.json) mode: build paths from envoy_env dir.
        env_dir = cmd.envoy_env_dir
        if env_dir is None:
            cf = find_commands_file()
            if cf is None:
                raise EnvironmentBuildError(
                    f"Cannot determine envoy_env directory for '{command_name}'."
                )
            env_dir = cf.parent

        global_env = env_dir / 'global_env.json'
        if global_env.exists():
            env_files.append(global_env)

        for env_file_name, entry_env_dir in resolved_env:
            dir_to_use = entry_env_dir or env_dir
            file_path = dir_to_use / env_file_name
            if not file_path.exists():
                raise EnvironmentBuildError(
                    f"Environment file not found: {file_path}"
                )
            env_files.append(file_path)

    return env_files


def _prepare_env(
    command_name: str,
    registry: CommandRegistry,
    bundles: list[BundleInfo] | None,
    inherit_env: bool,
    allowlist: list[str] | None,
    env_override: str | None = None,
) -> tuple[dict[str, str], CommandDefinition]:
    """Prepare the subprocess environment dict for a registered command.

    When *env_override* is given the environment files are sourced from that
    command instead of *command_name*, but *command_name* still determines
    the executable and alias.  This lets you run an arbitrary executable
    inside a different command's environment::

        _prepare_env('/path/to/krita', registry, bundles,
                     inherit_env=False, allowlist=None,
                     env_override='krita')

    Returns:
        ``(env_dict, command_definition)`` — the command definition provides
        the resolved executable and alias base-args.

    Raises:
        CommandNotFoundError: Command not registered.
        EnvironmentBuildError: Environment preparation failed.

    """
    # Decide which command name drives env-file collection.
    env_source = env_override if env_override is not None else command_name

    # Validate the environment-source command exists in the registry.
    if env_override is not None and registry.get(env_override) is None:
        raise CommandNotFoundError(
            f"Environment override command '{env_override}' is not registered."
        )

    # Build the CommandDefinition used for execution.
    if _is_raw_path(command_name):
        # Raw path: bypass registry lookup — wrap the path in a minimal definition.
        cmd = CommandDefinition(
            name=command_name,
            environment=[],
            alias=[command_name],
        )
    else:
        cmd = registry.get(command_name)
        if cmd is None:
            raise CommandNotFoundError(
                f"Command '{command_name}' is not registered."
            )

    env_files = _collect_env_files(env_source, registry, bundles)
    allowlist_set: set[str] | None = set(allowlist) if allowlist else None
    env_mgr = EnvironmentManager(inherit_env=inherit_env, allowlist=allowlist_set)

    try:
        env = env_mgr.prepare_environment(env_files=env_files)
    except WrapperError as e:
        raise EnvironmentBuildError(
            f"Failed to prepare environment for '{env_source}': {e}"
        ) from e

    return env, cmd


def _popen(
    cmd_def: CommandDefinition,
    extra_args: list[str],
    env: dict[str, str],
    **kwargs,
) -> subprocess.Popen:
    """Resolve the executable and spawn a :class:`~subprocess.Popen`.

    Executable is resolved against the subprocess PATH from *env*.  On
    Windows, ``CREATE_NO_WINDOW`` is applied automatically unless the caller
    passes an explicit ``creationflags``.

    """
    try:
        resolved = ProcessExecutor.resolve_executable(
            cmd_def.executable, search_path=env.get('PATH')
        )
    except WrapperError as e:
        raise EnvironmentBuildError(str(e)) from e

    full_cmd = [resolved] + cmd_def.base_args + list(extra_args)

    if os.name == 'nt':
        if 'creationflags' not in kwargs:
            kwargs['creationflags'] = subprocess.CREATE_NO_WINDOW
        # Batch files cannot be executed directly by CreateProcess; they must
        # be run via cmd.exe.  This also prevents %~dp0 expansion failures on
        # UNC paths that use forward slashes.
        if Path(resolved).suffix.lower() in ('.bat', '.cmd'):
            full_cmd = ['cmd', '/c'] + full_cmd

    return subprocess.Popen(full_cmd, env=env, **kwargs)


# ---------------------------------------------------------------------------
# Environment class
# ---------------------------------------------------------------------------

class Environment:
    """A pre-built envoy command environment for launching multiple processes.

    Bundle discovery and environment file parsing happen **at most once** — on
    the first execution call (or when :meth:`build` is called explicitly).
    Subsequent calls reuse the cached environment dict, making it cheap to
    spawn batches of processes for the same command.

    Args:
        command: The envoy command name (e.g. ``'maya'``, ``'nuke'``).
        inherit_env: When ``False`` (the default) the subprocess receives a
            closed environment containing only env-file variables plus the
            built-in OS seed vars.  Pass ``True`` to let the subprocess
            inherit the full current process environment.
        allowlist: Additional system variable names to carry through in closed
            mode (supplementary to the built-in seed vars).
        bundle_roots: Override the bundle discovery search roots for this
            instance only.  When given these paths replace ``ENVOY_BNDL_ROOTS``.
        commands_file: Explicit path to a ``commands.json`` file used when
            bundle discovery is unavailable.

    Examples::

        # Pre-build, then launch a batch — no discovery overhead per spawn
        env = Environment('houdini')
        env.build()
        for hip in scene_files:
            env.spawn([str(hip)])

        # Capture output from a tool
        output = Environment('python').check_output(['-c', 'import sys; print(sys.version)'])

    """

    def __init__(
        self,
        command: str,
        *,
        inherit_env: bool = False,
        allowlist: list[str] | None = None,
        whitelist: list[str] | None = None,
        bundle_roots: list[str] | None = None,
        commands_file: Path | None = None,
        env_override: str | None = None,
    ) -> None:
        # ``whitelist`` is a deprecated alias for ``allowlist``; both are
        # merged so legacy callers continue to work.
        combined = list(allowlist or []) + list(whitelist or [])
        self._command = command
        self._inherit_env = inherit_env
        self._allowlist: list[str] | None = combined if combined else None
        self._bundle_roots = bundle_roots
        self._commands_file = commands_file
        self._env_override = env_override
        self._env: dict[str, str] | None = None
        self._cmd_def: CommandDefinition | None = None

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def command(self) -> str:
        """The envoy command name this environment was created for."""
        return self._command

    @property
    def allowlist(self) -> list[str]:
        """The allowlist of system variable names passed at construction."""
        return list(self._allowlist) if self._allowlist else []

    @property
    def whitelist(self) -> list[str]:
        """Deprecated alias for :attr:`allowlist`."""
        return self.allowlist

    # ------------------------------------------------------------------
    # Dunder methods
    # ------------------------------------------------------------------

    def __str__(self) -> str:
        return f"<Environment {self._command}>"

    def __repr__(self) -> str:
        return f"<Environment {self._command}>"

    def build(self) -> dict[str, str]:
        """Compute the command environment ahead of execution.

        Idempotent — after the first call the cached result is returned.

        When :attr:`command` is an absolute path or contains a path separator
        the registry and env-file machinery is bypassed entirely.  The
        environment is built from the seed vars (core OS variables + any
        *allowlist* entries) in closed mode, or inherited in full when
        ``inherit_env=True``.

        Returns:
            The subprocess environment dictionary.

        Raises:
            CommandNotFoundError: The command is not in any discovered registry.
            EnvironmentBuildError: Environment files are missing or invalid.

        """
        if self._env is None:
            if _is_raw_path(self._command) and self._env_override is None:
                # Direct executable — no registry lookup or env-file loading.
                allowlist_set: set[str] | None = (
                    set(self._allowlist) if self._allowlist else None
                )
                env_mgr = EnvironmentManager(
                    inherit_env=self._inherit_env,
                    allowlist=allowlist_set,
                )
                self._env = env_mgr.prepare_environment(env_files=[])
                self._cmd_def = CommandDefinition(
                    name=self._command,
                    environment=[],
                    alias=[self._command],
                )
            else:
                registry, bundles = _load_registry(
                    bundle_roots=self._bundle_roots,
                    commands_file=self._commands_file,
                )
                self._env, self._cmd_def = _prepare_env(
                    self._command, registry, bundles,
                    self._inherit_env, self._allowlist,
                    env_override=self._env_override,
                )
        return self._env

    def spawn(self, args: list[str] | None = None, **kwargs) -> subprocess.Popen:
        """Launch the command and return the running process immediately.

        The call is **asynchronous** — it returns as soon as the process has
        started without waiting for it to exit.  Use :meth:`call` to block.

        Args:
            args: Extra arguments appended to the command's executable and
                base alias args.
            **kwargs: Forwarded to :class:`subprocess.Popen`.

        Returns:
            The running :class:`subprocess.Popen` instance.

        """
        self.build()
        assert self._cmd_def is not None and self._env is not None
        if _is_raw_path(self._command) and self._env_override is None:
            return _raw_popen(self._command, list(args or []), self._env, **kwargs)
        return _popen(self._cmd_def, list(args or []), self._env, **kwargs)

    def call(self, args: list[str] | None = None, **kwargs) -> int:
        """Run the command synchronously and return its exit code.

        Args:
            args: Extra arguments passed to the command.
            **kwargs: Forwarded to :class:`subprocess.Popen`.

        Returns:
            The process exit code.

        Raises:
            ValueError: If ``stdout`` or ``stderr`` is :data:`PIPE`.
                Use :meth:`check_output` or :meth:`spawn` when you need to
                capture a stream.

        """
        if kwargs.get('stdout') is PIPE or kwargs.get('stderr') is PIPE:
            raise ValueError(
                "'call' does not support PIPE redirection for stdout/stderr. "
                "Use 'spawn' for async capture or 'check_output' for "
                "synchronous capture."
            )
        proc = self.spawn(args, **kwargs)
        proc.wait()
        return proc.returncode

    def check_call(self, args: list[str] | None = None, **kwargs) -> int:
        """Run the command and raise :exc:`CalledProcessError` on failure.

        Args:
            args: Extra arguments passed to the command.
            **kwargs: Forwarded to :class:`subprocess.Popen`.

        Returns:
            The exit code (always ``0``).

        Raises:
            CalledProcessError: If the process exits with a non-zero status.

        """
        rc = self.call(args, **kwargs)
        if rc != 0:
            exe = self._cmd_def.executable if self._cmd_def else self._command
            raise CalledProcessError(rc, exe)
        return rc

    def check_output(
        self,
        args: list[str] | None = None,
        **kwargs,
    ) -> bytes:
        """Run the command and capture its stdout as bytes.

        ``stdout`` is automatically set to :data:`PIPE`.  Pass
        ``stderr=STDOUT`` to merge stderr into the returned bytes.

        Args:
            args: Extra arguments passed to the command.
            **kwargs: Forwarded to :class:`subprocess.Popen`.  ``input``
                may be provided as a keyword argument to send bytes/str to
                the process via stdin.  ``stdout`` may not be set (it is
                owned by this method).

        Returns:
            Captured stdout bytes.

        Raises:
            CalledProcessError: If the process exits with a non-zero status.
            ValueError: If ``stdout`` is given in *kwargs* (it will be
                overridden), or if both ``input`` and ``stdin`` are provided.

        """
        input_ = kwargs.pop('input', None)  # noqa: A001
        if 'stdout' in kwargs:
            raise ValueError(
                "'stdout' argument not allowed in check_output; "
                "it will be overridden."
            )
        if input_ is not None and 'stdin' in kwargs:
            raise ValueError("'input' and 'stdin' cannot both be specified")
        kwargs['stdout'] = PIPE
        if input_ is not None:
            kwargs['stdin'] = PIPE
        proc = self.spawn(args, **kwargs)
        stdout, _ = proc.communicate(input_)
        if proc.returncode != 0:
            exe = self._cmd_def.executable if self._cmd_def else self._command
            raise CalledProcessError(
                proc.returncode,
                exe,
                output=stdout,
            )
        return stdout


# ---------------------------------------------------------------------------
# Free functions
# ---------------------------------------------------------------------------

def call(
    cmd: list[str],
    *,
    inherit_env: bool = False,
    allowlist: list[str] | None = None,
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
    env_override: str | None = None,
    **kwargs,
) -> int:
    """Execute an envoy command and return its exit code.

    Each call performs full bundle discovery and environment preparation.
    When launching the **same command** multiple times, prefer
    :class:`Environment` to amortise the startup cost.

    Args:
        cmd: ``[command_name, arg1, arg2, ...]`` where *command_name* is the
            envoy command key (e.g. ``'maya'``) and the remaining items are
            forwarded to the subprocess.
        inherit_env: Pass ``True`` to inherit the full system environment.
        allowlist: Additional system variable names to include in closed mode.
        bundle_roots: Override bundle discovery roots.
        commands_file: Explicit ``commands.json`` path.
        env_override: Name of a different envoy command whose environment files
            should be used in place of *cmd[0]*'s own environment.  The
            executable from *cmd[0]* (or a raw path) is still what runs.
        **kwargs: Forwarded to :class:`subprocess.Popen`.

    Returns:
        The process exit code.

    Raises:
        ValueError: If ``stdout`` or ``stderr`` is :data:`PIPE`.

    See also:
        :func:`subprocess.call`

    """
    if not cmd:
        raise ValueError("'cmd' must be a non-empty list")
    if kwargs.get('stdout') is PIPE or kwargs.get('stderr') is PIPE:
        raise ValueError(
            "'call' does not support PIPE redirection for stdout/stderr. "
            "Use 'spawn' for async capture or 'check_output' for "
            "synchronous capture."
        )
    return Environment(
        cmd[0],
        inherit_env=inherit_env,
        allowlist=allowlist,
        bundle_roots=bundle_roots,
        commands_file=commands_file,
        env_override=env_override,
    ).call(cmd[1:], **kwargs)


def spawn(
    cmd: list[str],
    *,
    inherit_env: bool = False,
    allowlist: list[str] | None = None,
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
    env_override: str | None = None,
    **kwargs,
) -> subprocess.Popen:
    """Execute an envoy command and return the running process immediately.

    Args:
        cmd: ``[command_name, arg1, arg2, ...]``.
        inherit_env: Pass ``True`` to inherit the full system environment.
        allowlist: Additional system variable names in closed mode.
        bundle_roots: Override bundle discovery roots.
        commands_file: Explicit ``commands.json`` path.
        env_override: Name of a different envoy command whose environment files
            should be used in place of *cmd[0]*'s own environment.  The
            executable from *cmd[0]* (or a raw path) is still what runs.
        **kwargs: Forwarded to :class:`subprocess.Popen`.

    Returns:
        The running :class:`subprocess.Popen` instance.

    See also:
        :class:`subprocess.Popen`

    """
    if not cmd:
        raise ValueError("'cmd' must be a non-empty list")
    return Environment(
        cmd[0],
        inherit_env=inherit_env,
        allowlist=allowlist,
        bundle_roots=bundle_roots,
        commands_file=commands_file,
        env_override=env_override,
    ).spawn(cmd[1:], **kwargs)


def check_call(
    cmd: list[str],
    *,
    inherit_env: bool = False,
    allowlist: list[str] | None = None,
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
    env_override: str | None = None,
    **kwargs,
) -> int:
    """Execute an envoy command and raise on non-zero exit.

    Args:
        cmd: ``[command_name, arg1, arg2, ...]``.
        inherit_env: Pass ``True`` to inherit the full system environment.
        allowlist: Additional system variable names in closed mode.
        bundle_roots: Override bundle discovery roots.
        commands_file: Explicit ``commands.json`` path.
        env_override: Name of a different envoy command whose environment files
            should be used in place of *cmd[0]*'s own environment.
        **kwargs: Forwarded to :class:`subprocess.Popen`.

    Returns:
        The exit code (always ``0``).

    Raises:
        CalledProcessError: If the process exits with a non-zero status.

    See also:
        :func:`subprocess.check_call`

    """
    rc = call(
        cmd,
        inherit_env=inherit_env,
        allowlist=allowlist,
        bundle_roots=bundle_roots,
        commands_file=commands_file,
        env_override=env_override,
        **kwargs,
    )
    if rc != 0:
        raise CalledProcessError(rc, cmd[0])
    return rc


def check_output(
    cmd: list[str],
    *,
    inherit_env: bool = False,
    allowlist: list[str] | None = None,
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
    env_override: str | None = None,
    **kwargs,
) -> bytes:
    """Execute an envoy command and return its stdout as bytes.

    ``stdout`` is automatically captured (passing it in *kwargs* raises
    :class:`ValueError`).  Pass ``stderr=STDOUT`` to include stderr in the
    returned bytes.  Provide ``input=<bytes>`` as a keyword argument to
    send data to stdin.

    Args:
        cmd: ``[command_name, arg1, arg2, ...]``.
        inherit_env: Pass ``True`` to inherit the full system environment.
        allowlist: Additional system variable names in closed mode.
        bundle_roots: Override bundle discovery roots.
        commands_file: Explicit ``commands.json`` path.
        env_override: Name of a different envoy command whose environment files
            should be used in place of *cmd[0]*'s own environment.
        **kwargs: Forwarded to :class:`subprocess.Popen`.  ``input`` and
            ``stderr`` are the most commonly useful keys here.

    Returns:
        Captured stdout bytes.

    Raises:
        CalledProcessError: If the process exits with a non-zero status.
        ValueError: If ``stdout`` is in *kwargs* or both ``input`` and
            ``stdin`` are provided.

    See also:
        :func:`subprocess.check_output`

    """
    if not cmd:
        raise ValueError("'cmd' must be a non-empty list")
    return Environment(
        cmd[0],
        inherit_env=inherit_env,
        allowlist=allowlist,
        bundle_roots=bundle_roots,
        commands_file=commands_file,
        env_override=env_override,
    ).check_output(cmd[1:], **kwargs)
