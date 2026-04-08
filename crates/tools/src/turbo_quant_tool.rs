//! Agent-facing [turbo-quant](https://crates.io/crates/turbo-quant) operations on real vector inputs.

use serde::Deserialize;
use serde_json::{json, Value};
use turbo_quant::kv::{KvCacheCompressor, KvCacheConfig};
use turbo_quant::TurboQuantizer;

const MAX_DIM: usize = 4096;
const MAX_KEYS: usize = 512;

#[derive(Debug, Deserialize)]
pub struct TurboQuantToolInput {
    /// `inner_product_estimate` or `attention_scores`
    pub action: String,
    #[serde(default)]
    pub key: Option<Vec<f64>>,
    #[serde(default)]
    pub keys: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    pub query: Option<Vec<f64>>,
    #[serde(default)]
    pub bits: Option<u8>,
    #[serde(default)]
    pub projections: Option<usize>,
    #[serde(default)]
    pub seed: Option<u64>,
}

fn f32_slice(values: &[f64]) -> Vec<f32> {
    values.iter().map(|&x| x as f32).collect()
}

fn inner_product_f32(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn resolve_bits(bits: Option<u8>) -> Result<u8, String> {
    let b = bits.unwrap_or(8);
    match b {
        4 | 8 => Ok(b),
        _ => Err(format!("bits must be 4 or 8, got {b}")),
    }
}

fn resolve_projections(dim: usize, projections: Option<usize>) -> usize {
    projections.unwrap_or_else(|| (dim / 8).max(1))
}

fn resolve_seed(seed: Option<u64>) -> u64 {
    seed.unwrap_or(42)
}

pub fn execute_turbo_quant_tool(input: TurboQuantToolInput) -> Result<Value, String> {
    let action = input.action.trim();
    match action {
        "inner_product_estimate" => run_inner_product_estimate(input),
        "attention_scores" => run_attention_scores(input),
        _ => Err(format!(
            "unknown action '{action}' (expected inner_product_estimate or attention_scores)"
        )),
    }
}

fn run_inner_product_estimate(input: TurboQuantToolInput) -> Result<Value, String> {
    let key = input
        .key
        .as_ref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "inner_product_estimate requires non-empty key".to_string())?;
    let query = input
        .query
        .as_ref()
        .filter(|q| !q.is_empty())
        .ok_or_else(|| "inner_product_estimate requires non-empty query".to_string())?;
    if key.len() != query.len() {
        return Err(format!(
            "key length {} must match query length {}",
            key.len(),
            query.len()
        ));
    }
    let dim = key.len();
    if dim > MAX_DIM {
        return Err(format!("dim {dim} exceeds maximum {MAX_DIM}"));
    }

    let bits = resolve_bits(input.bits)?;
    let projections = resolve_projections(dim, input.projections);
    let seed = resolve_seed(input.seed);

    let key_f = f32_slice(key);
    let query_f = f32_slice(query);
    let true_ip = inner_product_f32(&key_f, &query_f);

    let tq = TurboQuantizer::new(dim, bits, projections, seed)
        .map_err(|e| format!("TurboQuantizer: {e}"))?;
    let code = tq.encode(&key_f).map_err(|e| format!("encode: {e}"))?;
    let est = tq
        .inner_product_estimate(&code, &query_f)
        .map_err(|e| format!("inner_product_estimate: {e}"))?;
    let encoded_bytes = code.encoded_bytes();
    let f32_baseline = dim * 4;
    let compression_ratio = f32_baseline as f64 / encoded_bytes.max(1) as f64;

    Ok(json!({
        "action": "inner_product_estimate",
        "dim": dim,
        "bits": bits,
        "projections": projections,
        "seed": seed,
        "true_inner_product": true_ip,
        "estimated_inner_product": est,
        "absolute_error": (true_ip - est).abs(),
        "encoded_bytes": encoded_bytes,
        "f32_baseline_bytes": f32_baseline,
        "compression_ratio": compression_ratio,
    }))
}

fn run_attention_scores(input: TurboQuantToolInput) -> Result<Value, String> {
    let keys = input
        .keys
        .as_ref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "attention_scores requires non-empty keys array".to_string())?;
    if keys.len() > MAX_KEYS {
        return Err(format!("at most {MAX_KEYS} keys allowed, got {}", keys.len()));
    }
    let query = input
        .query
        .as_ref()
        .filter(|q| !q.is_empty())
        .ok_or_else(|| "attention_scores requires non-empty query".to_string())?;

    let dim = query.len();
    if dim > MAX_DIM {
        return Err(format!("dim {dim} exceeds maximum {MAX_DIM}"));
    }
    for (i, k) in keys.iter().enumerate() {
        if k.len() != dim {
            return Err(format!(
                "keys[{i}] length {} must match query length {dim}",
                k.len()
            ));
        }
    }

    let bits = resolve_bits(input.bits)?;
    let projections = resolve_projections(dim, input.projections);
    let seed = resolve_seed(input.seed);

    let query_f = f32_slice(query);
    let true_scores: Vec<f32> = keys
        .iter()
        .map(|k| inner_product_f32(&f32_slice(k), &query_f))
        .collect();

    let kv_config = KvCacheConfig {
        head_dim: dim,
        bits,
        projections,
        seed,
    };
    let mut cache = KvCacheCompressor::new(kv_config).map_err(|e| format!("KvCacheCompressor: {e}"))?;
    let zeros = vec![0.0f32; dim];
    for k in keys {
        let kf = f32_slice(k);
        cache
            .compress_token(&kf, &zeros)
            .map_err(|e| format!("compress_token: {e}"))?;
    }

    let est_scores = cache
        .attention_scores(&query_f)
        .map_err(|e| format!("attention_scores: {e}"))?;

    let mut abs_err = 0.0f32;
    for (i, &s) in est_scores.iter().enumerate() {
        abs_err += (true_scores[i] - s).abs();
    }
    let mean_abs = abs_err / est_scores.len() as f32;

    Ok(json!({
        "action": "attention_scores",
        "token_count": est_scores.len(),
        "dim": dim,
        "bits": bits,
        "projections": projections,
        "seed": seed,
        "mean_absolute_error": mean_abs,
        "true_scores": true_scores,
        "estimated_scores": est_scores,
    }))
}
