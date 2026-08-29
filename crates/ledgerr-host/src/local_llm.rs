//! # Local LLM inference — candle backend (reference / correctness path)
//!
//! **Who this module is for.**  This file implements in-process LLM inference using
//! HuggingFace's `candle` library — a pure-Rust tensor computation framework.  It is
//! written to be auditable by governance committees who are not machine-learning
//! specialists.  Every non-obvious decision is explained in plain English below.
//!
//! For the higher-performance alternative (used in development and interactive
//! classification work), see `local_llm_mistral.rs`.  The two backends implement
//! the same `AgentRuntime` trait; the module-level documentation in
//! `local_llm_mistral.rs` contains a full comparison table and shared background on
//! the GGUF file format, quantization, and the Phi-4 Mini model.
//!
//! ---
//!
//! ## Role of this backend
//!
//! The candle backend is the **reference implementation**: slower than mistralrs but
//! simpler, entirely pure Rust, and with no C++ native dependencies.  It serves two
//! purposes:
//!
//! 1. **Smoke-test validation** — CI can run `just test-phi4` (no GPU, no heavy
//!    toolchain) to confirm that the GGUF file loads, that the in-memory patches are
//!    correct, and that the model produces non-empty output.
//!
//! 2. **Differential testing** — if candle and mistralrs both produce consistent
//!    output on the same prompt, it is strong evidence that the quantized weights and
//!    inference paths are behaving correctly.  A divergence narrows the root cause.
//!
//! ---
//!
//! ## GGUF patching — in-memory approach
//!
//! Unlike mistralrs, candle exposes its GGUF loading as a library function that this
//! code calls directly.  This means we can intercept the parsed `Content` struct —
//! which holds the decoded metadata and tensor-info table — and modify it in memory
//! *before* any weight data is read from disk.  No sidecar file is written; nothing
//! is persisted to disk.
//!
//! Two patches are applied (see `patch_tied_output_weight` and
//! `patch_rope_dim_to_head_dim` below, and the module-level docs in
//! `local_llm_mistral.rs` for the full rationale):
//!
//! * **Tied embeddings** — a `TensorInfo` alias entry for `output.weight` pointing at
//!   the same file-offset and dtype as `token_embd.weight` is inserted into the
//!   in-memory tensor-info table.  No bytes are copied.
//!
//! * **Partial RoPE** — the `phi3.rope.dimension_count` metadata value is overwritten
//!   from 96 to 128 (= `head_dim`) so candle builds a full-size positional-encoding
//!   frequency table.  The 32 extra dimensions receive slightly extended frequencies
//!   rather than no rotation — an approximation acceptable for smoke-test use.
//!
//! ---
//!
//! ## Tokenizer
//!
//! The tokenizer converts human-readable text into the integer "token" IDs the model
//! processes, and converts generated IDs back into text.  The vocabulary file
//! (`tokenizer.json`) is downloaded once from HuggingFace Hub
//! (`microsoft/Phi-4-mini-reasoning`) and stored in the local HuggingFace cache
//! (`~/.cache/huggingface/hub/`).  All subsequent calls use the cached copy; no
//! network access occurs during inference.
//!
//! ---
//!
//! ## Parameters and tuneable settings
//!
//! | Setting | Default | How to change | Effect |
//! |---|---|---|---|
//! | `max_tokens` | 256 | `runtime.with_max_tokens(n)` | Hard ceiling on generated token count.  One token ≈ ¾ of an English word.  Lower values finish faster; raise to 1024+ for long rationale outputs. |
//! | `tokenizer_repo` | `"microsoft/Phi-4-mini-reasoning"` | `runtime.with_tokenizer_repo(repo)` | HuggingFace repo for the tokenizer vocabulary.  Change only when switching models. |
//!
//! Enabled by Cargo feature: `local-llm`
//! Run smoke test: `just test-phi4`

use std::path::{Path, PathBuf};

use candle_core::quantized::gguf_file::TensorInfo;
use candle_core::{quantized::gguf_file, Device, Tensor};
use candle_transformers::{
    generation::{LogitsProcessor, Sampling},
    models::quantized_phi3::ModelWeights,
};
use tokenizers::Tokenizer;

