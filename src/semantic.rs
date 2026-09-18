use crate::{
    config::SemanticConfig,
    error::{AppError, AppResult},
    models::{Asset, ReindexResponse, SemanticSearchResult, SemanticStatusResponse},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

pub fn asset_document(asset: &Asset) -> String {
    let mut parts = vec![
        format!("name: {}", asset.row.name),
        format!("filename: {}", asset.row.original_filename),
        format!("type: {}", asset.row.extension.as_deref().unwrap_or("unknown")),
        format!("category: {}", asset.row.category.as_deref().unwrap_or("uncategorized")),
    ];

    if !asset.tags.is_empty() {
        parts.push(format!("tags: {}", asset.tags.join(", ")));
    }
    if let Some(description) = asset.row.description.as_deref() {
        if !description.trim().is_empty() {
            parts.push(format!("description: {}", description.trim()));
        }
    }
    if let Some(creator) = asset.row.creator.as_deref() {
        if !creator.trim().is_empty() {
            parts.push(format!("creator: {}", creator.trim()));
        }
    }
    if let Some(license) = asset.row.license.as_deref() {
        if !license.trim().is_empty() {
            parts.push(format!("license: {}", license.trim()));
        }
    }

    parts.join("\n")
}

pub fn document_hash(document: &str) -> String {
    hex::encode(Sha256::digest(document.as_bytes()))
}

pub async fn embed_text(config: &SemanticConfig, text: &str) -> AppResult<Vec<f32>> {
    if !config.enabled {
        return Err(AppError::BadRequest("semantic search is disabled".to_string()));
    }
    let url = format!("{}/api/embed", config.ollama_url.trim_end_matches('/'));
    let response = Client::new()
        .post(url)
        .json(&OllamaEmbedRequest {
            model: &config.model,
            input: text,
        })
        .send()
        .await
        .map_err(|err| AppError::Other(anyhow::anyhow!("Ollama request failed: {err}")))?
        .error_for_status()
        .map_err(|err| AppError::Other(anyhow::anyhow!("Ollama embed request failed: {err}")))?
        .json::<OllamaEmbedResponse>()
        .await
        .map_err(|err| AppError::Other(anyhow::anyhow!("invalid Ollama embed response: {err}")))?;

    response
        .embeddings
        .into_iter()
        .next()
        .filter(|embedding| !embedding.is_empty())
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("Ollama returned no embedding")))
}

pub async fn ollama_reachable(config: &SemanticConfig) -> bool {
    if !config.enabled {
        return false;
    }
    let url = format!("{}/api/tags", config.ollama_url.trim_end_matches('/'));
    Client::new()
        .get(url)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a <= f32::EPSILON || norm_b <= f32::EPSILON {
        return 0.0;
    }
    (dot / (norm_a.sqrt() * norm_b.sqrt())).clamp(-1.0, 1.0)
}

pub fn keyword_score(asset: &Asset, query: &str) -> f32 {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return 0.0;
    }

    let name = asset.row.name.to_ascii_lowercase();
    let filename = asset.row.original_filename.to_ascii_lowercase();
    let category = asset.row.category.as_deref().unwrap_or("").to_ascii_lowercase();
    let description = asset.row.description.as_deref().unwrap_or("").to_ascii_lowercase();
    let creator = asset.row.creator.as_deref().unwrap_or("").to_ascii_lowercase();
    let license = asset.row.license.as_deref().unwrap_or("").to_ascii_lowercase();
    let tags = asset.tags.join(" ").to_ascii_lowercase();

    let mut score = 0.0f32;
    if name == q { score += 1.0; }
    if name.contains(&q) { score += 0.70; }
    if filename.contains(&q) { score += 0.45; }
    if category.contains(&q) { score += 0.55; }
    if tags.contains(&q) { score += 0.65; }
    if description.contains(&q) { score += 0.35; }
    if creator.contains(&q) { score += 0.20; }
    if license.contains(&q) { score += 0.10; }

    let terms = q.split_whitespace().filter(|term| term.len() > 1).collect::<Vec<_>>();
    if !terms.is_empty() {
        let haystack = format!("{name} {filename} {category} {tags} {description}");
        let matched = terms.iter().filter(|term| haystack.contains(**term)).count();
        score += 0.45 * (matched as f32 / terms.len() as f32);
    }

    score.min(1.0)
}

pub fn combine_results(
    config: &SemanticConfig,
    mut scored: Vec<SemanticSearchResult>,
    limit: usize,
) -> Vec<SemanticSearchResult> {
    let semantic_weight = config.semantic_weight.max(0.0);
    let keyword_weight = config.keyword_weight.max(0.0);
    let total = (semantic_weight + keyword_weight).max(f32::EPSILON);

    for result in &mut scored {
        let normalized_semantic = ((result.semantic_score + 1.0) / 2.0).clamp(0.0, 1.0);
        result.combined_score =
            (normalized_semantic * semantic_weight + result.keyword_score * keyword_weight) / total;
    }

    scored.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.asset.row.name.cmp(&b.asset.row.name))
    });
    scored.truncate(limit);
    scored
}

pub fn fallback_results(
    query: &str,
    assets: Vec<Asset>,
    limit: usize,
) -> Vec<SemanticSearchResult> {
    let mut results = assets
        .into_iter()
        .map(|asset| {
            let keyword = keyword_score(&asset, query);
            SemanticSearchResult {
                asset,
                semantic_score: 0.0,
                keyword_score: keyword,
                combined_score: keyword,
            }
        })
        .filter(|result| result.keyword_score > 0.0)
        .collect::<Vec<_>>();

    results.sort_by(|a, b| {
        b.keyword_score
            .partial_cmp(&a.keyword_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    results
}

pub fn status(
    config: &SemanticConfig,
    reachable: bool,
    indexed_assets: i64,
    total_active_assets: i64,
    stale_assets: i64,
    last_error: Option<String>,
) -> SemanticStatusResponse {
    SemanticStatusResponse {
        enabled: config.enabled,
        ollama_reachable: reachable,
        model: config.model.clone(),
        indexed_assets,
        total_active_assets,
        stale_assets,
        last_error,
    }
}

pub fn reindex_response(
    model: String,
    requested: usize,
    indexed: usize,
    failures: Vec<String>,
) -> ReindexResponse {
    ReindexResponse {
        requested,
        indexed,
        failed: failures.len(),
        model,
        failures,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical_vectors_is_one() {
        let score = cosine_similarity(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]);
        assert!((score - 1.0).abs() < 0.0001);
    }

    #[test]
    fn cosine_similarity_rejects_mismatched_dimensions() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }
}
