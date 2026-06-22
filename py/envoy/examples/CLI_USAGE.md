# CLI Usage Guide

## Overview

The wrapper can now be used as a command-line interface (CLI) where commands are defined in a `commands.json` file.

## Command Definition

Create a `.envoy/commands.json` file with your command definitions:

```json
{
  "command_name": {
    "environment": ["env1.json", "env2.json"],
    "alias": ["executable", "arg1", "arg2"]
  }
}
```

### Fields

- **command_name**: The name used to invoke the command via CLI
- **environment**: List of JSON environment files to load (relative to `.envoy/`)
- **alias** (optional): Command and base arguments to execute
  - If provided: `alias[0]` is the executable, `alias[1:]` are base arguments
  - If not provided: `command_name` must be an executable on PATH

### Example

```json
{
  "python_dev": {
    "environment": ["example_env.json"],
    "alias": ["python"]
  },
  "python_test": {
    "environment": ["example_env.json", "test_list_paths.json"],
    "alias": ["python", "-X", "dev"]
  },
  "bat_example": {
    "environment": ["example_env.json"]
  }
}
```

## CLI Usage

### Basic Invocation

From the `py/` directory or with `PYTHONPATH` set:

```bash
# On Windows PowerShell
$env:PYTHONPATH = "R:\repo\gtvfx-contrib\gt\app\wrapper\py"
python -m gt.app.wrapper COMMAND [ARGS...]

# On Linux/Mac
export PYTHONPATH="/path/to/py"
python -m gt.app.wrapper COMMAND [ARGS...]
```

### List Available Commands

```bash
python -m gt.app.wrapper --list
```

Output:
```
Available commands:

  bat_example          (executable on PATH)
  python_dev           → python
  python_test          → python -X dev
```

### Show Command Information

```bash
python -m gt.app.wrapper --info COMMAND
```

Output:
```
Command: python_dev
Executable: python
Environment files:
  - example_env.json
Alias: python
```

### Execute a Command

```bash
# Execute python_dev with arguments
python -m gt.app.wrapper python_dev --version

# Execute python_test with code
python -m gt.app.wrapper python_test -c "print('Hello')"
```

### Options

- `--list`: List all available commands
- `--info COMMAND`: Show detailed information about a specific command
- `--commands-file FILE`: Specify a custom commands.json file path
- `--verbose, -v`: Enable verbose logging (shows environment loading, execution details)
- `--help, -h`: Show help message

## Creating Windows Batch Files

To make commands easily accessible on Windows, create `.bat` files:

```batch
@echo off
set PYTHONPATH=R:\repo\gtvfx-contrib\gt\app\wrapper\py
python -m gt.app.wrapper python_dev %*
```

Save as `python_dev.bat` and place it in a directory on your PATH.

## Command Discovery

The CLI automatically searches for `.envoy/commands.json`:

1. Starts in the current directory
2. Checks for `.envoy/commands.json`
3. If not found, moves up to the parent directory
4. Repeats until found or reaches the filesystem root

This allows you to run the CLI from anywhere within your project structure.

## Examples

### Running Python with Custom Environment

```bash
# List commands
python -m gt.app.wrapper --list

# Get command info
python -m gt.app.wrapper --info python_dev

# Run Python with loaded environment
python -m gt.app.wrapper python_dev script.py

# Run with verbose output
python -m gt.app.wrapper --verbose python_dev --version
```

### From Subdirectory

```bash
# Navigate to any subdirectory in your project
cd examples/

# Set PYTHONPATH
$env:PYTHONPATH = "R:\repo\gtvfx-contrib\gt\app\wrapper\py"

# Run command (will auto-detect .envoy/commands.json)
python -m gt.app.wrapper python_dev --help
```

## Integration with Development Workflow

### Option 1: Batch Files

Create batch files for each command and add them to PATH:

```
python_dev.bat    → Calls: gt.app.wrapper python_dev
python_test.bat   → Calls: gt.app.wrapper python_test
```

### Option 2: Shell Aliases

Add to your shell profile:

```bash
# PowerShell ($PROFILE)
function python_dev { python -m gt.app.wrapper python_dev $args }
function python_test { python -m gt.app.wrapper python_test $args }

# Bash (~/.bashrc)
alias python_dev='python -m gt.app.wrapper python_dev'
alias python_test='python -m gt.app.wrapper python_test'
```

### Option 3: Direct CLI Usage

Simply use the CLI directly from your working directory:

```bash
python -m gt.app.wrapper python_dev script.py
```

## Environment File Paths

Environment files in `commands.json` are relative to the `.envoy/` directory:

```
project/
├── .envoy/
│   ├── commands.json
│   ├── base_env.json       ← "base_env.json"
│   └── configs/
│       └── dev.json        ← "configs/dev.json"
```

## Error Handling

If commands.json is not found:
```
Error: Could not find commands.json
Searched for .envoy/commands.json in current directory and parents
```

Solution: Ensure you're in a project directory with an `.envoy/commands.json` file or specify the path with `--commands-file`.

If a command is not found:
```
Error: Command 'invalid_cmd' not found
```

Solution: Check available commands with `--list`.
