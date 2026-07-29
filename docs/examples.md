# Examples

## Example 1 — Python Development Environment

**`.envoy/commands.json`:**
```json
{
    "python": {
        "environment": ["python_env.json"],
        "alias": ["python", "-X", "dev"]
    }
}
```

**`.envoy/python_env.json`:**
```json
{
    "+=PYTHONPATH": "${__BUNDLE__}/src",
    "PYTHONDONTWRITEBYTECODE": "1",
    "PYTHONUTF8": "1"
}
```

**Usage:**
```console
en python script.py
en python -m pytest tests/
```

---

## Example 2 — One Command Across Platforms

**`.envoy/commands.json`:**
```json
{
    "editor": {
        "environment": ["editor_env.json"],
        "alias": ["editor"],
        "platforms": {
            "windows": {
                "alias": ["${__BUNDLE__}/bin/windows/editor.exe"]
            },
            "linux": {
                "alias": ["${__BUNDLE__}/bin/linux/editor"]
            },
            "macos": {
                "alias": ["${__BUNDLE__}/bin/macos/editor"],
                "architectures": {
                    "aarch64": {
                        "environment": [
                            "editor_env.json",
                            "editor_apple_silicon_env.json"
                        ]
                    }
                }
            }
        }
    }
}
```

The base definition remains valid for new operating systems. Known targets
replace only the fields they need, and macOS arm64 adds one environment layer.

**Usage:**

```console
en --info editor
en editor project.scene
```

---

## Example 3 — Application-Specific Environment

Application environment files remain ordinary, portable JSON:

**`.envoy/render_env.json`:**

```json
{
    "+=APP_PLUGIN_PATH": "${__BUNDLE__}/plugins",
    "+=PATH": "${__BUNDLE__}/bin",
    "APP_CONFIG": "${__BUNDLE__}/config/render.json"
}
```

---

## Example 4 — Multi-Bundle Setup

With `ENVOY_BNDL_ROOTS=/workspace/bundles` and two bundles discovered:

```mermaid
flowchart LR
    ROOT["ENVOY_BNDL_ROOTS\n/workspace/bundles"]
    A["gt:build-tools\ndefines: build, test"]
    B["gt:deploy-tools\ndefines: deploy, package"]
    REG["Command Registry"]

    ROOT --> A --> REG
    ROOT --> B --> REG
```

```console
en --list
```

```
Available commands:

  build                [gt:build-tools]
  test                 [gt:build-tools]
  deploy               [gt:deploy-tools]
  package              [gt:deploy-tools]
```

```console
en build --target Release
en deploy --env production
```

---

## Example 5 — Shared Baseline via `global_env.json`

**`gt:globals/.envoy/global_env.json`:**
```json
{
    "PYTHONDONTWRITEBYTECODE": "1",
    "STUDIO": "gtvfx"
}
```

This file is loaded before every command's env files from every bundle. Any bundle can reference `${STUDIO}` in its own env files:

```json
{
    "APP_CACHE": "${HOME}/.cache/${STUDIO}"
}
```

---

## Example 6 — Optional Site Packages

A common pattern where an additional Python path is only included when a specific env var is defined (e.g. set in `ENVOY_ALLOWLIST` on some machines):

**`python_env.json`:**
```json
{
    "PYTHONPATH": [
        "${__BUNDLE__}/py",
        "${ENVOY_SITE_PACKAGES}",
        "${STUDIO_ROOT}/shared/libs/py"
    ]
}
```

When `ENVOY_SITE_PACKAGES` is not defined, envoy warns and skips only that entry:

```
WARNING: List item '${ENVOY_SITE_PACKAGES}' in 'PYTHONPATH' (python_env.json)
         contains unresolved references: ENVOY_SITE_PACKAGES — skipping item.
```

The remaining paths are still applied normally.

---

## Example 7 — Layered Dev / Prod Environments

**`.envoy/commands.json`:**
```json
{
    "app_prod": {
        "environment": ["base_env.json"]
    },
    "app_dev": {
        "environment": ["base_env.json", "dev_env.json"]
    }
}
```

**`base_env.json`:**
```json
{
    "APP_ROOT": "${__BUNDLE__}",
    "LOG_LEVEL": "WARNING",
    "+=PYTHONPATH": "${__BUNDLE__}/py"
}
```

**`dev_env.json`:**
```json
{
    "LOG_LEVEL": "DEBUG",
    "APP_CONFIG": "${APP_ROOT}/config/dev.json",
    "DEVELOPMENT_MODE": "1"
}
```

```mermaid
flowchart TD
    B["base_env.json\nAPP_ROOT, LOG_LEVEL=WARNING, PYTHONPATH"]
    D["dev_env.json\nLOG_LEVEL=DEBUG, APP_CONFIG, DEV_MODE"]
    P["app_prod env\n(base only)"]
    DV["app_dev env\n(base + dev overrides)"]

    B --> P
    B --> D --> DV
```
