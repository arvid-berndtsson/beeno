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
    pub server: Option<ServerContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerContext {
    pub running: bool,
    pub url: Option<String>,
    pub port: Option<u16>,
    pub mode: String,
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
#[serde(default)]
pub struct LlmConfig {
    pub provider: String,
    pub endpoint: Option<String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub endpoint_env_var: String,
    pub api_key_env_var: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "http".to_string(),
            endpoint: None,
            model: "gpt-4.1-mini".to_string(),
            temperature: 0.1,
            max_tokens: 512,
            endpoint_env_var: "DENO_NL_ENDPOINT".to_string(),
            api_key_env_var: "DENO_NL_API_KEY".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicySettings {
    pub policy_path: Option<String>,
    pub confirm_risky: bool,
}

impl Default for PolicySettings {
    fn default() -> Self {
        Self {
            policy_path: None,
            confirm_risky: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SelfHealConfig {
    pub enabled: bool,
    pub auto_on_run_failure: bool,
    pub apply_fixes_default: bool,
    pub max_attempts: u8,
}

impl Default for SelfHealConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_on_run_failure: true,
            apply_fixes_default: false,
            max_attempts: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtifactConfig {
    pub dir: String,
    pub keep_last: usize,
}

impl Default for ArtifactConfig {
    fn default() -> Self {
        Self {
            dir: ".beeno/suggestions".to_string(),
            keep_last: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub max_files: usize,
    pub max_changed_lines: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_files: 10,
            max_changed_lines: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProtectConfig {
    pub deny: Vec<String>,
}

impl Default for ProtectConfig {
    fn default() -> Self {
        Self {
            deny: vec![
                ".env".to_string(),
                ".env.*".to_string(),
                "deno.lock".to_string(),
                "Cargo.lock".to_string(),
                "package-lock.json".to_string(),
                "pnpm-lock.yaml".to_string(),
                "yarn.lock".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub policy: PolicySettings,
    pub self_heal: SelfHealConfig,
    pub artifacts: ArtifactConfig,
    pub limits: LimitsConfig,
    pub protect: ProtectConfig,
    pub repl: ReplConfig,
    pub timeouts: TimeoutConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            policy: PolicySettings::default(),
            self_heal: SelfHealConfig::default(),
            artifacts: ArtifactConfig::default(),
            limits: LimitsConfig::default(),
            protect: ProtectConfig::default(),
            repl: ReplConfig::default(),
            timeouts: TimeoutConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_defaults_are_stable() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.llm.provider, "http");
        assert!(cfg.policy.confirm_risky);
        assert!(cfg.self_heal.auto_on_run_failure);
        assert_eq!(cfg.self_heal.max_attempts, 3);
        assert_eq!(cfg.artifacts.dir, ".beeno/suggestions");
        assert_eq!(cfg.artifacts.keep_last, 20);
    }

    #[test]
    fn partial_toml_parses_with_defaults() {
        let raw = r#"
        [llm]
        provider = "mock"

        [artifacts]
        keep_last = 5
        "#;
        let cfg: AppConfig = toml::from_str(raw).expect("must parse");
        assert_eq!(cfg.llm.provider, "mock");
        assert_eq!(cfg.llm.model, "gpt-4.1-mini");
        assert_eq!(cfg.artifacts.keep_last, 5);
        assert_eq!(cfg.artifacts.dir, ".beeno/suggestions");
    }
}
