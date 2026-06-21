#!/usr/bin/env python3
"""Read token-counts.json and print a human-readable summary."""
import json
import sys
from pathlib import Path

TOKEN_FILE = Path(__file__).resolve().parent / "train" / "token-counts.json"

if not TOKEN_FILE.exists():
    print("  Run 'just ledgrrr train-data' first.")
    sys.exit(0)

with open(TOKEN_FILE) as f:
    data = json.load(f)

fields = [
    ("Total tokens", "total_tokens"),
    ("Total pairs", "total_pairs"),
    ("Avg tokens/pair", "avg_tokens_per_pair"),
    ("Median tokens", "median_tokens"),
    ("P99 tokens", "p99_tokens"),
    ("Min tokens", "min_tokens"),
    ("Max tokens", "max_tokens"),
]

print("=== Token Count Report ===")
for label, key in fields:
    val = data.get(key, "N/A")
    if isinstance(val, float):
        print(f"  {label:20s}: {val:.1f}")
    else:
        print(f"  {label:20s}: {val}")
