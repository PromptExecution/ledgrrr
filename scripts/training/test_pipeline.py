#!/usr/bin/env python3
"""
test_pipeline.py — Comprehensive pytest test suite for the l3dg3rr training pipeline.

Covers generate-training-data.py (7 tests), fine-tune.py (5 tests), export-gguf.py (5 tests).

All tests are deterministic, fast (<30s total), and mock external services.
"""

from __future__ import annotations

import json
import os
import sys
import textwrap
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

SCRIPTS_DIR = Path(__file__).resolve().parent


# ── Helpers ──────────────────────────────────────────────────────────────────


def _exec_module(
    source_path: Path,
    namespace: dict | None = None,
    cli_args: list[str] | None = None,
) -> dict:
    """Compile and execute a Python module with a custom namespace.

    *namespace* keys are injected BEFORE module-level code runs.
    *cli_args* (if given) temporarily replaces sys.argv so argparse works.
    """
    import copy

    # Save/restore sys.argv so argparse in module-level code doesn't
    # accidentally read pytest's arguments.
    saved_argv = copy.deepcopy(sys.argv)
    if cli_args is not None:
        sys.argv = cli_args

    ns = {
        "__name__": source_path.stem,
        "__builtins__": __builtins__,
    }
    if namespace:
        ns.update(namespace)
    # __file__ must be set AFTER namespace so it can be overridden
    ns.setdefault("__file__", str(source_path))

    source = source_path.read_text()
    code = compile(source, str(source_path), "exec")
    try:
        exec(code, ns)
    finally:
        sys.argv = saved_argv
    return ns


# ── Fixtures ─────────────────────────────────────────────────────────────────


@pytest.fixture
def tmp_source_tree(tmp_path: Path) -> Path:
    """Create a minimal repo-like directory tree with deterministic mock data."""
    root = tmp_path

    # viz-manifest.json
    ui_docs = root / "ui" / "docs" / "public"
    ui_docs.mkdir(parents=True)
    manifest = {
        "version": "1.9.0",
        "objects": [
            {
                "type_name": "PipelineState<Ingested>",
                "spec": {
                    "semantic_type": "pipeline",
                    "z_layer": "Pipeline",
                    "description": "Initial ingestion pipeline state",
                    "rhai_dsl": {
                        "source": textwrap.dedent("""\
                            let tx = ingest(pdf_path);
                            check_constraints(tx);
                            if ok { route_to_validation(tx); }
                        """),
                    },
                },
            },
            {
                "type_name": "FlaggedIssue",
                "spec": {
                    "semantic_type": "flag",
                    "z_layer": "Constraint",
                    "description": "Flagged issue for review",
                    "rhai_dsl": {
                        "source": textwrap.dedent("""\
                            fn is_suspicious(tx) {
                                tx.amount > 10000
                            }
                        """),
                    },
                },
            },
            {
                "type_name": "NoDslEntry",
                "spec": {
                    "semantic_type": "pipeline",
                    "z_layer": "Pipeline",
                    "description": "An entry without rhai_dsl",
                    "rhai_dsl": {},
                },
            },
        ],
    }
    (ui_docs / "viz-manifest.json").write_text(json.dumps(manifest, indent=2))

    # domain.kerm
    types_dir = root / "types"
    types_dir.mkdir()
    kerm_content = textwrap.dedent("""\
        # types/domain.kerm
        [[type]]
        id = "iso::HasVisualization"
        label = "HasVisualization"
        kind = "abstract_trait"

        [[type]]
        id = "iso::RhaiDsl"
        label = "RhaiDsl"
        kind = "dsl_contract"
        rhai_dsl = '''
        fn describe_visualization(spec) {
            spec.layers
        }
        '''

        [[type]]
        id = "no_rhai_type"
        label = "NoRhai"
        kind = "simple"
    """)
    (types_dir / "domain.kerm").write_text(kerm_content)

    # ontology.rs
    ledger_core = root / "crates" / "ledger-core" / "src"
    ledger_core.mkdir(parents=True)
    ontology_content = textwrap.dedent("""\
        pub enum ArtifactKind {
            Transaction,
            Classification,
            Workbook,
            Proposal,
        }
        pub enum RelationKind {
            Produces,
            Validates,
            Classifies,
        }
    """)
    (ledger_core / "ontology.rs").write_text(ontology_content)

    # contract.rs
    mcp_src = root / "crates" / "ledgerr-mcp" / "src"
    mcp_src.mkdir(parents=True)
    contract_content = textwrap.dedent("""\
        pub const ONTOLOGY_TOOL: Tool = Tool {
            name: "ledgerr_ontology",
            actions: &["export_snapshot", "list_snapshots"],
            purpose: "ontology query/export/write operations",
        };
    """)
    (mcp_src / "contract.rs").write_text(contract_content)

    # Create train dir
    train_dir = root / "scripts" / "training" / "train"
    train_dir.mkdir(parents=True)

    return root


