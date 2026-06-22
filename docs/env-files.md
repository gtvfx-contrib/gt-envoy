# Environment Files

Environment files are JSON files placed in a bundle's `.envoy/` directory. They are listed in `commands.json` under the `environment` key and loaded in order when a command runs.

## Operators

Every key may carry an optional operator prefix:

| Key syntax | Effect |
|---|---|
| `"VAR": "value"` | Assign — set `VAR` to `value`, replacing any existing value |
| `"+=VAR": "value"` | Append — join `value` to the existing `VAR` with the OS path separator |
| `"^=VAR": "value"` | Prepend — join `value` before the existing `VAR` with the OS path separator |
| `"?=VAR": "value"` | Default — set `VAR` only if it is not already defined |

The OS path separator is `;` on Windows and `:` on Unix/macOS.

## Values

### Strings

```json
{
    "APP_NAME": "MyApp",
    "DEBUG": "1",
    "LOG_LEVEL": "DEBUG"
}
```

### Lists (path arrays)

A JSON array is joined into a single string using the OS path separator. This is the recommended form for `PATH`, `PYTHONPATH`, and similar multi-path variables:

```json
{
    "PYTHONPATH": [
        "${__BUNDLE__}/py",
        "${__BUNDLE__}/vendor",
        "R:/shared/libs"
    ]
}
```

### Null / Undefined Values

Setting a key to `null` skips that entry entirely and emits a warning. No value is written to the environment:

```json
{
    "SITE_PACKAGES": null
}
```

```
WARNING: Variable 'SITE_PACKAGES' is null in python_env.json — skipping.
```

When a list item resolves to `null` or contains an unresolved `${VAR}` reference (where `VAR` is not defined), only that item is dropped. The other items in the list are still applied:

```json
{
    "PYTHONPATH": [
        "${__BUNDLE__}/py",
        "${ENVOY_SITE_PACKAGES}",
        "R:/shared/libs"
    ]
}
```

If `ENVOY_SITE_PACKAGES` is not defined, only that entry is skipped and a warning is emitted.

### Optional Variable References (`${?VAR}`)

Prefix the variable name inside `${}` with `?` to mark a reference as **optional**. If the variable is not defined at expansion time, the item or entry is **silently dropped** — no warning is emitted:

```json
{
    "^=PYTHONPATH": [
        "${?ENVOY_SITE_PACKAGES}/Python311/site-packages",
        "R:/always/included"
    ]
}
```

- If `ENVOY_SITE_PACKAGES` **is** set: the item expands normally and is included.
- If `ENVOY_SITE_PACKAGES` **is not** set: the item is silently dropped. No warning.

This also applies to scalar values. If the optional variable is undefined, the entire assignment is silently skipped:

```json
{
    "MY_SITE_DIR": "${?ENVOY_SITE_PACKAGES}/Python311"
}
```

If `ENVOY_SITE_PACKAGES` is not set, `MY_SITE_DIR` will not be present in the environment at all.

!!! note "Optional references act as a conditional gate"
    The `?` prefix turns the entire list item (or scalar entry) into a conditional: the item
    is only included if the referenced variable is defined. This is distinct from the required
    form `${VAR}`, which always emits a warning when undefined.

**Combining optional and required references:**

If a list item contains both `${?OPT}` and `${REQ}`, and `OPT` is undefined, the item is
silently dropped regardless of whether `REQ` is defined. The optional ref fires first.

**Using `environment_allowlist` with optional references:**

The most common pattern is to declare the variable in `environment_allowlist`. This seeds the
variable from the calling environment before any `environment` entries are processed, ensuring
it is available at expansion time:

```json
{
    "environment": {
        "^=PYTHONPATH": [
            "${?ENVOY_SITE_PACKAGES}/Python311/site-packages",
            "${?ENVOY_SITE_PACKAGES}/Python311/Scripts"
        ]
    },
    "environment_allowlist": ["ENVOY_SITE_PACKAGES"]
}
```

If `ENVOY_SITE_PACKAGES` is set in the calling environment, both entries are included.
If it is not set, both are silently omitted.

## Variable Expansion

Use `${VARNAME}` to reference a variable already in scope. References resolve against the environment being built, not the raw system environment:

```json
{
    "APP_ROOT": "${__BUNDLE__}",
    "APP_BIN":  "${APP_ROOT}/bin",
    "+=PATH":   "${APP_BIN}"
}
```

## Special Variables

These are automatically available in every env file:

| Variable | Value |
|---|---|
| `${__BUNDLE__}` | Bundle root directory (parent of `.envoy/`) |
| `${__BUNDLE_ENV__}` | The `.envoy/` directory |
| `${__BUNDLE_NAME__}` | Bundle directory name |
| `${__FILE__}` | Full path of the current JSON file being loaded |

## Path Normalization

All paths in env file values are normalized to OS-native separators at the time they are applied to the environment. On Windows, forward slashes are converted to backslashes automatically.

You may write paths with either forward or back slashes in your JSON files — the output will always be consistent for the target OS.

## `global_env.json`

If a bundle contains `.envoy/global_env.json`, it is loaded automatically before any command-specific env files for every command sourced from that bundle. Use it for bundle-wide or studio-wide baseline variables:

```json
{
    "PYTHONDONTWRITEBYTECODE": "1",
    "STUDIO": "gtvfx"
}
```

In multi-bundle mode, `global_env.json` is collected from **every** discovered bundle in discovery order. Bundle order controls how `+=`/`^=` operators compose across these baseline layers.

## Loading Order

For a command `python_dev` with `"environment": ["base_env.json", "dev_env.json"]`:

```mermaid
flowchart TD
    A["global_env.json\n(all bundles, discovery order)"] --> B["base_env.json"]
    B --> C["dev_env.json"]
    C --> D[("Subprocess\nEnvironment")]

    style A fill:#1e3a5f,color:#fff
    style D fill:#1a3a1a,color:#fff
```

Later files override earlier ones for plain assignment (`=`). Append/prepend operators accumulate.

## Examples

### Python environment

```json
{
    "+=PYTHONPATH": [
        "${__BUNDLE__}/py",
        "${__BUNDLE__}/vendor"
    ],
    "PYTHONDONTWRITEBYTECODE": "1",
    "PYTHONUTF8": "1"
}
```

### Application with optional site packages

```json
{
    "environment": {
        "^=PYTHONPATH": [
            "${__BUNDLE__}/py",
            "${?ENVOY_SITE_PACKAGES}/Python311/site-packages",
            "R:/shared/libs/py"
        ]
    },
    "environment_allowlist": ["ENVOY_SITE_PACKAGES"]
}
```

If `ENVOY_SITE_PACKAGES` is not set in the calling environment, the optional entry is
silently omitted. The other paths are always included.

### Default / fallback variable

```json
{
    "?=LOG_DIR": "${__BUNDLE__}/logs"
}
```

Sets `LOG_DIR` only if it has not already been set by an earlier env file.

### Full chaining example

**`base_env.json`:**
```json
{
    "APP_ROOT": "${__BUNDLE__}",
    "LOG_LEVEL": "WARNING"
}
```

**`dev_env.json`:**
```json
{
    "APP_CONFIG": "${APP_ROOT}/config/dev.json",
    "LOG_LEVEL": "DEBUG"
}
```

`APP_ROOT` is set by `base_env.json` and referenced by `dev_env.json`. `LOG_LEVEL` is overridden to `DEBUG`.
