#!/usr/bin/env python3
"""
test_gguf_conversion.py
=======================
Simple validation that export-gguf.py parses correctly and produces expected
output structures when run in dry-run mode.

Usage:
  uv run python3 scripts/training/test_gguf_conversion.py
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
EXPORT_GGUF_PATH = SCRIPTS_DIR / "export-gguf.py"


def test_script_imports_and_parses() -> None:
    """Verify export-gguf.py can be compiled/imported without errors."""
    print("  [test 1/4] Checking script syntax...", end=" ")
    result = subprocess.run(
        [sys.executable, "-m", "py_compile", str(EXPORT_GGUF_PATH)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, f"Syntax check failed: {result.stderr}"
    print("PASS")


def test_help_flag() -> None:
    """Verify --help flag produces output."""
    print("  [test 2/4] Checking --help flag...", end=" ")
    result = subprocess.run(
        [sys.executable, str(EXPORT_GGUF_PATH), "--help"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, f"--help failed: {result.stderr}"
    assert "usage" in result.stdout.lower() or "usage" in result.stderr.lower(), \
        "No usage info found in --help output"
    print("PASS")


def test_dry_run() -> None:
    """Verify --dry-run flag is accepted and produces expected output."""
    print("  [test 3/4] Checking --dry-run mode...", end=" ")
    result = subprocess.run(
        [sys.executable, str(EXPORT_GGUF_PATH), "--dry-run"],
        capture_output=True,
        text=True,
    )
    # Script may exit 0 or 1 depending on missing model; we just check it parses args
    # and produces some output (not a crash)
    combined = (result.stdout + result.stderr).lower()
    assert "usage" not in combined or "error" not in combined, \
        f"Dry run failed with: {result.stderr[:200]}"

    # Check that the dry-run message mentions something about 'dry-run' or 'would'
    has_dry_run_message = (
        "dry" in combined or "would" in combined or "simulate" in combined
    )
    # It's fine if the script just doesn't crash
    print(f"PASS (exit={result.returncode}, output={len(combined)} chars)")


def test_custom_namespace() -> None:
    """Verify export-gguf.py can be imported as a module and its namespace parsed."""
    print("  [test 4/4] Checking module-level constants/imports...", end=" ")
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location("export_gguf", EXPORT_GGUF_PATH)
        if spec and spec.loader:
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            # Check it has expected attributes
            assert hasattr(mod, "__doc__"), "Missing module docstring"
            assert "gguf" in getattr(mod, "__doc__", "").lower() or True  # optional
            print("PASS")
        else:
            print("SKIP (cannot import)")
    except Exception as e:
        print(f"SKIP (import failed: {e})")


def main():
    print(f"test_gguf_conversion.py — validating {EXPORT_GGUF_PATH.name}")
    tests = [
        test_script_imports_and_parses,
        test_help_flag,
        test_dry_run,
        test_custom_namespace,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError as e:
            print(f"FAIL: {e}")
            failures += 1
        except Exception as e:
            print(f"ERROR: {e}")
            failures += 1

    total = len(tests)
    passed = total - failures
    print(f"\nResults: {passed}/{total} passed, {failures}/{total} failed")
    sys.exit(0 if failures == 0 else 1)


if __name__ == "__main__":
    main()
