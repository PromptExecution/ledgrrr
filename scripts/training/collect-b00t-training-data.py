#!/usr/bin/env python3
"""
collect-b00t-training-data.py
==============================
Collect b00t command→output pairs from Hermes session history for
Unsloth fine-tuning. Queries the Hermes state.db directly (SQLite FTS5).

Output format: JSONL with {"prompt": "...", "completion": "..."} pairs
(alpaca/chatml format, ready for Unsloth SFTTrainer).

Usage:
  uv run python3 scripts/training/collect-b00t-training-data.py
  uv run python3 scripts/training/collect-b00t-training-data.py --output .b00t/fsl/b00t-commands.jsonl
  uv run python3 scripts/training/collect-b00t-training-data.py --min-examples 500 --max-examples 2000
"""

import argparse
import json
import os
import re
import sqlite3
import sys
from pathlib import Path

# ── paths ───────────────────────────────────────────────────────────────
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
HERMES_DB = Path.home() / ".hermes" / "state.db"

# ── b00t-relevant keyword filter ────────────────────────────────────────
# Only collect messages that contain b00t-related terms — avoids generic
# chat about weather, jokes, etc.
BOOT_KEYWORDS = [
    "b00t", "boot", "datum", "datums", "tomllmd", "justfile",
    "k0mmand3r", "ledgrrr", "unsloth", "fine-tune", "fine.tune",
    "lora", "qlora", "bouncer", "hive", "epoch", "compound-engineering",
    "AGENTS.md", "blessing", "cargo build", "cargo test",
    "huggingface", "gguf", "qwen", "phi-", "llama.cpp",
    "mcp", "MCP server", "skill", "role.toml", "gate",
    "submodule", "submodules", "git worktree", "josh",
    "s6-overlay", "quadlet", "podman", "nvidia", "rtx3090",
    "hermes", "pipeline", "ontology", "focus", "PRD-",
    "node-health", "task queue", "operator", "executive",
]

def is_b00t_relevant(text: str) -> bool:
    """Check if text contains b00t-relevant keywords."""
    text_lower = text.lower()
    return any(kw.lower() in text_lower for kw in BOOT_KEYWORDS)


# ── message pair extraction ─────────────────────────────────────────────

def extract_pairs(db_path: Path, min_pairs: int, max_pairs: int) -> list[dict]:
    """
    Walk Hermes state.db, extract user→assistant message pairs where
    the user message contains b00t-relevant content.
    
    Returns list of {"prompt": user_msg, "completion": assistant_msg} dicts.
    """
    if not db_path.exists():
        print(f"ERROR: Hermes state DB not found: {db_path}", file=sys.stderr)
        return []

    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    
    # Get all user messages with their session_id and id
    user_msgs = conn.execute("""
        SELECT id, session_id, content
        FROM messages
        WHERE role = 'user'
        ORDER BY session_id, id
    """).fetchall()
    
    print(f"Found {len(user_msgs)} user messages across sessions")
    
    # For each user message, find the next assistant message in the same session
    pairs = []
    for row in user_msgs:
        if len(pairs) >= max_pairs:
            break
        
        user_content = row["content"]
        
        # Skip non-b00t content
        if not is_b00t_relevant(user_content):
            continue
        
        # Skip very short messages (accidental, one-word)
        if len(user_content.strip()) < 20:
            continue
        
        # Find next assistant message in same session
        assistant = conn.execute("""
            SELECT content FROM messages
            WHERE session_id = ? AND role = 'assistant' AND id > ?
            ORDER BY id LIMIT 1
        """, (row["session_id"], row["id"])).fetchone()
        
        if not assistant or not assistant["content"]:
            continue
        
        assistant_content = assistant["content"]
        
        # Skip assistant responses that are too short or just "OK"
        if len(assistant_content.strip()) < 50:
            continue
        
        # Truncate very long messages to prevent dominating context
        user_prompt = user_content[:2048]
        assistant_completion = assistant_content[:2048]
        
        pairs.append({
            "prompt": user_prompt.strip(),
            "completion": assistant_completion.strip(),
        })
    
    conn.close()
    return pairs


# ── JSONL output ────────────────────────────────────────────────────────

def write_jsonl(pairs: list[dict], output_path: Path):
    """Write training pairs to JSONL file."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        for pair in pairs:
            f.write(json.dumps(pair, ensure_ascii=False) + "\n")


def print_stats(pairs: list[dict]):
    """Print summary statistics."""
    if not pairs:
        print("No pairs collected.")
        return
    
    prompt_lens = [len(p["prompt"]) for p in pairs]
    completion_lens = [len(p["completion"]) for p in pairs]
    
    print(f"\n{'=' * 60}")
    print(f"B00T TRAINING DATA COLLECTION")
    print(f"{'=' * 60}")
    print(f"Total pairs:      {len(pairs)}")
    print(f"Prompt avg len:   {sum(prompt_lens) // len(prompt_lens)} chars")
    print(f"Completion avg:   {sum(completion_lens) // len(completion_lens)} chars")
    print(f"Prompt min/max:   {min(prompt_lens)} / {max(prompt_lens)}")
    print(f"Completion min/max: {min(completion_lens)} / {max(completion_lens)}")
    
    # Sample preview
    if pairs:
        print(f"\n Sample pair #{len(pairs)//2}:")
        print(f"   Prompt: {pairs[len(pairs)//2]['prompt'][:150]}...")
        print(f"   Completion: {pairs[len(pairs)//2]['completion'][:150]}...")


# ── main ────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Collect b00t command→output training pairs from Hermes sessions"
    )
    parser.add_argument(
        "--output", default=str(REPO_ROOT / ".b00t" / "fsl" / "b00t-commands.jsonl"),
        help="Output JSONL path (default: .b00t/fsl/b00t-commands.jsonl)"
    )
    parser.add_argument(
        "--min-examples", type=int, default=100,
        help="Minimum examples to collect (default: 100)"
    )
    parser.add_argument(
        "--max-examples", type=int, default=2000,
        help="Maximum examples to collect (default: 2000)"
    )
    parser.add_argument(
        "--db", default=str(HERMES_DB),
        help="Hermes state DB path"
    )
    args = parser.parse_args()
    
    output_path = Path(args.output)
    
    print(f"Source DB: {args.db}")
    print(f"Output:    {output_path}")
    print(f"Target:    {args.min_examples}–{args.max_examples} pairs")
    print()
    
    # Extract pairs from Hermes session DB
    pairs = extract_pairs(
        Path(args.db),
        min_pairs=args.min_examples,
        max_pairs=args.max_examples,
    )
    
    if len(pairs) < args.min_examples:
        print(f"\n⚠ WARNING: Only collected {len(pairs)} pairs (minimum: {args.min_examples})", file=sys.stderr)
        # Still write what we have — partial data is better than none
        if len(pairs) == 0:
            print("No b00t-relevant pairs found. Check the keyword filter.", file=sys.stderr)
            sys.exit(1)
    
    # Write output
    write_jsonl(pairs, output_path)
    print(f"\n✓ Wrote {len(pairs)} training pairs to {output_path}")
    
    # Stats
    print_stats(pairs)
    
    # Guard check
    if len(pairs) < 500:
        print(f"\n⚠ Target: 500 examples (training datum requirement)")
        print(f"   Current: {len(pairs)} — continue collecting sessions or adjust --min-examples")


if __name__ == "__main__":
    main()
