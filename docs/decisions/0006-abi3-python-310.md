# 0006: One stable-ABI wheel per platform; Python 3.10 floor

**Status:** accepted, 2026-10-11

**Context.** A wheel per Python version and platform multiplies the build
matrix and must be rebuilt for each new Python release.

**Options.** Per-version wheels; or abi3 (stable ABI) wheels from PyO3, one per
platform, valid for every CPython from the floor upward.

**Decision.** abi3 with a 3.10 floor (`abi3-py310`). Python 3.9 reached end of
life in October 2025. CI tests the floor (3.10) and the newest release (3.14).

**Evidence.** Eight wheels (Linux glibc and musl on x86_64 and aarch64, macOS
on both architectures, Windows x64 and arm64) cover CPython 3.10 and later.
Each is built and tested on its own architecture, `abi3audit` confirms only
the stable ABI is used, and a post-publish job installs from the index on six
runners.
