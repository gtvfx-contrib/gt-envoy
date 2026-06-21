# Installation

Each GitHub Release ships the following artefacts. Choose the one that matches your use case.

## Full Bundle [CLI + Python API] (Recommended)

Download `envoy-v<version>.zip` from the latest [GitHub Release](https://github.com/gtvfx-contrib/gt-envoy/releases), extract it, and add it to `ENVOY_BNDL_ROOTS` or add it to a `--bundles-config` file.


```powershell
# Extract to a tools directory
Expand-Archive envoy-v1.0.0.zip -DestinationPath C:\tools

# Register via bundles config
en --bundles-config C:\studio\bundles.json --list
```

**`bundles.json`:**
```json
{
  "bundles": [
    "C:/tools/envoy/v1.0.0"
  ]
}
```

## CLI Tools Only

Download `envoy.exe` and `engit.exe` from the latest [GitHub Release](https://github.com/gtvfx-contrib/gt-envoy/releases) and add their directory to your `PATH`.

```powershell
$env:PATH = "C:\tools\envoy;$env:PATH"
```

## Python API (Wheel)

Download `envoy-*.whl` from the latest [GitHub Release](https://github.com/gtvfx-contrib/gt-envoy/releases) and install it:

```bash
pip install envoy-1.0.0-py3-none-any.whl
```

Then use it in Python:

```python
import envoy
import envoy.proc as proc
```

## Developer / From Source

```powershell
git clone https://github.com/gtvfx-contrib/gt-envoy.git envoy
cd envoy
pip install -e py
# Add bin/ to PATH for the en / engit launchers
$env:PATH = "$PWD\bin;$env:PATH"
```

## PATH Setup

Add the `bin/` directory (or the directory containing the EXEs in a deployed bundle) to your `PATH`. The short alias `en` is the most convenient entry point for daily use.

=== "PowerShell (session)"

    ```powershell
    $env:PATH = "C:\tools\envoy;$env:PATH"
    ```

=== "System PATH (permanent)"

    Add `C:\tools\envoy` via **System Properties → Environment Variables → PATH**.

=== "cmd"

    ```batch
    set PATH=C:\tools\envoy;%PATH%
    ```
