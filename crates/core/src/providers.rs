use crate::types::{TranslateRequest, TranslateResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider response invalid: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait TranslatorProvider: Send + Sync {
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError>;
}

#[derive(Debug, Clone)]
pub struct HttpProvider {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    client: Client,
}

impl HttpProvider {
    pub fn new(
        endpoint: String,
        api_key: Option<String>,
        model: String,
        temperature: f32,
        max_tokens: u32,
    ) -> Self {
        Self {
            endpoint,
            api_key,
            model,
            temperature,
            max_tokens,
            client: Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct RequestBody {
    model: String,
    input: String,
    temperature: f32,
    max_tokens: u32,
    metadata: Value,
}

#[async_trait]
impl TranslatorProvider for HttpProvider {
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError> {
        let payload = RequestBody {
            model: self.model.clone(),
            input: format!(
                "Translate to executable JS/TS only. Input mode: {}.\\nSession summary: {:?}\\nInput: {}",
                req.mode, req.session_summary, req.input
            ),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            metadata: json!({
                "file_metadata": req.file_metadata,
            }),
        };

        let mut request = self.client.post(&self.endpoint).json(&payload);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::Request(format!(
                "http status {} from provider",
                status
            )));
        }

        let value: Value = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let code = value
            .get("code")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing string field `code`".to_string())
            })?
            .to_string();

        let mut meta = BTreeMap::new();
        meta.insert("raw".to_string(), value.clone());

        Ok(TranslateResult {
            code,
            explanation: value
                .get("explanation")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            confidence: value
                .get("confidence")
                .and_then(Value::as_f64)
                .map(|v| v as f32),
            tokens: value
                .get("tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
            raw_provider_meta: meta,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MockProvider;

#[async_trait]
impl TranslatorProvider for MockProvider {
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError> {
        let mut meta = BTreeMap::new();
        meta.insert("provider".to_string(), json!("mock"));

        Ok(TranslateResult {
            code: format!("console.log({:?});", req.input),
            explanation: Some("mock translation".to_string()),
            confidence: Some(0.99),
            tokens: Some(8),
            raw_provider_meta: meta,
        })
    }
}