use crate::agent_runtime::{
    AgentRuntime, AgentRuntimeError, ModelRequest, ModelResponse, ModelRole,
};

/// Phi-4 Mini chat template tokens.
const CHAT_START_SYS: &str = "<|system|>\n";
const CHAT_START_USER: &str = "<|user|>\n";
const CHAT_START_ASST: &str = "<|assistant|>\n";
const CHAT_END: &str = "<|end|>\n";

/// HuggingFace repo for the phi-4 mini tokenizer.
const DEFAULT_TOKENIZER_REPO: &str = "microsoft/Phi-4-mini-reasoning";

#[derive(Debug)]
pub struct LocalCandelRuntime {
    model_path: PathBuf,
    tokenizer_repo: String,
    max_tokens: usize,
}

impl LocalCandelRuntime {
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self, AgentRuntimeError> {
        let path = model_path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(AgentRuntimeError::LocalLlm(format!(
                "model file not found: {}",
                path.display()
            )));
        }
        Ok(Self {
            model_path: path,
            tokenizer_repo: DEFAULT_TOKENIZER_REPO.to_string(),
            max_tokens: 256,
        })
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_tokenizer_repo(mut self, repo: impl Into<String>) -> Self {
        self.tokenizer_repo = repo.into();
        self
    }

    fn load_tokenizer(&self) -> Result<Tokenizer, AgentRuntimeError> {
        let api = hf_hub::api::sync::Api::new()
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("hf-hub init failed: {e}")))?;
        let path = api
            .model(self.tokenizer_repo.clone())
            .get("tokenizer.json")
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("tokenizer fetch failed: {e}")))?;
        Tokenizer::from_file(path)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("tokenizer load failed: {e}")))
    }

    fn format_prompt(&self, request: &ModelRequest) -> String {
        let mut buf = String::new();

        if let Some(sys) = request.system_prompt.as_deref().map(str::trim) {
            if !sys.is_empty() {
                buf.push_str(CHAT_START_SYS);
                buf.push_str(sys);
                buf.push_str(CHAT_END);
            }
        }

        for turn in &request.history {
            let (start, content) = match turn.role {
                ModelRole::System => (CHAT_START_SYS, turn.content.trim()),
                ModelRole::User => (CHAT_START_USER, turn.content.trim()),
                ModelRole::Assistant => (CHAT_START_ASST, turn.content.trim()),
                // This candle backend has no tool-call/tool-result prompt
                // template — fold a tool turn back in as user-visible text
                // rather than failing to compile. Unreachable in practice via
                // the normal chat path (see the matching comment in
                // `local_llm_mistral.rs::build_messages`).
                ModelRole::Tool => (CHAT_START_USER, turn.content.trim()),
            };
            buf.push_str(start);
            buf.push_str(content);
            buf.push_str(CHAT_END);
        }

        buf.push_str(CHAT_START_USER);
        buf.push_str(request.user_message.trim());
        buf.push_str(CHAT_END);
        buf.push_str(CHAT_START_ASST);
        buf
    }

    fn run_inference(
        &self,
        prompt: &str,
        tokenizer: &Tokenizer,
    ) -> Result<String, AgentRuntimeError> {
        let device = Device::Cpu;

        let mut file = std::fs::File::open(&self.model_path)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("open model file: {e}")))?;
        let mut content = gguf_file::Content::read(&mut file)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("read gguf: {e}")))?;

        // Phi-4 mini uses tied embeddings: output.weight is absent in the GGUF and
        // must alias token_embd.weight so quantized_phi3::from_gguf can find it.
        patch_tied_output_weight(&mut content);

        // Phi-4 mini has partial RoPE: rope_dim=96 but head_dim=128.
        // candle's quantized_phi3 applies RoPE to the full head_dim and fails when
        // rope_dim != head_dim.  For this smoke test we extend the declared rope_dim
        // to equal head_dim so the table shapes match; the extra 32 dims get slightly
        // extended frequencies (not identity), which is acceptable for correctness
        // checking but not production inference.
        patch_rope_dim_to_head_dim(&mut content);

        let mut model = ModelWeights::from_gguf(false, content, &mut file, &device)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("load weights: {e}")))?;

        let encoded = tokenizer
            .encode(prompt, true)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("encode prompt: {e}")))?;
        let input_ids: Vec<u32> = encoded.get_ids().to_vec();
        let n_prompt = input_ids.len();

        let vocab = tokenizer.get_vocab(true);
        let eos_end = vocab.get("<|end|>").copied();
        let eos_text = vocab.get("<|endoftext|>").copied();

        let mut logits_proc = LogitsProcessor::from_sampling(42, Sampling::ArgMax);

        // Prefill: process all prompt tokens in one forward pass.
        let input = Tensor::new(input_ids.as_slice(), &device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("prompt tensor: {e}")))?;
        let logits = model
            .forward(&input, 0)
            .and_then(|t| t.squeeze(0))
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("prefill forward: {e}")))?;
        let mut next_token = logits_proc
            .sample(&logits)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("sample: {e}")))?;

        let mut output_ids = Vec::with_capacity(self.max_tokens);
        output_ids.push(next_token);

        // Autoregressive decode loop.
        for step in 1..self.max_tokens {
            if is_eos(next_token, eos_end, eos_text) {
                break;
            }
            let inp = Tensor::new(&[next_token], &device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| AgentRuntimeError::LocalLlm(format!("step tensor: {e}")))?;
            let logits = model
                .forward(&inp, n_prompt + step)
                .and_then(|t| t.squeeze(0))
                .map_err(|e| AgentRuntimeError::LocalLlm(format!("decode forward: {e}")))?;
            next_token = logits_proc
                .sample(&logits)
                .map_err(|e| AgentRuntimeError::LocalLlm(format!("sample: {e}")))?;
            output_ids.push(next_token);
        }

        // Strip any trailing EOS before decoding to text.
        let trimmed: Vec<u32> = output_ids
            .into_iter()
            .take_while(|&t| !is_eos(t, eos_end, eos_text))
            .collect();

        tokenizer
            .decode(&trimmed, true)
            .map(|s| s.trim().to_string())
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("decode tokens: {e}")))
    }
}

