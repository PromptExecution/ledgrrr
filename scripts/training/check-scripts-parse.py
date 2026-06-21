#!/usr/bin/env python3
"""Check that training scripts parse correctly (syntax validation)."""
import ast
import sys
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
files = [
    SCRIPTS_DIR / "fine-tune.py",
    SCRIPTS_DIR / "export-gguf.py",
    SCRIPTS_DIR / "collect-b00t-training-data.py",
]

all_ok = True
for f in files:
    try:
        with open(f) as fh:
            ast.parse(fh.read())
        print(f"  OK  {f.name}")
    except SyntaxError as e:
        print(f"  FAIL {f.name} — SYNTAX ERROR: {e}")
        all_ok = False

sys.exit(0 if all_ok else 1)