@pytest.fixture
def run_generator(tmp_source_tree: Path) -> callable:
    """Return a function that runs generate-training-data.py against tmp_source_tree.

    The returned function takes an optional train_dir override and returns
    the module namespace after execution.
    """

    def _run(train_override: Path | None = None) -> dict:
        gen_path = SCRIPTS_DIR / "generate-training-data.py"
        train_dir = train_override or (tmp_source_tree / "scripts" / "training" / "train")

        # Fake __file__ inside the temp tree so REPO_ROOT resolves to
        # tmp_source_tree instead of the real repo.
        fake_file = str(tmp_source_tree / "scripts" / "training" / "generate-training-data.py")

        # Build a namespace that overrides module-level constants.
        # REPO_ROOT is set here BUT the module also computes it at load-time
        # via `REPO_ROOT = Path(__file__).resolve().parent.parent.parent`.
        # By setting __file__ to a path inside the temp tree, the module's
        # own computation will land on tmp_source_tree.
        ns = {
            "__file__": fake_file,
            "REPO_ROOT": tmp_source_tree,
            "MANIFEST_PATH": tmp_source_tree / "ui" / "docs" / "public" / "viz-manifest.json",
            "KERM_PATH": tmp_source_tree / "types" / "domain.kerm",
            "ONTOLOGY_RS_PATH": tmp_source_tree / "crates" / "ledger-core" / "src" / "ontology.rs",
            "CONTRACT_RS_PATH": tmp_source_tree / "crates" / "ledgerr-mcp" / "src" / "contract.rs",
            "SCRIPTS_DIR": tmp_source_tree / "scripts" / "training",
            "TRAIN_DIR": train_dir,
            "ALL_PATH": train_dir / "training-data.jsonl",
            "TRAIN_PATH": train_dir / "train.jsonl",
            "VAL_PATH": train_dir / "val.jsonl",
            "SUMMARY_PATH": train_dir / "token-counts.json",
            "MCP_SERVER_URL": "http://localhost:1",  # will be unreachable
        }
        # Mock MCP calls to be no-ops
        extra = {
            "call_mcp_tool": lambda tool_name=None, params=None: None,
            "collect_mcp_training_pairs": lambda: [],
        }
        ns.update(extra)

        return _exec_module(gen_path, ns)

    return _run


# ═══════════════════════════════════════════════════════════════════════════════
# Tests for generate-training-data.py  (7 tests)
# ═══════════════════════════════════════════════════════════════════════════════


