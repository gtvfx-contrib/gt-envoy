# Troubleshooting

## Start Here — `envoy --diagnose`

Before digging into a specific symptom below, run:

```powershell
en --diagnose               # bundles, team/pipeline, cache, VCS, telemetry status
en --diagnose python    # also shows the command's full resolved environment
```

This surfaces discovered bundles and commands, resolved team/pipeline context,
package cache location and reachability, detected VCS backend, and
bundle-root reachability (flagging unreachable network paths) in one report —
often enough to spot the problem before checking the more specific sections
below.

## "Could not find commands.json"

Envoy has no commands to load. Fix one of:

- Set `ENVOY_BNDL_ROOTS` to point to a directory containing bundle subdirectories
- Use `--bundles-config` / `-b` with a path to a `bundles.json`
- Use `--commands-file` / `-c` with a direct path to `commands.json`
- Run from inside a project that has `.envoy/commands.json`

## Commands Not Appearing in `--list`

```powershell
en --verbose --list
```

Check the verbose output for:

- Which roots are being scanned
- Which directories have `.git/` but no `.envoy/` (or vice versa)
- Any parse errors in `commands.json`

**Common causes:**

| Symptom | Likely cause |
|---|---|
| Root scanned, no bundles found | Bundles are not direct children of the root (scan is one level deep) |
| Bundle found but no commands | `commands.json` missing, empty, or has a JSON syntax error |
| Bundle not found at all | `.git/` directory missing (required for auto-discovery) |
| Command from wrong bundle | Command conflict — last bundle wins; check with `--verbose` |

## Executable Not Found

In closed mode the subprocess `PATH` comes entirely from bundle env files. If the executable is not found:

1. Ensure the bundle env file sets `+=PATH` pointing to the executable's directory
2. Use `en --which <command>` to see what path resolves against the subprocess `PATH`
3. Use `en -i <command>` temporarily to confirm the executable is present on the system `PATH`

```powershell
en --which python          # check resolved path
en -i python script.py     # run with system PATH inherited
```

## Environment Variables Not Applying

1. Check JSON syntax in env files — use a JSON linter
2. Run `en --verbose <command>` to see exactly which files are loaded and what values are set
3. In closed mode, `${VARNAME}` references to system variables expand to empty string unless the variable is in `ENVOY_ALLOWLIST` or is a core OS variable
4. Use `en --trace VAR <command>` to see how a specific variable is mutated across files

```powershell
en --trace PYTHONPATH python
en --verbose python --help
```

## Path Inconsistency (Mixed Slashes)

Envoy normalizes all env file values to OS-native separators before applying them. If you see mixed slashes in a subprocess, the value likely came directly from the system environment (via `ENVOY_ALLOWLIST` or `--inherit-env`) rather than from a bundle env file.

Set the variable explicitly in an env file to ensure normalization is applied:

```json
{
    "SOME_PATH": "R:/repo/my-bundle/data"
}
```

## Null/Unresolved Variable Warnings

```
WARNING: Variable 'SITE_PACKAGES' is null in python_env.json — skipping.
WARNING: List item '${ENVOY_SITE_PACKAGES}' in 'PYTHONPATH' (python_env.json)
         contains unresolved references: ENVOY_SITE_PACKAGES — skipping item.
```

These warnings are informational — they indicate a variable or list item was intentionally or accidentally left undefined. To suppress:

- Remove the entry from the JSON file if not needed
- Set the variable before it is referenced (earlier env file, `global_env.json`, or `ENVOY_ALLOWLIST`)
- Use `?=` to make the value conditional instead of referencing an undefined variable

## `engit` Issues

### `engit tag` fails — no existing tags

```powershell
engit tag --version 0.1.0     # provide explicit version for first tag
```

### `engit release` fails — no `gh` CLI

`engit release` requires the [GitHub CLI (`gh`)](https://cli.github.com/) to be installed and authenticated:

```powershell
gh auth login
```

### `engit publish --version` — no git tags found

Use `--version dev` for a test build:

```powershell
engit publish --version dev --zip
```
