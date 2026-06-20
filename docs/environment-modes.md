# Environment Modes

Envoy controls what the subprocess environment contains through three modes.

## Overview

```mermaid
flowchart TD
    SYS[System Environment]
    AL[ENVOY_ALLOWLIST]
    CORE[Core OS Variables]
    ENV[Bundle Env Files]
    SUB[("Subprocess\nEnvironment")]

    subgraph Closed Mode default
        CORE --> SUB
        AL -->|named vars| SUB
        ENV --> SUB
    end

    subgraph Inherit-Env Mode
        SYS -->|full passthrough| SUB2[("Subprocess\nEnvironment")]
        ENV2[Bundle Env Files] -->|layered on top| SUB2
    end
```

## Closed Mode (Default)

By default Envoy runs in **closed mode**. The subprocess receives only:

1. **Core OS variables** — always present regardless of mode:

    | Variable | Purpose |
    |---|---|
    | `USERPROFILE`, `HOME` | User home directory |
    | `APPDATA`, `LOCALAPPDATA`, `ROAMING` | User app data |
    | `TEMP`, `TMP` | Temporary files |
    | `SystemRoot`, `SystemDrive` | Windows system root |
    | `COMPUTERNAME`, `USERNAME` | Machine/user identity |
    | `LANG`, `LC_ALL` | Locale |
    | `PATHEXT` | Executable extensions (Windows) |

2. **User allowlist** — additional variables named in `ENVOY_ALLOWLIST`
3. **Bundle env files** — everything defined in `global_env.json` and the command's env files

This prevents accidental dependency on developer machine state and makes environments fully reproducible across machines.

## Inherit-Env Mode

Pass `--inherit-env` (or `-i`) to inherit the full system environment, with bundle env files layered on top:

```powershell
en --inherit-env python_dev script.py
en -i python_dev script.py
```

Use this when a command needs tools from your system `PATH` that are not yet defined in a bundle env file, or when debugging environment issues.

!!! warning
    Inherit-env mode can mask missing bundle env file entries. A command that works with `--inherit-env` but fails without it is likely missing a `+=PATH` entry in its env file.

## Allowlist

`ENVOY_ALLOWLIST` lets specific system variables pass through in closed mode without enabling full passthrough:

```powershell
$env:ENVOY_ALLOWLIST = "MY_STUDIO_VAR;PERFORCE_PORT;LICENSE_SERVER"
```

Supports both `;` and `,` as separators. These are merged on top of the built-in core OS variables.

### Example: studio-wide variable

Set `ENVOY_ALLOWLIST=STUDIO_ROOT` in your system environment, then reference it in env files:

```json
{
    "+=PYTHONPATH": "${STUDIO_ROOT}/shared/py"
}
```

`STUDIO_ROOT` is carried through from the system environment into the subprocess, then expanded in the env file.

## Comparison

| | Closed (default) | Allowlist | Inherit-Env |
|---|---|---|---|
| Core OS vars | ✓ | ✓ | ✓ |
| Bundle env files | ✓ | ✓ | ✓ |
| Named system vars | ✗ | ✓ (listed) | ✓ (all) |
| Full system env | ✗ | ✗ | ✓ |
| Reproducible | ✓ | mostly ✓ | ✗ |
