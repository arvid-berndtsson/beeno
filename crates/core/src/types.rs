use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: Option<String>,
    pub language_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateRequest {
    pub input: String,
    pub mode: String,
    pub session_summary: SessionSummary,
    pub file_metadata: Option<FileMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResult {
    pub code: String,
    pub explanation: Option<String>,
    pub confidence: Option<f32>,
    pub tokens: Option<u32>,
    pub raw_provider_meta: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
pub enum RiskLevel {
    Safe,
    Risky,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReport {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub source: String,
    pub deno_permissions: DenoPermissions,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DenoPermissions {
    pub allow_read: Vec<String>,
    pub allow_write: Vec<String>,
    pub allow_net: Vec<String>,
    pub allow_env: bool,
    pub allow_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionSummary {
    pub symbols: Vec<String>,
    pub imports: Vec<String>,
    pub side_effects: Vec<String>,
    pub recent_intents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonEnvelope {
    pub status: String,
    pub phase: String,
    pub message: String,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplConfig {
    pub summary_window: usize,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self { summary_window: 8 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    pub translate_ms: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            translate_ms: 15_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub policy_path: Option<String>,
    pub confirm_risky: bool,
    pub repl: ReplConfig,
    pub timeouts: TimeoutConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: "http".to_string(),
            model: "gpt-4.1-mini".to_string(),
            temperature: 0.1,
            max_tokens: 512,
            policy_path: None,
            confirm_risky: true,
            repl: ReplConfig::default(),
            timeouts: TimeoutConfig::default(),
        }
    }
}