fn is_eos(token: u32, eos_end: Option<u32>, eos_text: Option<u32>) -> bool {
    eos_end.is_some_and(|t| t == token) || eos_text.is_some_and(|t| t == token)
}

/// Phi-4 mini uses partial RoPE (rope_dim=96, head_dim=128). quantized_phi3 applies
/// RoPE to the full head dimension; to avoid the shape mismatch we declare rope_dim
/// equal to head_dim. The extra dims get slightly-extended frequencies, which is
/// acceptable for smoke-test use but not suitable for production inference.
fn patch_rope_dim_to_head_dim(content: &mut gguf_file::Content) {
    use candle_core::quantized::gguf_file::Value;

    let head_dim = {
        let emb = content
            .metadata
            .get("phi3.embedding_length")
            .and_then(|v| v.to_u32().ok())
            .unwrap_or(3072);
        let heads = content
            .metadata
            .get("phi3.attention.head_count")
            .and_then(|v| v.to_u32().ok())
            .unwrap_or(24);
        emb / heads
    };

    content.metadata.insert(
        "phi3.rope.dimension_count".to_string(),
        Value::U32(head_dim),
    );
}

/// Phi-4 mini GGUF omits `output.weight` because the model uses tied embeddings
/// (the output projection reuses `token_embd.weight`). Patch the Content metadata
/// with an alias so `quantized_phi3::ModelWeights::from_gguf` can find the tensor.
fn patch_tied_output_weight(content: &mut gguf_file::Content) {
    if content.tensor_infos.contains_key("output.weight") {
        return;
    }
    if let Some(embd) = content.tensor_infos.get("token_embd.weight") {
        let aliased = TensorInfo {
            ggml_dtype: embd.ggml_dtype,
            shape: embd.shape.clone(),
            offset: embd.offset,
        };
        content
            .tensor_infos
            .insert("output.weight".to_string(), aliased);
    }
}

impl AgentRuntime for LocalCandelRuntime {
    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
        let tokenizer = self.load_tokenizer()?;
        let prompt = self.format_prompt(&request);
        let assistant_text = self.run_inference(&prompt, &tokenizer)?;
        Ok(ModelResponse::text(assistant_text))
    }
}
