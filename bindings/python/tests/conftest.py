import sys
from pathlib import Path

# Ensure the local built extension package is on sys.path for tests
BASE_DIR = Path(__file__).resolve().parents[1]  # bindings/python
PKG_DIR = BASE_DIR / "python"  # bindings/python/python
sys.path.insert(0, str(PKG_DIR))
