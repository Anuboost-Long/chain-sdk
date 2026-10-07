//! On-device sentence embeddings — see
//! agent-docs/capabilities/embeddings/CONTRACT.md. A Hugging Face
//! tokenizer (the `tokenizers` crate) feeds a BERT-style ONNX model run by
//! the ONNX Runtime sherpa-onnx already links (ort.rs), so it's the same
//! Rust on every OS.

#[cfg(not(chain_no_sherpa))]
mod ort;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

#[derive(Debug)]
pub enum EmbeddingsError {
    InvalidArgument(String),
    /// A model file, or the tokenizer, isn't there.
    NotFound(String),
    /// The files aren't an embedding model or a tokenizer Chain can use.
    InvalidModel(String),
    Unsupported(String),
    OutOfMemory(String),
    Cancelled,
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pooling {
    #[default]
    Mean,
    Cls,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Input {
    Query,
    Passage,
}

/// `model` and `tokenizer` are relative names from the app's catalog; the
/// models capability resolves them to paths before they get here.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModelConfig {
    pub model: String,
    pub tokenizer: String,
    pub tokenizer_model_id: Option<String>,
    #[serde(default)]
    pub pooling: Pooling,
    pub normalize: Option<bool>,
    pub max_tokens: Option<usize>,
    pub query_prefix: Option<String>,
    pub passage_prefix: Option<String>,
}

impl EmbeddingModelConfig {
    fn prefix(&self, input: Option<Input>) -> &str {
        match input {
            Some(Input::Query) => self.query_prefix.as_deref(),
            Some(Input::Passage) => self.passage_prefix.as_deref(),
            None => None,
        }
        .unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub available: bool,
    pub pooling: Vec<Pooling>,
}

pub fn availability() -> Availability {
    let available = cfg!(not(chain_no_sherpa));
    Availability { available, pooling: if available { vec![Pooling::Mean, Pooling::Cls] } else { Vec::new() } }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextInfo {
    pub tokens: usize,
    pub truncated: bool,
}

#[derive(Debug)]
pub struct Embedded {
    pub dimension: usize,
    pub max_tokens: usize,
    /// One `dimension`-long vector per text, back to back.
    pub vectors: Vec<f32>,
    pub texts: Vec<TextInfo>,
}

impl Embedded {
    /// What `embeddings_embed` sends the webview, little-endian u32s then
    /// f32s, all 4-byte aligned: dimension, maxTokens, count; per text its
    /// tokens and truncated (0/1); then every vector. Parsed by
    /// packages/sdk/src/embeddings.ts.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.texts.len() * 8 + self.vectors.len() * 4);
        for n in [self.dimension, self.max_tokens, self.texts.len()] {
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        for text in &self.texts {
            out.extend_from_slice(&(text.tokens as u32).to_le_bytes());
            out.extend_from_slice(&u32::from(text.truncated).to_le_bytes());
        }
        for x in &self.vectors {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCounts {
    pub tokens: Vec<usize>,
    pub max_tokens: usize,
}

pub const DEFAULT_BATCH_SIZE: usize = 16;
const BATCH_SIZES: std::ops::RangeInclusive<usize> = 1..=256;
/// When neither the tokenizer nor its tokenizer_config.json names a limit:
/// what BERT-style models are trained with.
const FALLBACK_MAX_TOKENS: usize = 512;

struct LoadedTokenizer {
    path: PathBuf,
    tokenizer: Arc<Tokenizer>,
    model_limit: usize,
    pad_id: u32,
}

static TOKENIZER: Mutex<Option<LoadedTokenizer>> = Mutex::new(None);

/// The tokenizer at `path`, loaded once, with its own truncation and
/// padding off: Chain cuts and pads itself.
fn tokenizer(path: &Path) -> Result<(Arc<Tokenizer>, usize, u32), EmbeddingsError> {
    let mut loaded = TOKENIZER.lock().expect("embeddings mutex poisoned");
    if let Some(t) = loaded.as_ref().filter(|t| t.path == path) {
        return Ok((t.tokenizer.clone(), t.model_limit, t.pad_id));
    }
    let mut tokenizer = Tokenizer::from_file(path)
        .map_err(|e| EmbeddingsError::InvalidModel(format!("the tokenizer isn't a Hugging Face tokenizer.json: {e}")))?;
    let model_limit = tokenizer
        .get_truncation()
        .map(|t| t.max_length)
        .or_else(|| config_max_length(&path.with_file_name("tokenizer_config.json")))
        .unwrap_or(FALLBACK_MAX_TOKENS);
    let pad_id = tokenizer
        .get_padding()
        .map(|p| p.pad_id)
        .or_else(|| ["[PAD]", "<pad>"].into_iter().find_map(|t| tokenizer.token_to_id(t)))
        .unwrap_or(0);
    tokenizer
        .with_truncation(None)
        .map_err(|e| EmbeddingsError::InvalidModel(e.to_string()))?
        .with_padding(None);
    let tokenizer = Arc::new(tokenizer);
    *loaded = Some(LoadedTokenizer { path: path.to_path_buf(), tokenizer: tokenizer.clone(), model_limit, pad_id });
    Ok((tokenizer, model_limit, pad_id))
}

/// `model_max_length` from a tokenizer_config.json beside the tokenizer.
/// Hugging Face writes a huge sentinel (1e30) when a model has none.
fn config_max_length(path: &Path) -> Option<usize> {
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    config["model_max_length"].as_f64().filter(|n| *n >= 1.0 && *n <= 1_000_000.0).map(|n| n as usize)
}

fn max_tokens(config: &EmbeddingModelConfig, model_limit: usize) -> Result<usize, EmbeddingsError> {
    match config.max_tokens {
        Some(0) => Err(EmbeddingsError::InvalidArgument("maxTokens must be at least 1".to_string())),
        Some(n) => Ok(n),
        None => Ok(model_limit),
    }
}

/// A text's tokens, cut to `max` the way Hugging Face truncates: the
/// content is cut and the trailing special tokens (`[SEP]`, `</s>`) kept.
struct Tokens {
    ids: Vec<u32>,
    full_len: usize,
}

fn encode(tokenizer: &Tokenizer, texts: Vec<String>, max: usize) -> Result<Vec<Tokens>, EmbeddingsError> {
    let encodings = tokenizer.encode_batch(texts, true).map_err(|e| EmbeddingsError::Other(format!("couldn't tokenize: {e}")))?;
    Ok(encodings
        .into_iter()
        .map(|encoding| {
            let ids = encoding.get_ids();
            let full_len = ids.len();
            if full_len <= max {
                return Tokens { ids: ids.to_vec(), full_len };
            }
            let trailing = encoding.get_special_tokens_mask().iter().rev().take_while(|&&special| special == 1).count().min(max);
            let mut cut = ids[..max - trailing].to_vec();
            cut.extend_from_slice(&ids[full_len - trailing..]);
            Tokens { ids: cut, full_len }
        })
        .collect())
}

fn prefixed(texts: &[String], prefix: &str) -> Vec<String> {
    texts.iter().map(|text| format!("{prefix}{text}")).collect()
}

pub fn count_tokens(texts: &[String], config: &EmbeddingModelConfig, input: Option<Input>) -> Result<TokenCounts, EmbeddingsError> {
    let (tokenizer, model_limit, _) = tokenizer(Path::new(&config.tokenizer))?;
    let max_tokens = max_tokens(config, model_limit)?;
    let tokens = tokenizer
        .encode_batch(prefixed(texts, config.prefix(input)), true)
        .map_err(|e| EmbeddingsError::Other(format!("couldn't tokenize: {e}")))?
        .iter()
        .map(|encoding| encoding.len())
        .collect();
    Ok(TokenCounts { tokens, max_tokens })
}

/// Mean over the real (unpadded) tokens, or the first token.
fn pool(hidden: &[f32], seq: usize, dimension: usize, lens: &[usize], pooling: Pooling) -> Vec<f32> {
    let mut out = Vec::with_capacity(lens.len() * dimension);
    for (row, &len) in lens.iter().enumerate() {
        let tokens = &hidden[row * seq * dimension..][..seq * dimension];
        match pooling {
            Pooling::Cls => out.extend_from_slice(&tokens[..dimension]),
            Pooling::Mean => {
                let mut sum = vec![0f32; dimension];
                for token in tokens.chunks_exact(dimension).take(len) {
                    sum.iter_mut().zip(token).for_each(|(s, x)| *s += x);
                }
                out.extend(sum.into_iter().map(|s| s / len.max(1) as f32));
            }
        }
    }
    out
}

fn normalize(vectors: &mut [f32], dimension: usize) {
    for vector in vectors.chunks_exact_mut(dimension) {
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            vector.iter_mut().for_each(|x| *x /= norm);
        }
    }
}

// Cancelling by the caller's id (the SDK's AbortSignal). A cancel can
// reach us before its embed registers, so cancel() leaves a marker the
// embed then finds.

struct Running {
    cancelled: AtomicBool,
    #[cfg(not(chain_no_sherpa))]
    run: Option<ort::RunOptions>,
}

static RUNNING: Mutex<Option<HashMap<String, Arc<Running>>>> = Mutex::new(None);

pub fn cancel(id: &str) {
    let mut running = RUNNING.lock().expect("embeddings mutex poisoned");
    let entry = running.get_or_insert_with(HashMap::new).entry(id.to_string()).or_insert_with(|| {
        Arc::new(Running {
            cancelled: AtomicBool::new(false),
            #[cfg(not(chain_no_sherpa))]
            run: None,
        })
    });
    entry.cancelled.store(true, Ordering::SeqCst);
    #[cfg(not(chain_no_sherpa))]
    if let Some(run) = &entry.run {
        run.terminate();
    }
}

#[cfg(chain_no_sherpa)]
fn unsupported() -> EmbeddingsError {
    EmbeddingsError::Unsupported("embeddings need ONNX Runtime, which this platform's build doesn't include".to_string())
}

#[cfg(chain_no_sherpa)]
pub fn embed(
    _id: &str,
    _texts: &[String],
    _config: &EmbeddingModelConfig,
    _input: Input,
    _batch_size: Option<usize>,
) -> Result<Embedded, EmbeddingsError> {
    Err(unsupported())
}

#[cfg(chain_no_sherpa)]
pub fn unload() {
    *TOKENIZER.lock().expect("embeddings mutex poisoned") = None;
}

#[cfg(not(chain_no_sherpa))]
pub use engine::{embed, unload};

#[cfg(not(chain_no_sherpa))]
mod engine {
    use std::ffi::CString;

    use super::ort::{InputData, OrtError, RunOptions, Session, ELEMENT_INT32, ELEMENT_INT64};
    use super::*;

    impl From<OrtError> for EmbeddingsError {
        fn from(e: OrtError) -> Self {
            match e {
                OrtError::NotFound(m) => EmbeddingsError::NotFound(m),
                OrtError::InvalidModel(m) => EmbeddingsError::InvalidModel(m),
                OrtError::OutOfMemory(m) => EmbeddingsError::OutOfMemory(m),
                OrtError::Other(m) => EmbeddingsError::Other(m),
            }
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Feed {
        Ids,
        Mask,
        TypeIds,
        Positions,
    }

    struct Model {
        path: PathBuf,
        session: Session,
        inputs: Vec<(CString, Feed, i32)>,
        output: CString,
        /// The output is already one vector per text.
        pooled: bool,
    }

    static MODEL: Mutex<Option<Model>> = Mutex::new(None);

    fn open(path: &Path) -> Result<Model, EmbeddingsError> {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4) as i32);
        let session = Session::open(path, threads)?;
        let not_embedding = |why: &str| EmbeddingsError::InvalidModel(format!("not a sentence-embedding model: {why}"));
        let mut inputs = Vec::new();
        for input in session.inputs()? {
            let name = input.name.to_string_lossy();
            let feed = match () {
                _ if name.contains("input_ids") => Feed::Ids,
                _ if name.contains("attention_mask") => Feed::Mask,
                _ if name.contains("token_type_ids") => Feed::TypeIds,
                _ if name.contains("position_ids") => Feed::Positions,
                _ => return Err(not_embedding(&format!("it takes an input {name:?}"))),
            };
            if input.element_type != ELEMENT_INT64 && input.element_type != ELEMENT_INT32 {
                return Err(not_embedding(&format!("its input {name:?} isn't integers")));
            }
            inputs.push((input.name, feed, input.element_type));
        }
        if !inputs.iter().any(|(_, feed, _)| *feed == Feed::Ids) {
            return Err(not_embedding("it has no input_ids input"));
        }
        let outputs = session.outputs()?;
        let output = ["last_hidden_state", "token_embeddings", "sentence_embedding"]
            .iter()
            .find_map(|wanted| outputs.iter().find(|o| o.name.to_bytes() == wanted.as_bytes()))
            .or(outputs.first())
            .ok_or_else(|| not_embedding("it has no outputs"))?;
        let pooled = match output.shape.len() {
            3 => false,
            2 => true,
            rank => return Err(not_embedding(&format!("its output has {rank} dimensions"))),
        };
        Ok(Model { path: path.to_path_buf(), session, inputs, output: output.name.clone(), pooled })
    }

    fn feed(feed: Feed, rows: &[Tokens], seq: usize, pad_id: u32) -> Vec<i64> {
        let mut values = Vec::with_capacity(rows.len() * seq);
        for row in rows {
            let len = row.ids.len();
            values.extend((0..seq).map(|at| match feed {
                Feed::Ids => i64::from(row.ids.get(at).copied().unwrap_or(pad_id)),
                Feed::Mask => i64::from(at < len),
                Feed::TypeIds => 0,
                Feed::Positions => at as i64,
            }));
        }
        values
    }

    /// One batch through the model, pooled: `rows.len()` vectors back to back.
    fn run(model: &Model, rows: &[Tokens], pad_id: u32, pooling: Pooling, options: &RunOptions) -> Result<(usize, Vec<f32>), EmbeddingsError> {
        let seq = rows.iter().map(|r| r.ids.len()).max().unwrap_or(0);
        let shape = vec![rows.len() as i64, seq as i64];
        let mut inputs: Vec<ort::Input> = model
            .inputs
            .iter()
            .map(|(name, kind, element_type)| {
                let values = feed(*kind, rows, seq, pad_id);
                ort::Input {
                    name: name.clone(),
                    shape: shape.clone(),
                    data: if *element_type == ELEMENT_INT32 {
                        InputData::Int32(values.into_iter().map(|v| v as i32).collect())
                    } else {
                        InputData::Int64(values)
                    },
                }
            })
            .collect();
        let (out_shape, data) = model.session.run(&mut inputs, &model.output, options)?;
        let dimension = *out_shape.last().unwrap_or(&0) as usize;
        if dimension == 0 {
            return Err(EmbeddingsError::InvalidModel("the model's output is empty".to_string()));
        }
        if model.pooled {
            return Ok((dimension, data));
        }
        let lens: Vec<usize> = rows.iter().map(|r| r.ids.len()).collect();
        Ok((dimension, pool(&data, seq, dimension, &lens, pooling)))
    }

    /// Removes the call's cancel entry however `embed` ends.
    struct Registered<'a>(&'a str);

    impl Drop for Registered<'_> {
        fn drop(&mut self) {
            if let Some(running) = RUNNING.lock().expect("embeddings mutex poisoned").as_mut() {
                running.remove(self.0);
            }
        }
    }

    fn register(id: &str) -> Result<(Registered<'_>, Arc<Running>), EmbeddingsError> {
        let mut running = RUNNING.lock().expect("embeddings mutex poisoned");
        let running = running.get_or_insert_with(HashMap::new);
        if running.get(id).is_some_and(|r| r.cancelled.load(Ordering::SeqCst)) {
            running.remove(id);
            return Err(EmbeddingsError::Cancelled);
        }
        let entry = Arc::new(Running { cancelled: AtomicBool::new(false), run: Some(RunOptions::new()?) });
        running.insert(id.to_string(), entry.clone());
        Ok((Registered(id), entry))
    }

    pub fn embed(
        id: &str,
        texts: &[String],
        config: &EmbeddingModelConfig,
        input: Input,
        batch_size: Option<usize>,
    ) -> Result<Embedded, EmbeddingsError> {
        let batch_size = batch_size.unwrap_or(DEFAULT_BATCH_SIZE);
        if !BATCH_SIZES.contains(&batch_size) {
            return Err(EmbeddingsError::InvalidArgument(format!("batchSize must be 1–256, got {batch_size}")));
        }
        let (_registered, running) = register(id)?;
        let (tokenizer, model_limit, pad_id) = tokenizer(Path::new(&config.tokenizer))?;
        let max_tokens = max_tokens(config, model_limit)?;
        let mut vectors = Vec::new();
        let mut infos = Vec::with_capacity(texts.len());
        let mut dimension = 0;
        for chunk in texts.chunks(batch_size) {
            let rows = encode(&tokenizer, prefixed(chunk, config.prefix(Some(input))), max_tokens)?;
            let mut loaded = MODEL.lock().expect("embeddings mutex poisoned");
            if running.cancelled.load(Ordering::SeqCst) {
                return Err(EmbeddingsError::Cancelled);
            }
            if loaded.as_ref().is_none_or(|m| m.path != Path::new(&config.model)) {
                *loaded = None; // free the old model before loading the next
                *loaded = Some(open(Path::new(&config.model))?);
            }
            let model = loaded.as_ref().expect("just loaded");
            let options = running.run.as_ref().expect("registered with run options");
            let result = run(model, &rows, pad_id, config.pooling, options);
            drop(loaded);
            if running.cancelled.load(Ordering::SeqCst) {
                return Err(EmbeddingsError::Cancelled);
            }
            let (batch_dimension, batch) = match result {
                Err(EmbeddingsError::OutOfMemory(m)) => {
                    return Err(EmbeddingsError::OutOfMemory(format!(
                        "ran out of memory embedding {} texts at once — try a smaller batchSize ({m})",
                        rows.len()
                    )))
                }
                other => other?,
            };
            dimension = batch_dimension;
            vectors.extend(batch);
            infos.extend(rows.iter().map(|r| TextInfo { tokens: r.full_len, truncated: r.full_len > max_tokens }));
        }
        if config.normalize.unwrap_or(true) {
            normalize(&mut vectors, dimension.max(1));
        }
        Ok(Embedded { dimension, max_tokens, vectors, texts: infos })
    }

    /// Frees the model and tokenizer, after the batch in progress.
    pub fn unload() {
        *MODEL.lock().expect("embeddings mutex poisoned") = None;
        *TOKENIZER.lock().expect("embeddings mutex poisoned") = None;
    }

    #[cfg(test)]
    pub fn runtime_version() -> String {
        super::ort::version()
    }

    #[cfg(test)]
    pub fn open_for_test(path: &Path) -> Result<(), EmbeddingsError> {
        open(path).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_pooling_skips_padding_and_cls_takes_the_first_token() {
        // 2 texts × seq 3 × dimension 2; the second text has 1 real token.
        let hidden = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 100.0, 100.0, 100.0, 100.0];
        assert_eq!(pool(&hidden, 3, 2, &[3, 1], Pooling::Mean), vec![3.0, 4.0, 7.0, 8.0]);
        assert_eq!(pool(&hidden, 3, 2, &[3, 1], Pooling::Cls), vec![1.0, 2.0, 7.0, 8.0]);
    }

    #[test]
    fn normalizing_gives_unit_length_and_leaves_zero_vectors() {
        let mut vectors = vec![3.0, 4.0, 0.0, 0.0];
        normalize(&mut vectors, 2);
        assert_eq!(vectors, vec![0.6, 0.8, 0.0, 0.0]);
    }

    #[test]
    fn bytes_layout_matches_the_sdk_parser() {
        let embedded = Embedded {
            dimension: 2,
            max_tokens: 512,
            vectors: vec![0.5, -1.0, 2.0, 0.0],
            texts: vec![TextInfo { tokens: 7, truncated: false }, TextInfo { tokens: 600, truncated: true }],
        };
        let bytes = embedded.to_bytes();
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!([u32_at(0), u32_at(4), u32_at(8)], [2, 512, 2]);
        assert_eq!([u32_at(12), u32_at(16), u32_at(20), u32_at(24)], [7, 0, 600, 1]);
        assert_eq!(f32::from_le_bytes(bytes[28..32].try_into().unwrap()), 0.5);
        assert_eq!(bytes.len(), 28 + 4 * 4);
    }

    #[cfg(not(chain_no_sherpa))]
    #[test]
    fn a_cancel_that_arrives_before_its_embed_still_cancels_it() {
        let config: EmbeddingModelConfig =
            serde_json::from_value(serde_json::json!({ "model": "unused.onnx", "tokenizer": "unused.json" })).unwrap();
        cancel("early");
        assert!(matches!(embed("early", &["text".to_string()], &config, Input::Query, None), Err(EmbeddingsError::Cancelled)));
    }

    #[cfg(not(chain_no_sherpa))]
    #[test]
    fn a_file_that_isnt_an_onnx_model_is_an_invalid_model() {
        let path = std::env::temp_dir().join(format!("chain-embeddings-not-a-model-{}.onnx", std::process::id()));
        std::fs::write(&path, b"not a model").unwrap();
        let result = engine::open_for_test(&path);
        std::fs::remove_file(&path).ok();
        assert!(matches!(result, Err(EmbeddingsError::InvalidModel(_))), "{result:?}");
    }

    /// Against real models, only when CHAIN_TEST_EMBEDDING_MODELS names a
    /// folder holding bge/ and e5/ (each model.onnx + tokenizer.json) and
    /// reference.json from research/reference.py.
    #[cfg(not(chain_no_sherpa))]
    #[test]
    fn matches_reference_vectors_from_real_models() {
        let Some(dir) = std::env::var_os("CHAIN_TEST_EMBEDDING_MODELS").map(PathBuf::from) else {
            eprintln!("skipped: set CHAIN_TEST_EMBEDDING_MODELS to run against real models");
            return;
        };
        eprintln!("ONNX Runtime {}", engine::runtime_version());
        let reference: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("reference.json")).unwrap()).unwrap();
        for case in reference.as_array().unwrap() {
            let name = case["model"].as_str().unwrap();
            let config: EmbeddingModelConfig = serde_json::from_value(serde_json::json!({
                "model": dir.join(name).join("model.onnx"),
                "tokenizer": dir.join(name).join("tokenizer.json"),
                "pooling": case["pooling"],
                "queryPrefix": case["queryPrefix"],
                "passagePrefix": case["passagePrefix"],
            }))
            .unwrap();
            let texts: Vec<String> = serde_json::from_value(case["texts"].clone()).unwrap();
            let input = if case["as"] == "query" { Input::Query } else { Input::Passage };
            let embedded = embed("test", &texts, &config, input, Some(3)).unwrap();
            let expected: Vec<Vec<f32>> = serde_json::from_value(case["vectors"].clone()).unwrap();
            assert_eq!(embedded.dimension, expected[0].len());
            for (got, want) in embedded.vectors.chunks(embedded.dimension).zip(&expected) {
                let cosine: f32 = got.iter().zip(want).map(|(a, b)| a * b).sum();
                assert!(cosine > 0.9999, "{name}: cosine {cosine} against the reference");
            }
            let counts = count_tokens(&texts, &config, Some(input)).unwrap();
            let expected_tokens: Vec<usize> = serde_json::from_value(case["tokens"].clone()).unwrap();
            assert_eq!(counts.tokens, expected_tokens, "{name}: token counts");
            assert_eq!(counts.max_tokens, 512);

            // Padding to a longer batch-mate changes only float rounding.
            let alone = embed("test", &texts[..1], &config, input, None).unwrap();
            let cosine: f32 = alone.vectors.iter().zip(&embedded.vectors).map(|(a, b)| a * b).sum();
            assert!(cosine > 0.99999, "{name}: batched vs alone, cosine {cosine}");
            let again = embed("test", &texts, &config, input, Some(3)).unwrap();
            assert_eq!(again.vectors, embedded.vectors, "{name}: same input, same vectors");
        }
        unload();
    }
}
