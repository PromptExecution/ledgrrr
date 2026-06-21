#!/usr/bin/env python3
"""
generate-training-data.py
=========================
Walks viz-manifest.json, ontology snapshot, and Rhai DSL types to produce
Unsloth-compatible JSONL training pairs. Uses tiktoken for token counting.

Optionally calls the running MCP server (ledgerr-mcp-server on port 3737)
to capture live tool response shapes as additional training examples.

Output (all in scripts/training/):
  train/training-data.jsonl  — all pairs
  train/train.jsonl          — 80% split
  train/val.jsonl            — 20% split
  train/token-counts.json    — summary statistics
"""

import json
import os
import random
import re
import sys
import textwrap
import urllib.request
import urllib.error
from pathlib import Path
from collections import OrderedDict

# ── ensure tiktoken is available ────────────────────────────────────────
try:
    import tiktoken
except ImportError:
    print("Installing tiktoken...", file=sys.stderr)
    import subprocess
    subprocess.check_call([sys.executable, "-m", "pip", "install", "tiktoken"])
    import tiktoken

# ── paths ───────────────────────────────────────────────────────────────
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPTS_DIR = REPO_ROOT / "scripts" / "training"
TRAIN_DIR = SCRIPTS_DIR / "train"

MANIFEST_PATH = REPO_ROOT / "ui" / "docs" / "public" / "viz-manifest.json"
KERM_PATH = REPO_ROOT / "types" / "domain.kerm"
ONTOLOGY_RS_PATH = REPO_ROOT / "crates" / "ledger-core" / "src" / "ontology.rs"
CONTRACT_RS_PATH = REPO_ROOT / "crates" / "ledgerr-mcp" / "src" / "contract.rs"

TRAIN_DIR.mkdir(parents=True, exist_ok=True)

# ── tiktoken setup ──────────────────────────────────────────────────────
ENCODING = tiktoken.get_encoding("cl100k_base")

def token_count(text: str) -> int:
    """Count tokens using tiktoken cl100k_base (GPT-4 tokenizer)."""
    return len(ENCODING.encode(text))

# ── helpers ─────────────────────────────────────────────────────────────

def load_json(path: Path) -> dict:
    with open(path, "r") as f:
        return json.load(f)

def load_text(path: Path) -> str:
    with open(path, "r") as f:
        return f.read()

def cap_source(source: str, max_chars: int = 2048) -> str:
    """Cap rhai_dsl source text at max_chars characters."""
    if len(source) > max_chars:
        return source[:max_chars] + "\n# ... [truncated]"
    return source

# ── 0. Optional live MCP server call ───────────────────────────────────

MCP_SERVER_URL = "http://localhost:3737"