class TestGenerate:
    """generate-training-data.py tests."""

    def test_output_format(self, run_generator, tmp_source_tree):
        """Every line in output JSONL must be valid JSON with prompt+completion."""
        run_generator()

        train_dir = tmp_source_tree / "scripts" / "training" / "train"
        for fname in ("training-data.jsonl", "train.jsonl", "val.jsonl"):
            path = train_dir / fname
            assert path.is_file(), f"Missing {fname}"
            lines = [l.strip() for l in path.read_text().splitlines() if l.strip()]
            assert len(lines) > 0, f"Empty file: {fname}"
            for i, line in enumerate(lines):
                obj = json.loads(line)
                assert "prompt" in obj, f"{fname}[{i}] missing prompt"
                assert "completion" in obj, f"{fname}[{i}] missing completion"
                assert isinstance(obj["prompt"], str) and len(obj["prompt"]) > 0
                assert isinstance(obj["completion"], str) and len(obj["completion"]) > 0

    def test_token_counts(self, run_generator, tmp_source_tree):
        """Token count fields exist and are within expected range."""
        run_generator()

        train_dir = tmp_source_tree / "scripts" / "training" / "train"
        summary = json.loads((train_dir / "token-counts.json").read_text())

        assert summary["total_pairs"] > 0
        assert summary["train_pairs"] > 0
        assert summary["val_pairs"] > 0
        assert summary["total_pairs"] == summary["train_pairs"] + summary["val_pairs"]
        assert summary["mean_tokens_per_pair"] > 0
        assert summary["min_tokens"] > 0
        assert summary["min_tokens"] <= summary["max_tokens"]
        assert summary["encoding"] == "cl100k_base"
        assert summary["total_tokens"] > 0

        # Per-pair token count validation
        with open(train_dir / "training-data.jsonl") as f:
            for i, line in enumerate(f):
                obj = json.loads(line)
                tc = obj["token_counts"]
                assert tc["prompt"] > 0, f"[{i}] zero prompt tokens"
                assert tc["completion"] > 0, f"[{i}] zero completion tokens"
                assert tc["total"] == tc["prompt"] + tc["completion"]
                assert tc["total"] < 500, f"[{i}] too many tokens: {tc['total']}"

    def test_split_integrity(self, run_generator, tmp_source_tree):
        """train.jsonl + val.jsonl = training-data.jsonl, no overlap."""
        run_generator()

        train_dir = tmp_source_tree / "scripts" / "training" / "train"

        def load_set(path):
            return {json.dumps(json.loads(l), sort_keys=True)
                    for l in path.read_text().splitlines() if l.strip()}

        all_set = load_set(train_dir / "training-data.jsonl")
        train_set = load_set(train_dir / "train.jsonl")
        val_set = load_set(train_dir / "val.jsonl")

        assert len(all_set) == len(train_set) + len(val_set)
        assert train_set | val_set == all_set
        assert train_set & val_set == set()

    def test_rhai_dsl_present(self, run_generator, tmp_source_tree):
        """Each prompt from viz-manifest or domain.kerm contains a code fence."""
        run_generator()

        train_dir = tmp_source_tree / "scripts" / "training" / "train"
        with open(train_dir / "training-data.jsonl") as f:
            for i, line in enumerate(f):
                obj = json.loads(line)
                prompt = obj["prompt"]
                meta = obj.get("metadata", {})
                if meta.get("source") in ("viz-manifest.json", "domain.kerm"):
                    assert "```" in prompt, \
                        f"[{i}] {meta.get('type_name')} missing code fence"
                assert "rhai_dsl_chars" in meta, f"[{i}] missing rhai_dsl_chars"

    def test_consistent(self, run_generator, tmp_source_tree):
        """Running twice produces identical output (deterministic)."""
        # Use separate source trees instead of out1/out2 overrides,
        # because the module recalculates TRAIN_DIR from __file__ internally.
        import shutil

        tree2 = tmp_source_tree.parent / f"{tmp_source_tree.name}_clone"
        shutil.copytree(tmp_source_tree, tree2)
        train_dir1 = tmp_source_tree / "scripts" / "training" / "train"
        train_dir2 = tree2 / "scripts" / "training" / "train"

        run_generator()  # writes to train_dir1
        # For the second run, use a lambda that sets __file__ differently
        # by modifying the fixture's tmp_source_tree to point to tree2
        gen_path = SCRIPTS_DIR / "generate-training-data.py"

        fake_file2 = str(tree2 / "scripts" / "training" / "generate-training-data.py")
        ns2 = {
            "__file__": fake_file2,
            "REPO_ROOT": tree2,
            "MANIFEST_PATH": tree2 / "ui" / "docs" / "public" / "viz-manifest.json",
            "KERM_PATH": tree2 / "types" / "domain.kerm",
            "ONTOLOGY_RS_PATH": tree2 / "crates" / "ledger-core" / "src" / "ontology.rs",
            "CONTRACT_RS_PATH": tree2 / "crates" / "ledgerr-mcp" / "src" / "contract.rs",
            "SCRIPTS_DIR": tree2 / "scripts" / "training",
            "TRAIN_DIR": train_dir2,
            "ALL_PATH": train_dir2 / "training-data.jsonl",
            "TRAIN_PATH": train_dir2 / "train.jsonl",
            "VAL_PATH": train_dir2 / "val.jsonl",
            "SUMMARY_PATH": train_dir2 / "token-counts.json",
            "MCP_SERVER_URL": "http://localhost:1",
            "call_mcp_tool": lambda tool_name=None, params=None: None,
            "collect_mcp_training_pairs": lambda: [],
        }
        _exec_module(gen_path, ns2)

        for fname in ("training-data.jsonl", "train.jsonl", "val.jsonl", "token-counts.json"):
            assert (train_dir1 / fname).read_text() == (train_dir2 / fname).read_text(), \
                f"{fname} differs between runs"

    def test_metadata_fields(self, run_generator, tmp_source_tree):
        """Every training pair has complete metadata."""
        run_generator()

        train_dir = tmp_source_tree / "scripts" / "training" / "train"
        with open(train_dir / "training-data.jsonl") as f:
            for i, line in enumerate(f):
                obj = json.loads(line)
                meta = obj.get("metadata", {})
                for key in ("type_name", "z_layer", "semantic_type", "source", "rhai_dsl_chars"):
                    assert key in meta, f"[{i}] missing metadata.{key}"
                assert isinstance(meta["rhai_dsl_chars"], int)

    def test_by_source_breakdown(self, run_generator, tmp_source_tree):
        """token-counts.json has by-source, by-layer, by-semantic-type breakdowns."""
        run_generator()

        train_dir = tmp_source_tree / "scripts" / "training" / "train"
        summary = json.loads((train_dir / "token-counts.json").read_text())

        assert "pairs_by_source" in summary
        assert "pairs_by_z_layer" in summary
        assert "pairs_by_semantic_type" in summary

        source_total = sum(summary["pairs_by_source"].values())
        assert source_total == summary["total_pairs"]


