# Python API

Envoy exposes a Python API for working with bundles, commands, and managed subprocesses.

## Installation

```bash
pip install envoy-<version>-py3-none-any.whl
```

Or from source:

```bash
pip install -e /path/to/envoy/py
```

## Quick Start

```python
import envoy
import envoy.proc as proc

bundle = envoy.Bundle('gt:pythoncore')
env = proc.Environment('python_dev')
proc.call(['python', 'script.py'], environment=env)
```

## `envoy.Bundle`

Represents a single bundle by filesystem path or bundle ID.

```python
# By bundle ID (resolved from ENVOY_BNDL_ROOTS)
bundle = envoy.Bundle('gt:pythoncore')

# By filesystem path
bundle = envoy.Bundle(r'R:/repo/gtvfx-contrib/gt/pythoncore')
```

### Attributes

| Attribute | Type | Description |
|---|---|---|
| `bndlid` | `str` | Bundle ID in `namespace:name` form |
| `namespace` | `str` | Namespace inferred from parent directory name |
| `name` | `str` | Bundle directory name |
| `version` | `str` | `'checkout'` for live checkouts; semver string for production bundles |
| `is_checkout` | `bool` | `True` if this is a live git checkout |
| `is_production` | `bool` | `True` if this is a deployed production build |
| `commands` | `list[str]` | Available command names |
| `env_files` | `dict[str, Path]` | All `*.json` files in `envoy_env/`, indexed by filename |

```python
print(bundle.bndlid)       # 'gt:pythoncore'
print(bundle.namespace)    # 'gt'
print(bundle.name)         # 'pythoncore'
print(bundle.version)      # 'checkout'
print(bundle.is_checkout)  # True
print(bundle.commands)     # ['python_dev', ...]
print(bundle.env_files)    # {'python_env.json': Path(...), ...}
```

### Namespace override

```python
bundle = envoy.Bundle(r'R:/repo/gtvfx-contrib/gt/pythoncore', namespace='vfx')
print(bundle.bndlid)  # 'vfx:pythoncore'
```

## `envoy.BundleConfig`

Load and inspect a bundle config file (flat list of bundle paths).

```python
cfg = envoy.BundleConfig(r'R:/studio/bundles.json')

for b in cfg.bundles:          # list[Bundle]
    print(b.bndlid, b.version)

print(cfg.commands)            # merged command list across all bundles
```

## `envoy.proc`

Managed subprocess execution using a command's configured environment.

### `proc.Environment`

Build the environment for a command:

```python
env = proc.Environment('python_dev')
```

By default uses bundle auto-discovery via `ENVOY_BNDL_ROOTS`. Pass a `BundleConfig` to use a specific config:

```python
cfg = envoy.BundleConfig(r'R:/studio/bundles.json')
env = proc.Environment('python_dev', config=cfg)
```

### `proc.call`

Run a command and wait for it to complete:

```python
proc.call(['python', 'script.py'], environment=env)
proc.call(['python', 'script.py', '--arg', 'value'], environment=env)
```

### `proc.check_output`

Run a command and capture its stdout:

```python
output = proc.check_output(['python', '-c', 'print(42)'], environment=env)
print(output)  # b'42\n'
```

## Exception Handling

```python
try:
    proc.call(['nuke', 'comp.nk'], environment=env)
except envoy.CalledProcessError as e:
    print(e.returncode, e.cmd)
except envoy.CommandNotFoundError:
    print("command not found")
```

## Constants

```python
envoy.BUNDLE_CHECKOUT           # 'checkout'
envoy.BUNDLE_DEFAULT_NAMESPACE  # 'gt'
```

## Verbosity

```python
envoy.set_api_verbosity('DEBUG')   # 'DEBUG', 'INFO', 'WARNING', 'ERROR'
```

## Full Example

```python
import envoy
import envoy.proc as proc

envoy.set_api_verbosity('INFO')

# Inspect available bundles
cfg = envoy.BundleConfig(r'R:/studio/bundles.json')
for b in cfg.bundles:
    print(f"{b.bndlid}  ({b.version})")
    for cmd in b.commands:
        print(f"  {cmd}")

# Run a command
env = proc.Environment('python_dev', config=cfg)
result = proc.check_output(
    ['python', '-c', 'import sys; print(sys.version)'],
    environment=env,
)
print(result.decode())
```
