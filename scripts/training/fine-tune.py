#!/usr/bin/env python3
"""
fine-tune.py
============
Takes JSONL training data from generate-training-data.py and runs Unsloth
fine-tuning with LoRA on a small base model (Phi-3-mini).

Usage:
  python3 scripts/training/fine-tune.py [--train PATH] [--val PATH] [--model NAME] [--epochs N] [--lr F]

Defaults:
  --train  scripts/training/train/train.jsonl
  --val    scripts/training/train/val.jsonl
  --model  microsoft/Phi-3-mini-4k-instruct
  --epochs 3
  --lr     2e-4
"""

import argparse
import json
import os
import sys
from pathlib import Path

# Ensure working directory is repo root
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
os.chdir(REPO_ROOT)

# ── parse args ──────────────────────────────────────────────────────────

parser = argparse.ArgumentParser(description="Unsloth LoRA fine-tuning for l3dg3rr")
parser.add_argument("--train", default=str(REPO_ROOT / "scripts" / "training" / "train" / "train.jsonl"))
parser.add_argument("--val", default=str(REPO_ROOT / "scripts" / "training" / "train" / "val.jsonl"))
parser.add_argument("--model", default="microsoft/Phi-3-mini-4k-instruct")
parser.add_argument("--epochs", type=int, default=3)
parser.add_argument("--lr", type=float, default=2e-4)
parser.add_argument("--output", default=str(REPO_ROOT / "scripts" / "training" / "lora-out"))
parser.add_argument("--batch-size", type=int, default=2)
parser.add_argument("--grad-accum", type=int, default=4)
parser.add_argument("--max-seq-len", type=int, default=2048)
parser.add_argument("--max-steps", type=int, default=None, help="Max training steps (for dry-run)")
args = parser.parse_args()

TRAIN_PATH = Path(args.train)
VAL_PATH = Path(args.val)
MODEL_NAME = args.model
EPOCHS = args.epochs
LR = args.lr
OUTPUT_DIR = Path(args.output)
BATCH_SIZE = args.batch_size
GRAD_ACCUM = args.grad_accum
MAX_SEQ_LEN = args.max_seq_len
MAX_STEPS = args.max_steps

# ── validate inputs ─────────────────────────────────────────────────────

if not TRAIN_PATH.exists():
    print(f"ERROR: Train file not found: {TRAIN_PATH}", file=sys.stderr)
    print("  Run scripts/training/generate-training-data.py first.", file=sys.stderr)
    sys.exit(1)

if not VAL_PATH.exists():
    print(f"WARNING: Val file not found: {VAL_PATH} — training without validation", file=sys.stderr)
    VAL_PATH = None

OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

# ── load JSONL ──────────────────────────────────────────────────────────

def load_jsonl(path: Path) -> list[dict]:
    pairs = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                pairs.append(json.loads(line))
    return pairs

print("=" * 60)
print("UNSLOTH FINE-TUNING PIPELINE")
print("=" * 60)
print(f"Model:          {MODEL_NAME}")
print(f"Train data:     {TRAIN_PATH}")
print(f"Val data:       {VAL_PATH}")
print(f"Epochs:         {EPOCHS}")
print(f"Learning rate:  {LR}")
print(f"Batch size:     {BATCH_SIZE}")
print(f"Grad accum:     {GRAD_ACCUM}")
print(f"Max seq len:    {MAX_SEQ_LEN}")
print(f"Max steps:      {MAX_STEPS}")
print(f"Output dir:     {OUTPUT_DIR}")
print()

train_pairs = load_jsonl(TRAIN_PATH)
val_pairs = load_jsonl(VAL_PATH) if VAL_PATH and VAL_PATH.exists() else []
print(f"Loaded {len(train_pairs)} train pairs, {len(val_pairs)} val pairs")

# ── import Unsloth ──────────────────────────────────────────────────────

try:
    import torch
    from unsloth import FastLanguageModel, is_bfloat16_supported
    from unsloth.chat_templates import get_chat_template, train_on_responses_only
    from datasets import Dataset
    from transformers import TrainingArguments
    from trl import SFTTrainer
except ImportError as e:
    print(f"\nERROR: Unsloth not installed: {e}", file=sys.stderr)
    print("  Install with: pip install unsloth", file=sys.stderr)
    print("  See https://github.com/unslothai/unsloth for GPU requirements", file=sys.stderr)
    sys.exit(1)

print(f"  torch:       {torch.__version__}")
print(f"  CUDA avail:  {torch.cuda.is_available()}")
if torch.cuda.is_available():
    print(f"  GPU:         {torch.cuda.get_device_name(0)}")