# ═══════════════════════════════════════════════════════════════════════════════
# Tests for fine-tune.py  (5 tests)
# ═══════════════════════════════════════════════════════════════════════════════


def _exec_fine_tune(
    namespace: dict | None = None,
    cli_args: list[str] | None = None,
) -> dict:
    """Compile and run fine-tune.py with mocked torch/unsloth/datasets/etc."""
    ft_path = SCRIPTS_DIR / "fine-tune.py"

    # Default CLI args — just the script name, all args use defaults
    if cli_args is None:
        cli_args = ["fine-tune.py"]

    # Build a chain of mocks that can survive the full module-level execution
    # (data loading, model loading, LoRA setup, training setup).
    mock_torch = MagicMock()
    mock_torch.cuda.is_available.return_value = False
    mock_torch.__version__ = "2.0.0"

    mock_model = MagicMock(name="model")
    mock_model.get_nb_trainable_parameters.return_value = 42_000_000
    mock_tokenizer = MagicMock(name="tokenizer")

    mock_unsloth = MagicMock(name="unsloth")
    mock_unsloth.is_bfloat16_supported.return_value = False
    # FastLanguageModel.from_pretrained returns (model, tokenizer)
    mock_unsloth.FastLanguageModel.from_pretrained.return_value = (mock_model, mock_tokenizer)
    # get_peft_model returns the model
    mock_unsloth.FastLanguageModel.get_peft_model.return_value = mock_model

    mock_dataset = MagicMock(name="Dataset")
    mock_dataset.__len__.return_value = 5

    mock_datasets = MagicMock(name="datasets")
    mock_datasets.Dataset.from_list.return_value = mock_dataset

    mock_training_args = MagicMock(name="TrainingArguments")

    mock_transformers = MagicMock(name="transformers")
    mock_transformers.TrainingArguments.return_value = mock_training_args

    mock_trainer = MagicMock(name="SFTTrainer")
    mock_trainer.train.return_value = MagicMock()
    mock_trainer.train.return_value.global_step = 10
    mock_trainer.train.return_value.training_loss = 0.5

    mock_sft_trainer_module = MagicMock(name="trl")
    mock_sft_trainer_module.SFTTrainer.return_value = mock_trainer

    # Register mocks in sys.modules so module-level 'import torch' etc. work
    _modules = {}
    for mod_name, mod_val in [
        ("torch", mock_torch),
        ("unsloth", mock_unsloth),
        ("unsloth.chat_templates", MagicMock()),
        ("datasets", mock_datasets),
        ("transformers", mock_transformers),
        ("trl", mock_sft_trainer_module),
    ]:
        _modules[mod_name] = sys.modules.get(mod_name)
        sys.modules[mod_name] = mod_val

    overrides = {
        "torch": mock_torch,
        "unsloth": mock_unsloth,
        "FastLanguageModel": mock_unsloth.FastLanguageModel,
        "is_bfloat16_supported": mock_unsloth.is_bfloat16_supported,
        "get_chat_template": MagicMock(),
        "train_on_responses_only": MagicMock(),
        "datasets": mock_datasets,
        "Dataset": mock_datasets.Dataset,
        "transformers": mock_transformers,
        "TrainingArguments": mock_transformers.TrainingArguments,
        "trl": mock_sft_trainer_module,
        "SFTTrainer": mock_sft_trainer_module.SFTTrainer,
    }
    if namespace:
        overrides.update(namespace)

    try:
        return _exec_module(ft_path, overrides, cli_args=cli_args)
    finally:
        # Restore sys.modules
        for mod_name, prev in _modules.items():
            if prev is None:
                sys.modules.pop(mod_name, None)
            else:
                sys.modules[mod_name] = prev


