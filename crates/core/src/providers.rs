use crate::types::{TranslateRequest, TranslateResult};
use async_trait::async_trait;
use reqwest::{Client, RequestBuilder};
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

#[async_trait]
impl<T> TranslatorProvider for Box<T>
where
    T: TranslatorProvider + ?Sized,
{
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError> {
        (**self).translate(req).await
    }
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
struct LegacyRequestBody {
    model: String,
    input: String,
    temperature: f32,
    max_tokens: u32,
    metadata: Value,
}

#[async_trait]
impl TranslatorProvider for HttpProvider {
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError> {
        let payload = LegacyRequestBody {
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

        let value = send_json(request).await?;
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
pub struct OpenAICompatProvider {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    client: Client,
}

impl OpenAICompatProvider {
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
struct OpenAICompatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAICompatRequest {
    model: String,
    messages: Vec<OpenAICompatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[async_trait]
impl TranslatorProvider for OpenAICompatProvider {
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError> {
        let payload = OpenAICompatRequest {
            model: self.model.clone(),
            messages: vec![
                OpenAICompatMessage {
                    role: "system".to_string(),
                    content: "Translate user input to executable JavaScript/TypeScript only. Return code only.".to_string(),
                },
                OpenAICompatMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Input mode: {}\\nSession summary: {:?}\\nInput: {}",
                        req.mode, req.session_summary, req.input
                    ),
                },
            ],
            temperature: self.temperature,
            max_tokens: self.max_tokens,
        };

        let mut request = self.client.post(&self.endpoint).json(&payload);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }

        let value = send_json(request).await?;
        let content = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|first| first.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "missing choices[0].message.content in OpenAI-compatible response".to_string(),
                )
            })?;

        let code = strip_code_fences(content);
        let mut meta = BTreeMap::new();
        meta.insert("raw".to_string(), value);

        Ok(TranslateResult {
            code,
            explanation: None,
            confidence: None,
            tokens: None,
            raw_provider_meta: meta,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    pub endpoint: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    client: Client,
}

impl OllamaProvider {
    pub fn new(endpoint: String, model: String, temperature: f32, max_tokens: u32) -> Self {
        Self {
            endpoint,
            model,
            temperature,
            max_tokens,
            client: Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: Value,
}

#[async_trait]
impl TranslatorProvider for OllamaProvider {
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult, ProviderError> {
        let payload = OllamaRequest {
            model: self.model.clone(),
            prompt: format!(
                "Translate to executable JavaScript/TypeScript only. Return code only.\\nInput mode: {}\\nSession summary: {:?}\\nInput: {}",
                req.mode, req.session_summary, req.input
            ),
            stream: false,
            options: json!({
                "temperature": self.temperature,
                "num_predict": self.max_tokens,
            }),
        };

        let request = self.client.post(&self.endpoint).json(&payload);
        let value = send_json(request).await?;

        let response = value
            .get("response")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "missing string field `response` in Ollama response".to_string(),
                )
            })?;

        let code = strip_code_fences(response);
        let mut meta = BTreeMap::new();
        meta.insert("raw".to_string(), value);

        Ok(TranslateResult {
            code,
            explanation: None,
            confidence: None,
            tokens: None,
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

async fn send_json(request: RequestBuilder) -> Result<Value, ProviderError> {
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
    response
        .json()
        .await
        .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
}

fn strip_code_fences(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let _ = lines.next();
        let mut body: Vec<&str> = lines.collect();
        if matches!(body.last(), Some(last) if last.trim() == "```") {
            body.pop();
        }
        body.join("\n").trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::strip_code_fences;

    #[test]
    fn strips_markdown_fence() {
        let src = "```ts\\nconsole.log('x');\\n```";
        assert_eq!(strip_code_fences(src), "console.log('x');");
    }
}
