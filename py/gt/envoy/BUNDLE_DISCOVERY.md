# Bundle Discovery

## Overview

Envoy discovers commands from one or more **bundles** — Git repositories containing an `envoy_env/` directory. Commands from all discovered bundles are merged into a single registry, with each command tagged with its source bundle.

## Discovery Methods

### 1. Auto-Discovery (`ENVOY_BNDL_ROOTS`)

Set `ENVOY_BNDL_ROOTS` to a list of root directories. Envoy scans each root for subdirectories that are Git repositories with an `envoy_env/` directory.

```powershell
# PowerShell
$env:ENVOY_BNDL_ROOTS = "R:\repo\gtvfx-contrib;R:\repo\gtvfx"

# cmd
set ENVOY_BNDL_ROOTS=R:\repo\gtvfx-contrib;R:\repo\gtvfx

# Unix/macOS
export ENVOY_BNDL_ROOTS=/home/user/repos:/opt/studio
```

Separator: `;` on Windows, `:` on Unix/macOS.

#### How it Works

1. Each root is scanned one level deep for subdirectories
2. A subdirectory qualifies if it has both a `.git/` folder and an `envoy_env/` directory
3. `envoy_env/commands.json` is loaded from each qualifying bundle
4. All JSON files in each `envoy_env/` are indexed at discovery time for fast lookup at run time

#### Example Structure

```
R:\repo\
├── project-a\
│   ├── .git\
│   └── envoy_env\
│       ├── commands.json
│       ├── global_env.json
│       └── project_env.json
├── project-b\
│   ├── .git\
│   └── envoy_env\
│       ├── commands.json
│       └── project_env.json
└── not-a-bundle\        ← no envoy_env/, skipped
```

### 2. Config File (`--bundles-config` / `-bc`)

Explicitly list bundles in a JSON file and pass it with `--bundles-config`/`-bc`:

```json
{
    "bundles": [
        "R:/repo/gtvfx-contrib/gt/unreal/wrapper",
        "R:/repo/gtvfx-contrib/gt/globals",
        "R:/repo/gtvfx-contrib/gt/pythoncore"
    ]
}
```

Or as a direct array:

```json
[
    "R:/repo/gtvfx-contrib/gt/unreal/wrapper",
    "R:/repo/gtvfx-contrib/gt/globals"
]
```

Usage:

```powershell
envoy --bundles-config R:/repo/bundles.json --list
envoy -bc R:/repo/bundles.json --list
envoy -bc R:/repo/bundles.json unreal
```

### 3. Local Fallback

If no bundles are found via the above methods, Envoy walks up from the current directory looking for `envoy_env/commands.json`. This allows running from inside a single-bundle project without any configuration.

## Bundle Structure

A valid bundle must have an `envoy_env/` directory. A `.git/` directory is required for auto-discovery but not for config-file or local-fallback discovery.

```
my-bundle/
├── .git/                       # required for auto-discovery
├── envoy_env/
│   ├── commands.json           # command definitions
│   ├── global_env.json         # loaded automatically before every command's env files
│   ├── base_env.json           # shared environment
│   └── tool_env.json           # per-tool overrides
└── src/
```

### `global_env.json`

If a bundle contains `envoy_env/global_env.json`, it is loaded automatically before any command-specific env files for every command sourced from that bundle. Use it for studio-wide or bundle-wide baseline variables:

```json
{
    "PYTHONDONTWRITEBYTECODE": "1",
    "STUDIO": "gtvfx"
}
```

## Priority and Conflict Resolution

### Loading Order

1. `--bundles-config`/`-bc` — if specified, only those bundles are used
2. `ENVOY_BNDL_ROOTS` auto-discovery — if no config file
3. Local `envoy_env/commands.json` fallback — if no bundles found

### Command Conflicts

If two bundles define the same command name, the last loaded bundle wins and a warning is logged:

```
WARNING - Command 'python_dev' from bundle-b overrides existing command from bundle-a
```

Use `--verbose` to surface these warnings.

## CLI Integration

