"""PyInstaller entry point for the engit executable."""
import sys
from engit._cli import main

if __name__ == '__main__':
    sys.exit(main())
