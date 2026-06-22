# Environment JSON File Examples

This directory contains example JSON files demonstrating how to configure environment variables for the ApplicationWrapper.

## Path Formats

**Recommended:** Use Unix-style forward slashes (`/`) for all paths - they work cross-platform and are automatically converted to backslashes on Windows.

**Paths as Lists:** Provide multiple paths as JSON arrays for cleaner, more readable configuration:

```json
{
  "PYTHONPATH": [
    "R:/repo/project1/py",
    "R:/repo/project2/py",
    "R:/repo/project3/py"
  ]
}
```

Lists are automatically joined with the OS-appropriate separator (`;` on Windows, `:` on Unix).

## Basic Usage

### Single Environment File

**example_env.json** - Complete environment setup:
```json
{
  "PYTHONPATH": [
    "path1",
    "path2",
    "path3"
  ],
  "PATH": ["binpath1", "binpath2"],
  "APP_NAME": "MyApp",
  "DEBUG": "true"
}
```

Usage:
```python
from gt.app.wrapper import WrapperConfig, ApplicationWrapper

config = WrapperConfig(
    executable="python",
    args=["script.py"],
    env_files="example_env.json"
)
```

## Variable Expansion (Append/Prepend)

You can append or prepend to existing environment variables using two methods:

### Special Wrapper Variables

The wrapper provides special internal variables that resolve to bundle-relative paths:

**Available Special Variables:**
- `{$__BUNDLE__}` - Root directory of the bundle (parent of `.envoy/` directory)
- `{$__BUNDLE_ENV__}` - The `.envoy/` directory itself
- `{$__BUNDLE_NAME__}` - Name of the bundle (directory name)
- `{$__FILE__}` - Path to the current environment JSON file

**Example:**
```json
{
  "+=PYTHONPATH": [
    "{$__BUNDLE__}/py",
    "{$__BUNDLE__}/lib/python"
  ],
  "+=PATH": "{$__BUNDLE__}/bin",  
  "APP_ROOT": "{$__BUNDLE__}",
  "APP_NAME": "{$__BUNDLE_NAME__}",
  "CONFIG_PATH": "{$__BUNDLE__}/config"
}
```

**Bundle Structure:**
```
my-bundle/
├── .envoy/
│   └── base.json    ← {$__FILE__} points here
│                    ← {$__BUNDLE_ENV__} points to .envoy/
├── bin/             ← {$__BUNDLE__} points to my-bundle/
├── py/              ← {$__BUNDLE_NAME__} = "my-bundle"
└── config/
```

These special variables make your environment files portable and reusable across different installations.

### Method 1: Operator Syntax (Recommended)

Use `+=` to append and `^=` to prepend:

**Append with +=**
```json
{
  "+=PYTHONPATH": ["R:/new/path1", "R:/new/path2"],
  "+=PATH": "R:/repo/tools/bin"
}
```

**Prepend with ^=**
```json
{
  "^=PYTHONPATH": ["R:/new/path"],
  "^=PATH": "R:/repo/tools/bin"
}
```

Works with both strings and arrays. The path separator is automatically added.

### Method 2: Variable Expansion with {$VARNAME}

Reference existing environment variables using `{$VARNAME}` syntax:

**Append to Existing Variable**

**example_env_append.json**:
```json
{
  "PYTHONPATH": "{$PYTHONPATH};R:/new/path",
  "PATH": "{$PATH};R:/repo/tools/bin"
}
```

**Prepend to Existing Variable**

**example_env_prepend.json**:
```json
{
  "PYTHONPATH": "R:/new/path;{$PYTHONPATH}",
  "PATH": "R:/repo/tools/bin;{$PATH}"
}
```

### Combining Lists with Operators

```json
{
  "+=PYTHONPATH": [
    "R:/repo/module1/py",
    "R:/repo/module2/py"
  ],
  "^=PATH": "R:/myapp/bin",
  "APP_ROOT": "R:/myapp"
}
```

### Chaining Multiple Expansions

When using multiple JSON files, variable expansion happens in sequence:

```python
# File 1: base.json
{
  "MY_PATH": "{$MY_PATH};/added/first"
}

# File 2: override.json
{
  "MY_PATH": "/added/second;{$MY_PATH}"
}

# Result: /added/second;[original];/added/first
```

Usage:
```python
config = WrapperConfig(
    executable="python",
    args=["script.py"],
    env_files=["base.json", "override.json"]
)
```

## Multiple Environment Files (Layered Configuration)

**example_env_base.json** - Base/production settings:
```json
{
  "PYTHONPATH": "required_paths",
  "DEBUG": "false",
  "LOG_LEVEL": "WARNING"
}
```

**example_env_dev.json** - Development overrides:
```json
{
  "DEBUG": "true",
  "LOG_LEVEL": "DEBUG",
  "DEVELOPMENT_MODE": "true"
}
```

Usage (files merged in order, later files override earlier):
```python
config = WrapperConfig(
    executable="python",
    args=["script.py"],
    env_files=["example_env_base.json", "example_env_dev.json"]
)
```

### Combining Files and Direct Environment

```python
config = WrapperConfig(
    executable="python",
    args=["script.py"],
    env_files=["example_env_base.json", "example_env_dev.json"],
    env={"CUSTOM_VAR": "overrides_everything"}  # This takes highest priority
)
```

## Environment Variable Priority

When multiple sources provide environment variables, they merge with this priority (later overrides earlier):

1. **System environment** (if `inherit_env=True`, which is default)
2. **JSON files** (in order specified in `env_files` list)
3. **Direct `env` dict** (highest priority)

## Path Variables on Windows

For `PATH` and `PYTHONPATH` on Windows:
- Use semicolon (`;`) as separator
- Use double backslashes (`\\`) or forward slashes (`/`) in paths
- Example: `"C:\\Python312;C:\\Tools"`
**Recommended:** Use forward slashes (`/`) - automatically converted to backslashes
- Separator automatically determined (`;` on Windows, `:` on Unix)
- Example: `"C:/Python312"` becomes `"C:\\Python312"` on Windows

## Path Variables on Linux/Mac

For `PATH` and `PYTHONPATH` on Linux/Mac:
- Use forward slashes (`/`) 
- Separator is colon (`:`)
- Example: `"/usr/local/bin"`

## Value Types

### String Values
```json
{
  "APP_NAME": "MyApp",
  "SINGLE_PATH": "R:/repo/path"
}
```

### List/Array Values (for paths)
```json
{
  "PYTHONPATH": [
    "R:/repo/path1",
    "R:/repo/path2",
    "R:/repo/path3"
  ]
}
```

Lists are automatically joined with the OS path separator.

### Mixed Usage
```json
{
  "PYTHONPATH": ["R:/path1", "R:/path2"],
  "+=PATH": "R:/extra/bin",
  "APP_NAME": "MyApp"
}
``
- `PYTHONPATH` - Python module search paths
- `PATH` - Executable search paths
- `DEBUG` - Debug mode flag
- `LOG_LEVEL` - Logging verbosity (DEBUG, INFO, WARNING, ERROR)
- `TEMP` / `TMP` - Temporary directory
- Custom application-specific variables