print()

# ── load model with LoRA ────────────────────────────────────────────────

print("Loading model and applying LoRA...")

model, tokenizer = FastLanguageModel.from_pretrained(
    model_name=MODEL_NAME,
    max_seq_length=MAX_SEQ_LEN,
    dtype=None,          # auto-detect
    load_in_4bit=True,   # 4-bit QLoRA
)

model = FastLanguageModel.get_peft_model(
    model,
    r=16,
    target_modules=[
        "q_proj", "k_proj", "v_proj", "o_proj",
        "gate_proj", "up_proj", "down_proj",
    ],
    lora_alpha=16,
    lora_dropout=0.0,
    bias="none",
    use_gradient_checkpointing="unsloth",
    random_state=42,
    use_rslora=False,
    loftq_config=None,
)

print("  LoRA applied (r=16, alpha=16, 4-bit QLoRA)")
print(f"  Parameters: {model.get_nb_trainable_parameters():,}")
print()

# ── template format ─────────────────────────────────────────────────────

def format_pair(pair: dict) -> str:
    """Format a training pair into a chat template string."""
    return pair["prompt"] + " " + pair["completion"]

# ── build datasets ──────────────────────────────────────────────────────

train_texts = [format_pair(p) for p in train_pairs]
train_dataset = Dataset.from_list([{"text": t} for t in train_texts])

if val_pairs:
    val_texts = [format_pair(p) for p in val_pairs]
    val_dataset = Dataset.from_list([{"text": t} for t in val_texts])
else:
    val_dataset = None

# ── training args ───────────────────────────────────────────────────────

training_args = TrainingArguments(
    output_dir=str(OUTPUT_DIR),
    per_device_train_batch_size=BATCH_SIZE,
    gradient_accumulation_steps=GRAD_ACCUM,
    num_train_epochs=EPOCHS,
    max_steps=MAX_STEPS,
    learning_rate=LR,
    warmup_ratio=0.1,
    logging_steps=10,
    save_strategy="epoch",
    evaluation_strategy="epoch" if val_dataset else "no",
    save_total_limit=2,
    load_best_model_at_end=True if val_dataset else False,
    fp16=not is_bfloat16_supported(),
    bf16=is_bfloat16_supported(),
    optim="adamw_8bit",
    weight_decay=0.01,
    lr_scheduler_type="cosine",
    seed=42,
    report_to="none",
)

# ── trainer ──────────────────────────────────────────────────────────────

trainer = SFTTrainer(
    model=model,
    tokenizer=tokenizer,
    train_dataset=train_dataset,
    eval_dataset=val_dataset,
    dataset_text_field="text",
    max_seq_length=MAX_SEQ_LEN,
    dataset_num_proc=2,
    packing=False,
    args=training_args,
)

# ── train ────────────────────────────────────────────────────────────────

print("Starting training...")
print(f"  Steps per epoch: {len(train_dataset) // (BATCH_SIZE * GRAD_ACCUM)}")
print()

trainer_stats = trainer.train()

# ── save ─────────────────────────────────────────────────────────────────

print("\nSaving model...")
model.save_pretrained(str(OUTPUT_DIR / "adapter"))
tokenizer.save_pretrained(str(OUTPUT_DIR / "adapter"))

# Also save merged 16-bit for inference
print("Saving merged 16-bit model for inference...")
model.save_pretrained_merged(
    str(OUTPUT_DIR / "merged-16bit"),
    tokenizer,
    save_method="merged_16bit",
)

print(f"\n{'=' * 60}")
print("TRAINING COMPLETE")
print("=" * 60)
print(f"  Adapter:     {OUTPUT_DIR / 'adapter'}")
print(f"  Merged 16b:  {OUTPUT_DIR / 'merged-16bit'}")
print(f"  Steps:       {trainer_stats.global_step}")
print(f"  Loss:        {trainer_stats.training_loss:.4f}")
print()
print("  To run inference with the fine-tuned model:")
print(f"    python3 -c \"from unsloth import FastLanguageModel; \\")
print(f"      model, tokenizer = FastLanguageModel.from_pretrained( \\")
print(f"        '{OUTPUT_DIR / 'merged-16bit'}'); \\")
print(f"      FastLanguageModel.for_inference(model); \\")
print(f"      print(tokenizer.decode(model.generate( \\")
print(f"        tokenizer('Your prompt', return_tensors='pt').input_ids, \\")
print(f"        max_new_tokens=256)[0]))\"")
print()
