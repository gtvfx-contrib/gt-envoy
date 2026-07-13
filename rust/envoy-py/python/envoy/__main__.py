"""Main entry point for running envoy as a module.

Usage:
    python -m envoy [command] [args...]
    python -m envoy --list
    python -m envoy --info <command>

"""

import sys

from . import cli_main

if __name__ == '__main__':
    sys.exit(cli_main())
