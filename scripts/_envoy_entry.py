"""PyInstaller entry point for envoy / en executables."""
import sys
from envoy._cli import main

if __name__ == '__main__':
    sys.exit(main())