class TestFineTune:
    """fine-tune.py tests."""

    def test_argparse_help(self):
        """--help exits 0 and shows expected flags."""
        import subprocess
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "fine-tune.py"), "--help"],
            capture_output=True, text=True,
        )
        assert r.returncode == 0
        assert "usage:" in r.stdout.lower()
        for flag in ("--train", "--val", "--model", "--epochs", "--lr",
                     "--output", "--batch-size", "--grad-accum", "--max-steps"):
            assert flag in r.stdout, f"Flag {flag} missing from --help"

    def test_load_jsonl(self, tmp_path):
        """load_jsonl reads valid JSONL and skips blank lines."""
        # Create train files at the default path so module-level validation passes
        train_file = tmp_path / "scripts" / "training" / "train" / "train.jsonl"
        val_file = tmp_path / "scripts" / "training" / "train" / "val.jsonl"
        train_file.parent.mkdir(parents=True, exist_ok=True)
        train_file.write_text('{"prompt":"p","completion":"c"}\n')
        val_file.write_text('{"prompt":"p","completion":"c"}\n')

        cli = ["fine-tune.py", "--train", str(train_file), "--val", str(val_file)]
        ns = _exec_fine_tune(cli_args=cli)

        # Now test load_jsonl
        f = tmp_path / "data.jsonl"
        f.write_text(
            '{"prompt": "a", "completion": "b"}\n'
            '\n'
            '{"prompt": "c", "completion": "d"}\n'
        )
        result = ns["load_jsonl"](f)
        assert len(result) == 2
        assert result[0]["prompt"] == "a"
        assert result[1]["prompt"] == "c"

    def test_load_jsonl_empty(self, tmp_path):
        """Loading an empty JSONL returns empty list."""
        train_file = tmp_path / "scripts" / "training" / "train" / "train.jsonl"
        val_file = tmp_path / "scripts" / "training" / "train" / "val.jsonl"
        train_file.parent.mkdir(parents=True, exist_ok=True)
        train_file.write_text('{"prompt":"p","completion":"c"}\n')
        val_file.write_text('{"prompt":"p","completion":"c"}\n')

        ns = _exec_fine_tune(
            cli_args=["fine-tune.py", "--train", str(train_file), "--val", str(val_file)]
        )

        f = tmp_path / "empty.jsonl"
        f.write_text("")
        assert ns["load_jsonl"](f) == []

    def test_max_steps_flag(self, tmp_path):
        """--max-steps flag is accepted and parsed correctly."""
        train_file = tmp_path / "scripts" / "training" / "train" / "train.jsonl"
        val_file = tmp_path / "scripts" / "training" / "train" / "val.jsonl"
        train_file.parent.mkdir(parents=True, exist_ok=True)
        train_file.write_text('{"prompt":"p","completion":"c"}\n')
        val_file.write_text('{"prompt":"p","completion":"c"}\n')

        ns = _exec_fine_tune(
            namespace={"__file__": str(tmp_path / "scripts" / "training" / "fine-tune.py"),
                       "REPO_ROOT": tmp_path},
            cli_args=["fine-tune.py"],
        )
        # The module parsed its args at load time; verify the parser accepts --max-steps
        parser = ns["parser"]
        args = parser.parse_args(["--max-steps", "10"])
        assert args.max_steps == 10
        args = parser.parse_args([])
        assert args.max_steps is None

    def test_default_paths(self, tmp_path):
        """Default --train and --val paths resolve correctly."""
        train_file = tmp_path / "scripts" / "training" / "train" / "train.jsonl"
        val_file = tmp_path / "scripts" / "training" / "train" / "val.jsonl"
        train_file.parent.mkdir(parents=True, exist_ok=True)
        train_file.write_text('{"prompt":"p","completion":"c"}\n')
        val_file.write_text('{"prompt":"p","completion":"c"}\n')

        # __file__ must be under tmp_path/scripts/training/ so that
        # Path(__file__).resolve().parent.parent.parent == tmp_path
        ns = _exec_fine_tune(
            namespace={
                "__file__": str(tmp_path / "scripts" / "training" / "fine-tune.py"),
                "REPO_ROOT": tmp_path,
            },
            cli_args=["fine-tune.py"],
        )

        assert ns["TRAIN_PATH"].name == "train.jsonl"
        assert ns["VAL_PATH"].name == "val.jsonl"


