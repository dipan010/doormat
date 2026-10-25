"""Run the command-line scanner: ``python -m doormat scan PATH...``."""

from __future__ import annotations

import sys

from doormat.cli import main

sys.exit(main())
