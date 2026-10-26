"""Run the command-line scanner: ``python -m doormat scan PATH...``."""

from __future__ import annotations

import sys

from doormat.cli import main

# Guarded so that importing this module never starts a scan.
if __name__ == "__main__":
    sys.exit(main())
