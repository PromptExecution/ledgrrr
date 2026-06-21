#!/usr/bin/env python3
"""
export-gguf.py
===============
Merge a fine-tuned Unsloth LoRA adapter into the base Q3_K_M GGUF model
and produce a single .gguf file for the endpoint-server.

Workflow (called by `just ledgrrr fine-tune-export`):
  1. Load the base model in 4-bit via Unsloth
  2. Load the LoRA adapter on top
  3. Merge adapter weights into the full model
  4. Convert the HuggingFace model to GGUF Q3_K_M
  5. Write the .gguf to the canonical model path

Requires:
  - unsloth (pip install unsloth) — for model loading / merging
  - llama.cpp (https://github.com/ggml-org/llama.cpp) — for GGUF conversion
    Specifically: convert_hf_to_gguf.py from the llama.cpp repository.

If llama.cpp conversion tools are not found, the script will complete the
merge step and print clear instructions for running conversion on the
GPU machine that has llama.cpp installed.

Usage:
  uv run python3 scripts/training/export-gguf.py
  uv run python3 scripts/training/export-gguf.py --help
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# ── repo root ────────────────────────────────────────────────────────────────
REPO_ROOT = Path(__file__).resolve().parent.parent.parent

# ── default paths ────────────────────────────────────────────────────────────
DEFAULT_ADAPTER = str(REPO_ROOT / "scripts" / "training" / "lora-out" / "adapter")
DEFAULT_BASE    = "unsloth/Phi-4-mini-reasoning-GGUF"
DEFAULT_OUTPUT  = str(
    REPO_ROOT
    / "models"
    / "unsloth"
    / "Phi-4-mini-reasoning-GGUF"
    / "Phi-4-mini-reasoning-Q3_K_M.gguf"
)
DEFAULT_MERGE_DIR = str(REPO_ROOT / "scripts" / "training" / "lora-out" / "merged-gguf-export")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Merge Unsloth LoRA adapter into base GGUF model and convert to Q3_K_M GGUF.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  uv run python3 scripts/training/export-gguf.py\n"
            "  uv run python3 scripts/training/export-gguf.py --adapter-path /custom/lora --output /custom/model.gguf\n"
            "\n"
            "If llama.cpp convert_hf_to_gguf.py is not on PATH, the script still\n"
            "completes the merge and prints conversion instructions.\n"
        ),
    )
    parser.add_argument(
        "--adapter-path",
        default=DEFAULT_ADAPTER,
        help=f"Path to the saved LoRA adapter (default: {DEFAULT_ADAPTER})",
    )
    parser.add_argument(
        "--base-model",
        default=DEFAULT_BASE,
        help=f"HuggingFace model ID for the base model (default: {DEFAULT_BASE})",
    )
    parser.add_argument(
        "--output",
        default=DEFAULT_OUTPUT,
        help=f"Output .gguf file path (default: {DEFAULT_OUTPUT})",
    )
    parser.add_argument(
        "--merge-dir",
        default=DEFAULT_MERGE_DIR,
        help=f"Temporary directory for merged HuggingFace model (default: {DEFAULT_MERGE_DIR})",
    )
    parser.add_argument(
        "--quantization",
        default="Q3_K_M",
        help="Quantization type for GGUF conversion (default: Q3_K_M)",
    )
    parser.add_argument(
        "--llama-cpp-dir",
        default=None,
        help="Path to llama.cpp repository root (containing convert_hf_to_gguf.py). Overrides PATH search.",
    )
    parser.add_argument(
        "--keep-merge-dir",
        action="store_true",
        help="Do not delete the intermediate merged model directory after conversion.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be done without executing.",
    )
    return parser.parse_args(argv)


# ── helpers ──────────────────────────────────────────────────────────────────

def find_convert_script(llama_cpp_dir: str | None) -> str | None:
    """Locate convert_hf_to_gguf.py from llama.cpp."""
    if llama_cpp_dir:
        candidate = Path(llama_cpp_dir) / "convert_hf_to_gguf.py"
        if candidate.is_file():
            return str(candidate)
        # also try the old convert.py
        candidate_old = Path(llama_cpp_dir) / "convert.py"
        if candidate_old.is_file():
            return str(candidate_old)
        return None

    # Search PATH
    for name in ("convert_hf_to_gguf.py", "convert.py"):
        which = shutil.which(name)
        if which:
            return which

    # Check common locations
    common_dirs = [
        Path.home() / "src" / "llama.cpp",
        Path.home() / "llama.cpp",
        Path.home() / "git" / "llama.cpp",
        Path.home() / "code" / "llama.cpp",
        Path("/opt") / "llama.cpp",
        Path("/usr") / "local" / "src" / "llama.cpp",
    ]
    for d in common_dirs:
        for name in ("convert_hf_to_gguf.py", "convert.py"):
            candidate = d / name
            if candidate.is_file():
                return str(candidate)

    return None


def merge_model(
    base_model: str,
    adapter_path: str,
    merge_dir: str,
    dry_run: bool = False,
) -> None:
    """Load base model + LoRA adapter via Unsloth, merge, and save as HF format."""
    if dry_run:
        print(f"[DRY-RUN] Would merge adapter at: {adapter_path}")
        print(f"[DRY-RUN]   into base model:      {base_model}")
        print(f"[DRY-RUN]   output merged dir:     {merge_dir}")
        return

    try:
        import torch
        from unsloth import FastLanguageModel
    except ImportError as e:
        print(f"\nERROR: Unsloth not installed: {e}", file=sys.stderr)
        print("  Install with: uv pip install unsloth", file=sys.stderr)
        print("  See https://github.com/unslothai/unsloth for GPU requirements", file=sys.stderr)
        sys.exit(1)

    print("=" * 60)
    print("EXPORT: MERGE LORA ADAPTER INTO BASE MODEL")
    print("=" * 60)
    print(f"  Base model:     {base_model}")
    print(f"  Adapter path:   {adapter_path}")
    print(f"  Merge output:   {merge_dir}")
    print()

    adapter_path_p = Path(adapter_path)
    if not adapter_path_p.is_dir():
        print(f"ERROR: Adapter directory not found: {adapter_path_p}", file=sys.stderr)
        print("  Run `just ledgrrr fine-tune` first to produce adapter weights.", file=sys.stderr)
        sys.exit(1)

    # Verify adapter directory contains expected files
    required_files = ["adapter_model.safetensors", "adapter_config.json"]
    missing = [f for f in required_files if not (adapter_path_p / f).is_file()]
    if missing:
        print(
            f"WARNING: Adapter directory missing expected files: {missing}",
            file=sys.stderr,
        )
        print("  The adapter might be incomplete or from a different training run.", file=sys.stderr)
        answer = input("  Continue anyway? [y/N] ").strip().lower()
        if answer not in ("y", "yes"):
            print("Aborted.")
            sys.exit(1)

    print("Step 1: Loading base model in 4-bit...")
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name=base_model,
        max_seq_length=2048,
        dtype=None,          # auto-detect
        load_in_4bit=True,   # 4-bit QLoRA
    )
    print("  Base model loaded.")

    print("Step 2: Loading LoRA adapter...")
    # Load the adapter weights on top of the base model
    # Unsloth's from_pretrained can also load a full model with a pre-trained adapter
    model = FastLanguageModel.from_pretrained(
        model_name=adapter_path,
        max_seq_length=2048,
        dtype=None,
        load_in_4bit=True,
        # Use the existing model as the base
        model=model,
    )
    print("  Adapter loaded.")

    print("Step 3: Merging weights and saving as HuggingFace format (16-bit)...")
    # merge_and_unload() gives us the full model without LoRA layers, in 16-bit
    merged_model = model.merge_and_unload()
    merged_model.save_pretrained(
        merge_dir,
        safe_serialization=True,  # save as safetensors
    )
    tokenizer.save_pretrained(merge_dir)

    # Verify the merged model was saved
    expected_files = [
        "model.safetensors",
        "model.safetensors.index.json",
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
    ]
    saved_files = [f for f in expected_files if (Path(merge_dir) / f).is_file()]
    if saved_files:
        print(f"  Merged model saved to: {merge_dir}")
        print(f"  Files: {', '.join(saved_files)}")
    else:
        print(f"WARNING: No expected model files found in {merge_dir}", file=sys.stderr)
        print("  The merge may have failed silently.", file=sys.stderr)
        print(f"  Directory contents: {list(Path(merge_dir).iterdir())}", file=sys.stderr)

    print()


def convert_to_gguf(
    merge_dir: str,
    output: str,
    quantization: str = "Q3_K_M",
    convert_script: str | None = None,
    dry_run: bool = False,
) -> bool:
    """Convert a HuggingFace model directory to GGUF format using llama.cpp."""
    output_path = Path(output)
    merge_dir_p = Path(merge_dir)

    if dry_run:
        print(f"[DRY-RUN] Would convert {merge_dir_p} -> {output_path}")
        print(f"[DRY-RUN]   quantization: {quantization}")
        print(f"[DRY-RUN]   converter:    {convert_script or 'auto-detect'}")
        return True

    convert_script = convert_script or find_convert_script(None)

    if not convert_script:
        print("llama.cpp conversion tools not found.")
        return False

    if not merge_dir_p.is_dir():
        print(f"ERROR: Merged model directory not found: {merge_dir_p}", file=sys.stderr)
        return False

    output_path.parent.mkdir(parents=True, exist_ok=True)

    # Build the command
    # convert_hf_to_gguf.py takes: model directory, output file, quantization type
    script_name = Path(convert_script).name

    if script_name == "convert_hf_to_gguf.py":
        cmd = [
            sys.executable, convert_script,
            str(merge_dir_p),
            "--outfile", str(output_path),
            "--outtype", quantization,
        ]
    elif script_name == "convert.py":
        # Older convert.py uses different flags
        cmd = [
            sys.executable, convert_script,
            str(merge_dir_p),
            "--outfile", str(output_path),
            "--outtype", quantization,
        ]
    else:
        print(f"ERROR: Unknown conversion script: {convert_script}", file=sys.stderr)
        return False

    print(f"Running: {' '.join(cmd)}")
    print("This may take several minutes and use significant RAM/CPU...")
    print()

    try:
        result = subprocess.run(cmd, check=True, capture_output=True, text=True)
        if result.stdout:
            print(result.stdout)
        if result.stderr:
            print(result.stderr, file=sys.stderr)
        print()
        return True
    except subprocess.CalledProcessError as e:
        print(f"ERROR: GGUF conversion failed (exit code {e.returncode})", file=sys.stderr)
        if e.stdout:
            print(e.stdout, file=sys.stderr)
        if e.stderr:
            print(e.stderr, file=sys.stderr)
        return False
    except FileNotFoundError:
        print(f"ERROR: Python interpreter not found at {sys.executable}", file=sys.stderr)
        return False


def print_conversion_instructions(
    merge_dir: str,
    output: str,
    quantization: str,
    base_model: str,
    adapter_path: str,
) -> None:
    """Print actionable instructions for GGUF conversion on a GPU machine."""
    print()
    print("=" * 60)
    print("GGUF CONVERSION REQUIRED ON GPU MACHINE")
    print("=" * 60)
    print()
    print("llama.cpp conversion tools were not found on this machine.")
    print()
    print("The LoRA adapter has been merged into a HuggingFace model at:")
    print(f"  {merge_dir}")
    print()
    print(f"To convert it to GGUF (Q3_K_M), run this command on a machine")
    print("that has llama.cpp installed (e.g., your Windows GPU host):")
    print()
    print(f"    # On the GPU machine with llama.cpp checked out:")
    print(f"    cd /path/to/llama.cpp")
    print(f"    python3 convert_hf_to_gguf.py \\")
    print(f"        {merge_dir} \\")
    print(f"        --outfile {output} \\")
    print(f"        --outtype {quantization}")
    print()
    print("Or, if using the older convert.py:")
    print()
    print(f"    python3 convert.py \\")
    print(f"        {merge_dir} \\")
    print(f"        --outfile {output} \\")
    print(f"        --outtype {quantization}")
    print()
    print("After conversion, copy the .gguf to the target machine at:")
    print(f"  {output}")
    print()
    print("Then restart the endpoint-server:")
    print("  just ledgrrr llm-rebuild")
    print()


def verify_output(output: str) -> bool:
    """Verify the output GGUF file exists and looks reasonable."""
    out_path = Path(output)
    if not out_path.is_file():
        return False

    size_mb = out_path.stat().st_size / (1024 * 1024)
    print(f"Output GGUF file: {out_path}")
    print(f"  Size: {size_mb:.1f} MB")

    if size_mb < 100:
        print(f"  WARNING: File seems too small ({size_mb:.1f} MB).")
        print(f"    Expected ~1-2 GB for a Q3_K_M model of this size.")
        return False

    return True


# ── main ─────────────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()

    adapter_path = Path(args.adapter_path)
    merge_dir = args.merge_dir
    output_path = Path(args.output)
    base_model = args.base_model
    quantization = args.quantization

    print("=" * 60)
    print("EXPORT: FINE-TUNED MODEL TO GGUF")
    print("=" * 60)
    print(f"  Base model:      {base_model}")
    print(f"  Adapter path:    {adapter_path}")
    print(f"  Merge dir:       {merge_dir}")
    print(f"  Output GGUF:     {output_path}")
    print(f"  Quantization:    {quantization}")
    print(f"  Dry run:         {args.dry_run}")
    print()

    # ── Step 1: Merge ──────────────────────────────────────────────────────
    merge_model(
        base_model=base_model,
        adapter_path=str(adapter_path),
        merge_dir=merge_dir,
        dry_run=args.dry_run,
    )

    if args.dry_run:
        print("[DRY-RUN] Would attempt GGUF conversion next.")
        sys.exit(0)

    # ── Step 2: Convert to GGUF ────────────────────────────────────────────
    convert_script = find_convert_script(args.llama_cpp_dir)
    if convert_script:
        print(f"Found llama.cpp conversion script: {convert_script}")
        print()

    success = convert_to_gguf(
        merge_dir=merge_dir,
        output=str(output_path),
        quantization=quantization,
        convert_script=convert_script,
        dry_run=args.dry_run,
    )

    if success:
        print()
        print("=" * 60)
        print("EXPORT COMPLETE")
        print("=" * 60)
        print(f"  Output: {output_path}")

        if verify_output(str(output_path)):
            print("  Verification: PASSED")
        else:
            print("  Verification: FAILED (see warnings above)")

        print()
        print("Next step: Restart the endpoint-server")
        print("  just ledgrrr llm-rebuild")
        print()

        # Clean up merge dir unless --keep-merge-dir
        if not args.keep_merge_dir:
            merge_path = Path(merge_dir)
            if merge_path.is_dir() and merge_path.exists():
                print(f"Cleaning up intermediate merge directory: {merge_path}")
                shutil.rmtree(merge_path, ignore_errors=True)
                print("  Done.")
    else:
        print_conversion_instructions(
            merge_dir=merge_dir,
            output=str(output_path),
            quantization=quantization,
            base_model=base_model,
            adapter_path=str(adapter_path),
        )


if __name__ == "__main__":
    main()