# ═══════════════════════════════════════════════════════════════════════════════
# Tests for export-gguf.py  (5 tests)
# ═══════════════════════════════════════════════════════════════════════════════


def _exec_export_gguf(namespace: dict | None = None) -> dict:
    """Compile and run export-gguf.py with a custom namespace."""
    eg_path = SCRIPTS_DIR / "export-gguf.py"
    overrides = {}
    if namespace:
        overrides.update(namespace)
    return _exec_module(eg_path, overrides)


class TestExportGguf:
    """export-gguf.py tests."""

    def test_argparse_help(self):
        """--help exits 0 and shows expected flags."""
        import subprocess
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "export-gguf.py"), "--help"],
            capture_output=True, text=True,
        )
        assert r.returncode == 0
        assert "usage:" in r.stdout.lower()
        for flag in ("--adapter-path", "--base-model", "--output",
                     "--merge-dir", "--quantization", "--dry-run", "--keep-merge-dir"):
            assert flag in r.stdout, f"Flag {flag} missing from --help"

    def test_dry_run(self):
        """--dry-run prints commands without executing anything."""
        ns = _exec_export_gguf()
        # Run main with --dry-run — should exit 0 after printing
        with patch.object(sys, "argv", ["export-gguf.py", "--dry-run"]):
            with pytest.raises(SystemExit) as exc:
                ns["main"]()
            assert exc.value.code == 0

    def test_output_path(self):
        """--output flag resolves correctly; default path is under models/."""
        ns = _exec_export_gguf()

        # Custom path
        args = ns["parse_args"](["--output", "/tmp/custom-model.gguf"])
        assert args.output == "/tmp/custom-model.gguf"

        # Default path
        args = ns["parse_args"]([])
        default = Path(args.output)
        assert "models" in default.parts
        assert default.suffix == ".gguf"

    def test_parse_args_all_flags(self):
        """All CLI flags parse correctly."""
        ns = _exec_export_gguf()
        args = ns["parse_args"]([
            "--adapter-path", "/tmp/adapter",
            "--base-model", "test/model",
            "--output", "/tmp/out.gguf",
            "--merge-dir", "/tmp/merge",
            "--quantization", "Q4_K_M",
            "--llama-cpp-dir", "/tmp/llama.cpp",
            "--keep-merge-dir",
            "--dry-run",
        ])
        assert args.adapter_path == "/tmp/adapter"
        assert args.base_model == "test/model"
        assert args.output == "/tmp/out.gguf"
        assert args.merge_dir == "/tmp/merge"
        assert args.quantization == "Q4_K_M"
        assert args.llama_cpp_dir == "/tmp/llama.cpp"
        assert args.keep_merge_dir is True
        assert args.dry_run is True

    def test_find_convert_script(self, tmp_path):
        """find_convert_script returns valid path or None."""
        ns = _exec_export_gguf()

        # No dir specified — should return None or find something on PATH
        result = ns["find_convert_script"](None)
        assert result is None or result.endswith(".py")

        # Valid llama.cpp dir with convert_hf_to_gguf.py
        script = tmp_path / "convert_hf_to_gguf.py"
        script.write_text("#!/usr/bin/env python3\nprint('ok')\n")
        result = ns["find_convert_script"](str(tmp_path))
        assert result == str(script)

        # Old-style convert.py
        (tmp_path / "convert.py").write_text("#!/usr/bin/env python3\n")
        result = ns["find_convert_script"](str(tmp_path))
        # Should prefer convert_hf_to_gguf.py over convert.py
        assert result == str(script)

    def test_merge_model_dry_run(self, capsys):
        """merge_model() with dry_run=True prints and returns."""
        ns = _exec_export_gguf()
        ns["merge_model"](
            base_model="test/model",
            adapter_path="/fake/adapter",
            merge_dir="/fake/merge",
            dry_run=True,
        )
        captured = capsys.readouterr()
        assert "[DRY-RUN]" in captured.out
        assert "Would merge adapter" in captured.out