### Listing with Bundle Tags

```powershell
envoy --list
```

Output:
```
Available commands:

  python_dev           → python -X dev  [pythoncore]
  unreal               (executable on PATH)  [unreal]
  vscode               → C:/.../Code.exe --wait  [globals]
```

### Command Info

```powershell
envoy --info unreal
```

Output:
```
Command: unreal
Bundle: unreal
Executable: unreal
Environment files:
  - unreal_env.json
Environment directory: R:/repo/.../unreal/wrapper/envoy_env
```

### Verbose Discovery Logging

```powershell
envoy --verbose --list
```

Shows which roots were scanned, which bundles qualified, which commands were loaded, and any conflicts.

## Examples

### Example 1: Auto-Discovery

```powershell
$env:ENVOY_BNDL_ROOTS = "R:/repo/gtvfx-contrib"
envoy --list
envoy unreal
envoy python_dev script.py
```

### Example 2: Config File

```powershell
envoy -bc R:/repo/bundles.json --list
envoy -bc R:/repo/bundles.json unreal
```

### Example 3: Multi-Bundle Registry

With two bundles:

**`app-framework/envoy_env/commands.json`:**
```json
{
    "python_dev": {
        "environment": ["python_env.json"],
        "alias": ["python", "-X", "dev"]
    }
}
```

**`tools/envoy_env/commands.json`:**
```json
{
    "build": {
        "environment": ["build_env.json"],
        "alias": ["make", "build"]
    }
}
```

Both commands are available after discovery:

```
envoy --list

  python_dev           → python -X dev  [app-framework]
  build                → make build  [tools]
```

## Python API

```python
from gt.envoy import get_bundles, BundleInfo

# Auto-discovery (reads ENVOY_BNDL_ROOTS)
bundles = get_bundles()

# From config file
from pathlib import Path
bundles = get_bundles(config_file=Path("bundles.json"))

# Inspect bundles
for bundle in bundles:
    print(f"{bundle.name}: {bundle.root}")
    print(f"  envoy_env: {bundle.envoy_env}")
    # env_files is pre-indexed at discovery time: dict[str, Path]
    print(f"  env files: {list(bundle.env_files.keys())}")
```

### Loading Commands from Bundles

```python
from gt.envoy import CommandRegistry, get_bundles

registry = CommandRegistry()
bundles = get_bundles()
registry.load_from_bundles(bundles)

for cmd_name in registry.list_commands():
    cmd = registry.get(cmd_name)
    print(f"{cmd_name} from bundle: {cmd.bundle}")
```

### `BundleInfo` Attributes

| Attribute | Type | Description |
|---|---|---|
| `name` | `str` | Bundle directory name |
| `root` | `Path` | Absolute path to the bundle root |
| `envoy_env` | `Path` | Absolute path to `envoy_env/` |
| `env_files` | `dict[str, Path]` | All `*.json` files in `envoy_env/`, indexed by filename at construction time |

## Environment Variable Reference

| Variable | Separator | Description |
|---|---|---|
| `ENVOY_BNDL_ROOTS` | `;` (Windows) / `:` (Unix) | Root directories to scan for bundles |

## Troubleshooting

**No commands loaded**
- Check `ENVOY_BNDL_ROOTS` is set and points to directories that contain bundle subdirectories
- Each bundle must have both `.git/` (auto-discovery) and `envoy_env/`
- Use `--bundles-config`/`-bc` with explicit paths to bypass auto-discovery
- Run `envoy --verbose --list` to see exactly what is being scanned

**Bundle not discovered**
1. Does the directory have `envoy_env/`?
2. Does it have `.git/` (required for auto-discovery)?
3. Is its parent directory in `ENVOY_BNDL_ROOTS`?
4. Is the path separator correct for the OS (`;` Windows, `:` Unix)?

**Command resolves to wrong bundle**
Multiple bundles define the same command name — last loaded wins. Use `--verbose` to see the override warning, then adjust bundle order in your config file or `ENVOY_BNDL_ROOTS`.