def call_mcp_tool(tool_name: str, params: dict = None) -> dict | None:
    """
    Call an MCP tool on the running ledgerr-mcp-server via JSON-RPC.
    Returns the result dict on success, or None if the server is unreachable.
    """
    if params is None:
        params = {}
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": params,
        },
    }
    try:
        req = urllib.request.Request(
            MCP_SERVER_URL,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            result = json.loads(resp.read().decode("utf-8"))
            return result
    except (urllib.error.URLError, urllib.error.HTTPError, ConnectionRefusedError,
            TimeoutError, OSError) as e:
        print(f"  [MCP] Server not reachable at {MCP_SERVER_URL}: {e}")
        return None
    except json.JSONDecodeError as e:
        print(f"  [MCP] Invalid JSON response: {e}")
        return None

def collect_mcp_training_pairs() -> list[dict]:
    """
    Attempt to collect live training examples from the MCP server.
    Returns a list of training pair dicts, or empty list if server is down.
    """
    pairs = []
    print("Attempting live MCP server connection...")

    # ── Tool: ledgerr_ontology export_snapshot ──────────────────────────
    result = call_mcp_tool("ledgerr_ontology", {"action": "export_snapshot"})
    if result is not None:
        print("  [MCP] ledgerr_ontology export_snapshot: OK")
        response_json = json.dumps(result, indent=2)

        # Truncate very large responses
        if len(response_json) > 4096:
            response_json = response_json[:4096] + "\n  // ... [truncated]"

        pair = {
            "prompt": (
                "You are calling the ledgerr_ontology MCP tool with "
                'action "export_snapshot".\n\n'
                "Predict the JSON-RPC response shape returned by the "
                "ledgerr-mcp-server.\n"
            ),
            "completion": (
                f"The ledgerr_ontology MCP tool returns a JSON-RPC response "
                f"containing the ontology snapshot with ArtifactKind and "
                f"RelationKind variants. The response shape is:\n"
                f"```json\n{response_json}\n```\n"
            ),
            "token_counts": {},
            "metadata": {
                "type_name": "MCP::ledgerr_ontology::export_snapshot",
                "z_layer": "Document",
                "semantic_type": "mcp_action",
                "source": "mcp_live",
                "rhai_dsl_chars": len(response_json),
            },
        }
        pair["token_counts"] = {
            "prompt": token_count(pair["prompt"]),
            "completion": token_count(pair["completion"]),
            "total": token_count(pair["prompt"]) + token_count(pair["completion"]),
        }
        pairs.append(pair)

    # ── Additional MCP tools (if available) ─────────────────────────────
    # Try additional ontology actions
    for action in ["list_snapshots", "get_snapshot_metadata"]:
        result = call_mcp_tool("ledgerr_ontology", {"action": action})
        if result is not None:
            print(f"  [MCP] ledgerr_ontology {action}: OK")
            response_json = json.dumps(result, indent=2)
            if len(response_json) > 4096:
                response_json = response_json[:4096] + "\n  // ... [truncated]"
            pair = {
                "prompt": (
                    f"You are calling the ledgerr_ontology MCP tool with "
                    f'action "{action}".\n\n'
                    "Predict the JSON-RPC response shape returned by the "
                    "ledgerr-mcp-server.\n"
                ),
                "completion": (
                    f"The ledgerr_ontology MCP tool returns a JSON-RPC response "
                    f"for action '{action}'. The response shape is:\n"
                    f"```json\n{response_json}\n```\n"
                ),
                "token_counts": {},
                "metadata": {
                    "type_name": f"MCP::ledgerr_ontology::{action}",
                    "z_layer": "Document",
                    "semantic_type": "mcp_action",
                    "source": "mcp_live",
                    "rhai_dsl_chars": len(response_json),
                },
            }
            pair["token_counts"] = {
                "prompt": token_count(pair["prompt"]),
                "completion": token_count(pair["completion"]),
                "total": token_count(pair["prompt"]) + token_count(pair["completion"]),
            }
            pairs.append(pair)

    if pairs:
        print(f"  [MCP] Collected {len(pairs)} live training pairs from MCP server")
    else:
        print("  [MCP] No live MCP training pairs collected (server may be down)")

    return pairs

# ── 1. Load viz-manifest entries ────────────────────────────────────────

print("Loading viz-manifest.json...")
manifest = load_json(MANIFEST_PATH)
viz_entries = manifest.get("objects", [])
print(f"  Found {len(viz_entries)} entries in viz-manifest.json")

# ── 2. Parse domain.kerm types with rhai_dsl ────────────────────────────

print("Loading domain.kerm...")
kerm_text = load_text(KERM_PATH)

def parse_kerm_types(kerm_text: str):
    """Parse TOML [[type]] blocks from domain.kerm, returning those
    that have a non-empty rhai_dsl field."""
    try:
        import tomllib
    except ImportError:
        tomllib = None

    # Try Python 3.11+ tomllib first
    if tomllib:
        try:
            data = tomllib.loads(kerm_text)
            raw_types = data.get("type", [])
        except Exception:
            raw_types = []
    else:
        raw_types = []

    # Fallback: manual TOML-like parser
    if not raw_types:
        raw_types = []
        current = None
        multi_line_key = None
        multi_line_val = []
        for line in kerm_text.splitlines():
            stripped = line.strip()
            # Section header
            if stripped.startswith("[["):
                # Save previous type
                if current is not None:
                    if multi_line_key:
                        current[multi_line_key] = "\n".join(multi_line_val).strip()
                        multi_line_key = None
                        multi_line_val = []
                    raw_types.append(current)
                current = {}
                continue
            if current is None:
                continue
            # Comment or empty
            if not stripped or stripped.startswith("#"):
                continue
            # Multi-line string continuation
            if multi_line_key:
                if stripped.endswith("'''"):
                    multi_line_val.append(stripped[:-3])
                    current[multi_line_key] = "\n".join(multi_line_val)
                    multi_line_key = None
                    multi_line_val = []
                else:
                    multi_line_val.append(stripped)
                continue
            # Key = value
            if "=" in stripped:
                key, _, val = stripped.partition("=")
                key = key.strip()
                val = val.strip()
                # Multi-line string start
                if val.startswith("'''") and not val.endswith("'''"):
                    multi_line_key = key
                    multi_line_val.append(val[3:])
                elif val.startswith("'''") and val.endswith("'''"):
                    current[key] = val[3:-3]
                else:
                    current[key] = val.strip('"')
        if current is not None:
            if multi_line_key:
                current[multi_line_key] = "\n".join(multi_line_val).strip()
            raw_types.append(current)

    # Filter to types with rhai_dsl
    result = []
    seen = set()
    for t in raw_types:
        label = t.get("label", t.get("id", ""))
        rhai = t.get("rhai_dsl", "")
        if rhai and label not in seen:
            seen.add(label)
            result.append({
                "label": label,
                "id": t.get("id", ""),
                "description": t.get("description", ""),
                "z_layer": t.get("z_layer", ""),
                "semantic_type": t.get("semantic_type", ""),
                "rhai_dsl": rhai,
            })
    return result

kerm_types = parse_kerm_types(kerm_text)
print(f"  Found {len(kerm_types)} types with rhai_dsl in domain.kerm")

# ── 3. Parse ontology enum info ─────────────────────────────────────────

print("Loading ontology.rs...")
ontology_rs = load_text(ONTOLOGY_RS_PATH)

def parse_rust_enum(text: str, enum_name: str) -> list[str]:
    """Extract variant names from a Rust enum definition."""
    pattern = rf"pub enum {enum_name}\s*\{{([^}}]+)\}}"
    m = re.search(pattern, text, re.DOTALL)
    if not m:
        return []
    body = m.group(1)
    variants = re.findall(r"^\s+(\w+),?\s*$", body, re.MULTILINE)
    return variants

artifact_kinds = parse_rust_enum(ontology_rs, "ArtifactKind")
relation_kinds = parse_rust_enum(ontology_rs, "RelationKind")
print(f"  ArtifactKind: {len(artifact_kinds)} variants")
print(f"  RelationKind: {len(relation_kinds)} variants")

# ── 4. Load MCP contract ontology tool info ─────────────────────────────

print("Loading contract.rs...")
contract_rs = load_text(CONTRACT_RS_PATH)

def parse_mcp_ontology_actions(text: str) -> list[dict]:
    """Extract ontology tool actions from contract.rs."""
    pattern = r'ONTOLOGY_TOOL.*?actions:\s*&\[(.*?)\]'
    m = re.search(pattern, text, re.DOTALL)
    if not m:
        return []
    actions_text = m.group(1)
    actions = re.findall(r'"(.*?)"', actions_text)
    return [{"action": a, "purpose": f"MCP ontology action: {a}"} for a in actions]

ontology_actions = parse_mcp_ontology_actions(contract_rs)
print(f"  MCP ontology actions: {len(ontology_actions)}")

# Also grab purpose field
purpose_pattern = r'ONTOLOGY_TOOL,\s*\n\s*purpose:\s*"(.*?)"'
purpose_m = re.search(purpose_pattern, contract_rs, re.DOTALL)
ontology_purpose = purpose_m.group(1) if purpose_m else "ontology query/export/write operations"

# ── 5. Build ontology snapshot context string ───────────────────────────

def build_ontology_context() -> str:
    lines = []
    lines.append("Ontology snapshot schema:")
    lines.append(f"  ArtifactKind variants: {', '.join(artifact_kinds)}")
    lines.append(f"  RelationKind variants: {', '.join(relation_kinds)}")
    lines.append(f"  MCP ontology tool ({ontology_purpose}):")
    for a in ontology_actions:
        lines.append(f"    - {a['action']}")
    return "\n".join(lines)

ontology_context = build_ontology_context()

# ── 6. Merge and deduplicate training entries ───────────────────────────

def make_entry_from_viz(viz_entry: dict) -> dict | None:
    """Build a training entry from a viz-manifest.json entry."""
    type_name = viz_entry.get("type_name", "")
    spec = viz_entry.get("spec", {})
    description = spec.get("description", "")
    z_layer = spec.get("z_layer", "")
    semantic_type = spec.get("semantic_type", "")
    rhai_dsl_raw = spec.get("rhai_dsl", {})

    if isinstance(rhai_dsl_raw, dict):
        source = rhai_dsl_raw.get("source", "")
    elif isinstance(rhai_dsl_raw, str):
        source = rhai_dsl_raw
    else:
        source = ""

    source = cap_source(source, 2048)

    if not source.strip():
        return None  # skip entries without Rhai DSL

    return {
        "type_name": type_name,
        "description": description,
        "z_layer": z_layer,
        "semantic_type": semantic_type,
        "rhai_dsl": source,
        "source": "viz-manifest.json",
    }

def make_entry_from_kerm(kerm_type: dict) -> dict | None:
    """Build a training entry from a domain.kerm [[type]] block."""
    type_name = kerm_type.get("label", kerm_type.get("id", ""))
    description = kerm_type.get("description", "")
    z_layer = kerm_type.get("z_layer", "")
    semantic_type = kerm_type.get("semantic_type", "")
    source = cap_source(kerm_type.get("rhai_dsl", ""), 2048)

    if not source.strip():
        return None

    return {
        "type_name": type_name,
        "description": description,
        "z_layer": z_layer,
        "semantic_type": semantic_type,
        "rhai_dsl": source,
        "source": "domain.kerm",
    }

# Collect all entries, deduplicating by type_name
entries_by_name: dict[str, dict] = OrderedDict()

for ve in viz_entries:
    entry = make_entry_from_viz(ve)
    if entry:
        entries_by_name[entry["type_name"]] = entry

for kt in kerm_types:
    entry = make_entry_from_kerm(kt)
    if entry and entry["type_name"] not in entries_by_name:
        entries_by_name[entry["type_name"]] = entry

all_entries = list(entries_by_name.values())
print(f"  Total unique training entries: {len(all_entries)}")

# ── 7. Build training prompt format (matching main.ts Simulate button) ──

SYSTEM_PROMPT = (
    "You are narrating the l3dg3rr financial pipeline visualization tool. "
    "The user clicked on the \"{type_name}\" object.\n\n"
    "Object description: {description}\n"
    "Layer: {z_layer}\n"
    "Semantic type: {semantic_type}\n\n"
    "Rhai DSL example:\n"
    "```\n"
    "{rhai_dsl}\n"
    "```\n\n"
    "In 3-4 sentences, explain what this object does in the pipeline "
    "and how the Rhai DSL snippet relates to it. Be concise and technical."
)

def build_completion(entry: dict) -> str:
    """Build a reference completion for the training pair.
    This is derived from the description + ontology context.
    """
    d = entry["description"]
    z = entry["z_layer"]
    st = entry["semantic_type"]
    tn = entry["type_name"]
    return (
        f"The {tn} is a {st} object in the {z} layer. "
        f"{d}"
    )

# ── 8. Add ontology context entries ─────────────────────────────────────

def make_ontology_entries() -> list[dict]:
    """Create training entries for ontology concepts (ArtifactKind,
    RelationKind, MCP tool actions) that aren't in viz-manifest."""
    entries = []

    # ArtifactKind
    for ak in artifact_kinds:
        entries.append({
            "type_name": f"ArtifactKind::{ak}",
            "description": f"Ontology entity kind: {ak}. Used in the ledger-core ontology snapshot to identify entity types.",
            "z_layer": "Document",
            "semantic_type": "artifact_kind",
            "rhai_dsl": f"// ArtifactKind::{ak} — canonical ontology entity type",
            "source": "ontology.rs",
        })

    # RelationKind
    for rk in relation_kinds:
        rk_snake = re.sub(r'(?<!^)(?=[A-Z])', '_', rk).lower()
        entries.append({
            "type_name": f"RelationKind::{rk}",
            "description": f"Ontology relation type: {rk_snake}. Connects artifact entities in the ontology graph.",
            "z_layer": "Document",
            "semantic_type": "relation_kind",
            "rhai_dsl": f"// RelationKind::{rk} — canonical ontology edge type",
            "source": "ontology.rs",
        })

    # MCP ontology actions
    for a in ontology_actions:
        entries.append({
            "type_name": f"MCP::{a['action']}",
            "description": f"MCP ontology tool action: {a['action']}. {a['purpose']}",
            "z_layer": "Document",
            "semantic_type": "mcp_action",
            "rhai_dsl": f"// MCP ontology tool: ledgerr_ontology -> {a['action']}",
            "source": "contract.rs",
        })

    return entries

ontology_training_entries = make_ontology_entries()
print(f"  Ontology concept entries: {len(ontology_training_entries)}")

# ── 8.5. Collect live MCP training pairs ────────────────────────────────

mcp_live_pairs = collect_mcp_training_pairs()
print(f"  MCP live server pairs: {len(mcp_live_pairs)}")

# ── 9. Generate all training pairs ──────────────────────────────────────

training_pairs: list[dict] = []

for entry in all_entries:
    prompt = SYSTEM_PROMPT.format(
        type_name=entry["type_name"],
        description=entry["description"],
        z_layer=entry["z_layer"],
        semantic_type=entry["semantic_type"],
        rhai_dsl=entry["rhai_dsl"],
    )
    completion = build_completion(entry)

    prompt_tokens = token_count(prompt)
    completion_tokens = token_count(completion)

    pair = {
        "prompt": prompt,
        "completion": completion,
        "token_counts": {
            "prompt": prompt_tokens,
            "completion": completion_tokens,
            "total": prompt_tokens + completion_tokens,
        },
        "metadata": {
            "type_name": entry["type_name"],
            "z_layer": entry["z_layer"],
            "semantic_type": entry["semantic_type"],
            "source": entry["source"],
            "rhai_dsl_chars": len(entry["rhai_dsl"]),
        },
    }
    training_pairs.append(pair)

# Add ontology concept entries
for entry in ontology_training_entries:
    prompt = SYSTEM_PROMPT.format(
        type_name=entry["type_name"],
        description=entry["description"],
        z_layer=entry["z_layer"],
        semantic_type=entry["semantic_type"],
        rhai_dsl=entry["rhai_dsl"],
    )
    completion = build_completion(entry)

    prompt_tokens = token_count(prompt)
    completion_tokens = token_count(completion)

    pair = {
        "prompt": prompt,
        "completion": completion,
        "token_counts": {
            "prompt": prompt_tokens,
            "completion": completion_tokens,
            "total": prompt_tokens + completion_tokens,
        },
        "metadata": {
            "type_name": entry["type_name"],
            "z_layer": entry["z_layer"],
            "semantic_type": entry["semantic_type"],
            "source": entry["source"],
            "rhai_dsl_chars": len(entry["rhai_dsl"]),
        },
    }
    training_pairs.append(pair)

# Add live MCP server pairs
for pair in mcp_live_pairs:
    training_pairs.append(pair)

print(f"  Total training pairs generated: {len(training_pairs)}")

# ── 10. Split into train/val (80/20) ────────────────────────────────────

random.seed(42)
random.shuffle(training_pairs)

split_idx = int(len(training_pairs) * 0.8)
train_pairs = training_pairs[:split_idx]
val_pairs = training_pairs[split_idx:]

print(f"  Train split: {len(train_pairs)}")
print(f"  Val split:   {len(val_pairs)}")

# ── 11. Write output files ──────────────────────────────────────────────

def write_jsonl(pairs: list[dict], path: Path):
    with open(path, "w") as f:
        for pair in pairs:
            f.write(json.dumps(pair, ensure_ascii=False) + "\n")
    print(f"  Wrote {path} ({len(pairs)} pairs)")

ALL_PATH = TRAIN_DIR / "training-data.jsonl"
TRAIN_PATH = TRAIN_DIR / "train.jsonl"
VAL_PATH = TRAIN_DIR / "val.jsonl"

write_jsonl(training_pairs, ALL_PATH)
write_jsonl(train_pairs, TRAIN_PATH)
write_jsonl(val_pairs, VAL_PATH)

# ── 12. Token count summary ─────────────────────────────────────────────

all_token_counts = [p["token_counts"]["total"] for p in training_pairs]
train_token_counts = [p["token_counts"]["total"] for p in train_pairs]
val_token_counts = [p["token_counts"]["total"] for p in val_pairs]

summary = {
    "total_pairs": len(training_pairs),
    "train_pairs": len(train_pairs),
    "val_pairs": len(val_pairs),
    "total_tokens": sum(all_token_counts),
    "train_tokens": sum(train_token_counts),
    "val_tokens": sum(val_token_counts),
    "mean_tokens_per_pair": round(sum(all_token_counts) / len(all_token_counts), 1) if all_token_counts else 0,
    "min_tokens": min(all_token_counts) if all_token_counts else 0,
    "max_tokens": max(all_token_counts) if all_token_counts else 0,
    "encoding": "cl100k_base",
    "pairs_by_source": {},
    "pairs_by_z_layer": {},
    "pairs_by_semantic_type": {},
}

# Per-source breakdown
for p in training_pairs:
    src = p["metadata"]["source"]
    summary["pairs_by_source"][src] = summary["pairs_by_source"].get(src, 0) + 1

# Per-layer breakdown
for p in training_pairs:
    zl = p["metadata"]["z_layer"]
    summary["pairs_by_z_layer"][zl] = summary["pairs_by_z_layer"].get(zl, 0) + 1

# Per-semantic-type breakdown
for p in training_pairs:
    st = p["metadata"]["semantic_type"]
    summary["pairs_by_semantic_type"][st] = summary["pairs_by_semantic_type"].get(st, 0) + 1

SUMMARY_PATH = TRAIN_DIR / "token-counts.json"
with open(SUMMARY_PATH, "w") as f:
    json.dump(summary, f, indent=2)
print(f"  Wrote {SUMMARY_PATH}")

# ── 13. Print summary ───────────────────────────────────────────────────

print("\n" + "=" * 60)
print("GENERATION COMPLETE")
print("=" * 60)
print(f"  Total pairs:        {summary['total_pairs']}")
print(f"  Train/Val split:    {summary['train_pairs']}/{summary['val_pairs']}")
print(f"  Total tokens:       {summary['total_tokens']:,}")
print(f"  Mean tokens/pair:   {summary['mean_tokens_per_pair']}")
print(f"  Token range:        {summary['min_tokens']} - {summary['max_tokens']}")
print(f"  Encoding:           {summary['encoding']}")
print()
print("  By source:")
for src, cnt in sorted(summary["pairs_by_source"].items()):
    print(f"    {src}: {cnt}")
print()
print("  By Z-layer:")
for zl, cnt in sorted(summary["pairs_by_z_layer"].items()):
    print(f"    {zl}: {cnt}")
print()
print(f"  Output files:")
print(f"    {ALL_PATH}")
print(f"    {TRAIN_PATH}")
print(f"    {VAL_PATH}")
print(f"    {SUMMARY_PATH}")
