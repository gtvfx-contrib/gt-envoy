# envoy CLI Reference

The `envoy` command (and its short alias `en`) launches managed subprocesses with configured environments.

## Synopsis

```
envoy [OPTIONS] [command] [args ...]
en    [OPTIONS] [command] [args ...]
```

!!! note "Flag position matters"
    Envoy flags must appear **before** the command name. Anything after the command name is passed verbatim to the child process.

    ```powershell
    en -i python_dev -c "print(1)"   # -i goes to envoy, -c goes to python
    en -c file.json python_dev        # -c file.json goes to envoy
    ```

## Options

| Flag | Short | Description |
|---|---|---|
| `--list` | | List all available commands |
| `--info COMMAND` | | Show detailed information about a command |
| `--which COMMAND` | | Resolve the executable path for a command |
| `--commands-file PATH` | `-c` | Path to a specific `commands.json` |
| `--bundles-config PATH` | `-b` | Path to a bundles config file |
| `--set-config KEY=VALUE` | `-sc` | Set a user config value and save |
| `--get-config [KEY]` | `-gc` | Print one or all user config values |
| `--list-configs` | `-lc` | List all known configurable settings |
| `--ignore-config` | `-ic` | Bypass user config for this run |
| `--env ENV_COMMAND` | `-e` | Run command inside a different command's environment |
| `--inherit-env` | `-i` | Inherit the full system environment (overrides closed mode) |
| `--verbose` | `-v` | Enable verbose logging |
| `--trace VAR` | | Trace how `VAR` is mutated through env file processing |
| `--docs` | | Open the envoy documentation in the default browser |
| `--version` | | Show version and exit |
| `--help` | `-h` | Show help message |

## Commands

### `--list`

List all available commands with their source bundle and alias:

```powershell
en --list
```

```
Available commands:

  python_dev           → python -X dev  [gt:pythoncore]
  unreal               (executable on PATH)  [gt:unreal]
  vscode               → C:/.../Code.exe --wait  [gt:globals]
```

### `--info COMMAND`

Show full details for a command:

```powershell
en --info python_dev
```

```
Command: python_dev
Bundle:  gt:pythoncore
Executable: python -X dev
Environment files:
  - python_env.json
Environment directory: R:/repo/.../pythoncore/envoy_env
```

### `--which COMMAND`

Resolve the executable path using the subprocess `PATH` built from the command's env files — the same `PATH` the child process will actually see:

```powershell
en --which python_dev
# C:\Python311\python.exe
```

### `--trace VAR`

Show how a variable is set, modified, and resolved as each env file is loaded:

```powershell
en --trace PYTHONPATH python_dev
```

Useful for debugging unexpected variable values.

### `--verbose`

Emit detailed logging for bundle discovery, command loading, environment processing, and executable resolution:

```powershell
en --verbose --list
en --verbose python_dev script.py
```

### `--env` / `-e`

Run a command inside a different command's environment:

```powershell
en --env python_dev cmd
```

Opens `cmd.exe` with the `python_dev` environment — useful for interactive inspection.

## Discovery Flags

### `--bundles-config` / `-b`

Override bundle discovery with an explicit config file:

```powershell
en -b R:/studio/bundles.json --list
en -b R:/studio/bundles.json python_dev script.py
```

### `--commands-file` / `-c`

Point directly at a `commands.json`, bypassing bundle discovery entirely:

```powershell
en -c R:/my-project/envoy_env/commands.json my_command
```

## Environment Variables

| Variable | Description |
|---|---|
| `ENVOY_BNDL_ROOTS` | Semicolon-separated root directories for bundle auto-discovery |
| `ENVOY_ALLOWLIST` | Semicolon- or comma-separated variable names to pass through in closed mode |
| `ENVOY_USER_CONFIG` | Override path to the user config file (useful for testing) |

## User Config Flags

### `--set-config KEY=VALUE` / `-sc`

Save a preference to the user config file:

```powershell
en --set-config bundles_config=R:/studio/envoy/bundles.json
en --set-config verbosity=verbose
en --set-config bundles_config=     # clear the setting
```

### `--get-config [KEY]` / `-gc`

Print the current value of one setting, or all settings if no key is given:

```powershell
en --get-config                   # show all settings
en --get-config bundles_config    # show one setting
```

### `--list-configs` / `-lc`

Show all configurable settings with descriptions and current values:

```powershell
en --list-configs
```

### `--ignore-config` / `-ic`

Run without reading any user config values:

```powershell
en --ignore-config python_dev script.py
```

See [User Configuration](../user-config.md) for the full reference.
