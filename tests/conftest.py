"""Test configuration for headless/local execution."""

import os
from pathlib import Path


_TEST_STATE_ROOT = Path(__file__).resolve().parents[1] / ".local_state" / "tests"
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("XDG_CACHE_HOME", str(_TEST_STATE_ROOT / "cache"))
os.environ.setdefault("XDG_CONFIG_HOME", str(_TEST_STATE_ROOT / "config"))
