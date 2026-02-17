use crate::providers::{ProviderError, TranslatorProvider};
use crate::types::{
    ExecutionRequest, FileMetadata, RiskLevel, RiskReport, SessionSummary, TranslateRequest,
    TranslateResult,
};
use async_trait::async_trait;
use deno_ast::{parse_module, MediaType, ParseParams};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::process::Command;
use url::Url;

/// Heuristic classification of user input before translation/execution.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InputKind {
    Code,
    Pseudocode,
}

/// Classifies text as probable JS/TS code or pseudocode.
///
/// # Examples
///
/// ```
/// use beeno_core::engine::{classify_input, InputKind};
///
/// assert_eq!(classify_input("let x = 1;"), InputKind::Code);
/// assert_eq!(classify_input("create a map and print all keys."), InputKind::Pseudocode);
/// ```
pub fn classify_input(input: &str) -> InputKind {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return InputKind::Code;
    }

    let indicators = [
        "let ",
        "const ",
        "function ",
        "=>",
        "import ",
        "export ",
        "class ",
        "if (",
        "for (",
        "while (",
        "console.",
    ];

    if indicators.iter().any(|i| trimmed.contains(i)) || trimmed.ends_with(';') {
        return InputKind::Code;
    }

    let words = trimmed.split_whitespace().count();
    let has_sentence_markers =
        trimmed.contains('.') || trimmed.contains(" then ") || trimmed.contains(" and ");
    if words > 5 && has_sentence_markers {
        InputKind::Pseudocode
    } else {
        InputKind::Code
    }
}

/// Policy interface used to validate generated source.
#[async_trait]
pub trait RiskPolicy: Send + Sync {
    /// Analyzes source and returns a risk report for execution gating.
    async fn analyze(&self, source: &str) -> RiskReport;
}

/// Configurable string-pattern policy inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    pub blocked_patterns: Vec<String>,
    pub risky_patterns: Vec<String>,
    pub trusted_import_prefixes: Vec<String>,
}

/// Default built-in policy implementation used by Beeno.
#[derive(Debug, Clone)]
pub struct DefaultRiskPolicy {
    cfg: PolicyConfig,
}

impl Default for DefaultRiskPolicy {
    fn default() -> Self {
        Self {
            cfg: PolicyConfig {
                blocked_patterns: vec![
                    "Deno.Command".to_string(),
                    "child_process".to_string(),
                    "import(\"http://".to_string(),
                    "import('http://".to_string(),
                ],
                risky_patterns: vec![
                    "eval(".to_string(),
                    "Function(".to_string(),
                    "Deno.permissions.request".to_string(),
                    "**/*".to_string(),
                ],
                trusted_import_prefixes: vec!["https://deno.land".to_string()],
            },
        }
    }
}

impl DefaultRiskPolicy {
    /// Loads policy settings from TOML or JSON file.
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let cfg = if path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .eq_ignore_ascii_case("json")
        {
            serde_json::from_str(&content)?
        } else {
            toml::from_str(&content)?
        };
        Ok(Self { cfg })
    }
}

#[async_trait]
impl RiskPolicy for DefaultRiskPolicy {
    async fn analyze(&self, source: &str) -> RiskReport {
        let mut reasons = Vec::new();
        for pattern in &self.cfg.blocked_patterns {
            if source.contains(pattern) {
                reasons.push(format!("blocked pattern detected: {pattern}"));
            }
        }

        if parse_js(source).is_err() {
            reasons.push("generated source does not parse as JS/TS".to_string());
            return RiskReport {
                level: RiskLevel::Blocked,
                reasons,
                requires_confirmation: false,
            };
        }

        if !reasons.is_empty() {
            return RiskReport {
                level: RiskLevel::Blocked,
                reasons,
                requires_confirmation: false,
            };
        }

        let mut risky_reasons = Vec::new();
        for pattern in &self.cfg.risky_patterns {
            if source.contains(pattern) {
                risky_reasons.push(format!("risky pattern detected: {pattern}"));
            }
        }

        if !risky_reasons.is_empty() {
            return RiskReport {
                level: RiskLevel::Risky,
                reasons: risky_reasons,
                requires_confirmation: true,
            };
        }

        RiskReport {
            level: RiskLevel::Safe,
            reasons: vec![],
            requires_confirmation: false,
        }
    }
}

/// Interface used to maintain rolling session context for LLM prompts.
#[async_trait]
pub trait ContextSummarizer: Send + Sync {
    /// Consumes an event and returns the updated summary snapshot.
    async fn update(&mut self, event: &str) -> SessionSummary;
    /// Returns the current summary snapshot.
    fn current(&self) -> SessionSummary;
}

/// Fixed-size rolling summary implementation for REPL-like workflows.
#[derive(Debug, Clone)]
pub struct RollingContextSummarizer {
    max: usize,
    summary: SessionSummary,
}

impl RollingContextSummarizer {
    /// Creates a summarizer with a maximum retained item count per bucket.
    pub fn new(max: usize) -> Self {
        Self {
            max,
            summary: SessionSummary::default(),
        }
    }

    fn push_trimmed(vec: &mut Vec<String>, value: String, max: usize) {
        vec.push(value);
        if vec.len() > max {
            let overflow = vec.len() - max;
            vec.drain(0..overflow);
        }
    }
}

#[async_trait]
impl ContextSummarizer for RollingContextSummarizer {
    async fn update(&mut self, event: &str) -> SessionSummary {
        let event = event.trim();
        if event.starts_with("import ") {
            Self::push_trimmed(&mut self.summary.imports, event.to_string(), self.max);
        } else if event.starts_with("let ")
            || event.starts_with("const ")
            || event.starts_with("function ")
        {
            let symbol = event
                .split_whitespace()
                .nth(1)
                .unwrap_or(event)
                .trim_matches(|c: char| c == '{' || c == '(' || c == ';')
                .to_string();
            Self::push_trimmed(&mut self.summary.symbols, symbol, self.max);
        } else {
            Self::push_trimmed(&mut self.summary.side_effects, event.to_string(), self.max);
        }
        Self::push_trimmed(
            &mut self.summary.recent_intents,
            event.to_string(),
            self.max,
        );
        self.summary.clone()
    }

    fn current(&self) -> SessionSummary {
        self.summary.clone()
    }
}

/// Errors emitted by translation, policy, and runtime execution flows.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("provider failure: {0}")]
    Provider(#[from] ProviderError),
    #[error("source blocked by policy: {0:?}")]
    Blocked(Vec<String>),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Main orchestration entry for classify/translate/validate flows.
///
/// This type coordinates [`TranslatorProvider`] and [`RiskPolicy`] to
/// convert input into executable source.
pub struct Engine<P, R>
where
    P: TranslatorProvider,
    R: RiskPolicy,
{
    provider: P,
    policy: R,
}

impl<P, R> Engine<P, R>
where
    P: TranslatorProvider,
    R: RiskPolicy,
{
    /// Constructs a new engine with a provider and policy implementation.
    pub fn new(provider: P, policy: R) -> Self {
        Self { provider, policy }
    }

    /// Prepares executable source from raw input and returns risk metadata.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use beeno_core::engine::{DefaultRiskPolicy, Engine};
    /// use beeno_core::providers::MockProvider;
    /// use beeno_core::types::SessionSummary;
    ///
    /// # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::new(MockProvider, DefaultRiskPolicy::default());
    /// let (source, _, risk) = engine
    ///     .prepare_source("print hello", "eval", SessionSummary::default(), None)
    ///     .await?;
    /// assert!(source.contains("console.log"));
    /// assert!(!risk.requires_confirmation);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn prepare_source(
        &self,
        input: &str,
        mode: &str,
        summary: SessionSummary,
        file_metadata: Option<FileMetadata>,
    ) -> Result<(String, Option<TranslateResult>, RiskReport), EngineError> {
        let (source, translated) = match classify_input(input) {
            InputKind::Code if mode != "force_nl" => (input.to_string(), None),
            _ => {
                let req = TranslateRequest {
                    input: input.to_string(),
                    mode: mode.to_string(),
                    session_summary: summary,
                    file_metadata,
                };
                let translated = self.provider.translate(req).await?;
                (translated.code.clone(), Some(translated))
            }
        };

        let risk = self.policy.analyze(&source).await;
        if risk.level == RiskLevel::Blocked {
            return Err(EngineError::Blocked(risk.reasons));
        }

        Ok((source, translated, risk))
    }

    /// Replaces tagged NL blocks in script content with translated JS/TS.
    pub async fn process_tagged_script(
        &self,
        script: &str,
        summary: SessionSummary,
        file_path: Option<String>,
    ) -> Result<(String, Vec<String>), EngineError> {
        let mut out = String::new();
        let mut warnings = Vec::new();
        let mut cursor = 0;

        while let Some(start) = script[cursor..].find("/*nl") {
            let abs_start = cursor + start;
            out.push_str(&script[cursor..abs_start]);
            let after_tag = abs_start + 4;
            let Some(end_rel) = script[after_tag..].find("*/") else {
                warnings.push("unterminated nl block; leaving remainder unchanged".to_string());
                out.push_str(&script[abs_start..]);
                return Ok((out, warnings));
            };
            let abs_end = after_tag + end_rel;
            let nl_body = script[after_tag..abs_end].trim();
            let req = TranslateRequest {
                input: strip_fenced_nl(nl_body),
                mode: "run".to_string(),
                session_summary: summary.clone(),
                file_metadata: Some(FileMetadata {
                    path: file_path.clone(),
                    language_hint: Some("typescript".to_string()),
                }),
            };
            let translated = self.provider.translate(req).await?;
            let risk = self.policy.analyze(&translated.code).await;
            if risk.level == RiskLevel::Blocked {
                return Err(EngineError::Blocked(risk.reasons));
            }
            out.push_str(&translated.code);
            cursor = abs_end + 2;
        }

        out.push_str(&script[cursor..]);
        Ok((out, warnings))
    }
}

fn strip_fenced_nl(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.starts_with("```nl") && trimmed.ends_with("```") {
        trimmed
            .trim_start_matches("```nl")
            .trim_end_matches("```")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    }
}

/// Validates permissions and executes source using the runtime backend.
pub async fn execute_request(req: ExecutionRequest) -> Result<(), EngineError> {
    enforce_permission_alignment(&req.source, &req.deno_permissions)?;
    execute_with_deno_binary(req).await
}

fn enforce_permission_alignment(
    source: &str,
    perms: &crate::types::DenoPermissions,
) -> Result<(), EngineError> {
    let read_ops = ["Deno.readTextFile", "Deno.readFile", "Deno.open("];
    let write_ops = ["Deno.writeTextFile", "Deno.writeFile", "Deno.mkdir("];
    let net_ops = ["fetch(", "WebSocket(", "Deno.connect("];
    let env_ops = ["Deno.env.get", "Deno.env.toObject", "Deno.env.set"];
    let run_ops = ["Deno.Command", "Deno.run("];

    if read_ops.iter().any(|op| source.contains(op)) && perms.allow_read.is_empty() {
        return Err(EngineError::Execution(
            "code requires --allow-read but none was provided".to_string(),
        ));
    }
    if write_ops.iter().any(|op| source.contains(op)) && perms.allow_write.is_empty() {
        return Err(EngineError::Execution(
            "code requires --allow-write but none was provided".to_string(),
        ));
    }
    if net_ops.iter().any(|op| source.contains(op)) && perms.allow_net.is_empty() {
        return Err(EngineError::Execution(
            "code requires --allow-net but none was provided".to_string(),
        ));
    }
    if env_ops.iter().any(|op| source.contains(op)) && !perms.allow_env {
        return Err(EngineError::Execution(
            "code requires --allow-env but none was provided".to_string(),
        ));
    }
    if run_ops.iter().any(|op| source.contains(op)) && !perms.allow_run {
        return Err(EngineError::Execution(
            "code requires --allow-run but none was provided".to_string(),
        ));
    }
    Ok(())
}

/// Parses source as TypeScript/JavaScript to ensure syntactic validity.
///
/// # Examples
///
/// ```
/// use beeno_core::engine::parse_js;
///
/// assert!(parse_js("const x: number = 1;").is_ok());
/// assert!(parse_js("const =").is_err());
/// ```
pub fn parse_js(source: &str) -> anyhow::Result<()> {
    parse_module(ParseParams {
        specifier: Url::parse("file:///inline.ts")?,
        text: Arc::<str>::from(source),
        media_type: MediaType::TypeScript,
        capture_tokens: false,
        maybe_syntax: None,
        scope_analysis: false,
    })?;
    Ok(())
}

async fn execute_with_deno_binary(req: ExecutionRequest) -> Result<(), EngineError> {
    let temp_path = temp_module_path();
    fs::write(&temp_path, req.source).map_err(EngineError::Io)?;

    let mut cmd = Command::new("deno");
    cmd.arg("run");
    for arg in permission_args(&req.deno_permissions) {
        cmd.arg(arg);
    }
    cmd.arg(&temp_path);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.stdin(Stdio::inherit());

    let status = cmd
        .status()
        .await
        .map_err(|e| EngineError::Execution(format!("failed to launch deno binary: {e}")))?;

    let _ = fs::remove_file(&temp_path);

    if status.success() {
        Ok(())
    } else {
        Err(EngineError::Execution(format!(
            "deno run exited with status {status}"
        )))
    }
}

fn permission_args(perms: &crate::types::DenoPermissions) -> Vec<String> {
    let mut args = Vec::new();
    if !perms.allow_read.is_empty() {
        args.push(format!("--allow-read={}", perms.allow_read.join(",")));
    }
    if !perms.allow_write.is_empty() {
        args.push(format!("--allow-write={}", perms.allow_write.join(",")));
    }
    if !perms.allow_net.is_empty() {
        args.push(format!("--allow-net={}", perms.allow_net.join(",")));
    }
    if perms.allow_env {
        args.push("--allow-env".to_string());
    }
    if perms.allow_run {
        args.push("--allow-run".to_string());
    }
    args
}

fn temp_module_path() -> std::path::PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("beeno-{millis}-{}.ts", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockProvider;

    #[test]
    fn classifier_detects_basic_code() {
        assert_eq!(classify_input("let x = 1;"), InputKind::Code);
    }

    #[test]
    fn classifier_detects_pseudocode() {
        assert_eq!(
            classify_input("create a map and then print every key and value."),
            InputKind::Pseudocode
        );
    }

    #[tokio::test]
    async fn policy_blocks_command_spawn() {
        let policy = DefaultRiskPolicy::default();
        let report = policy.analyze("new Deno.Command('ls')").await;
        assert_eq!(report.level, RiskLevel::Blocked);
    }

    #[tokio::test]
    async fn policy_marks_eval_as_risky() {
        let policy = DefaultRiskPolicy::default();
        let report = policy.analyze("eval('1 + 1')").await;
        assert_eq!(report.level, RiskLevel::Risky);
    }

    #[test]
    fn strip_fenced() {
        let body = "```nl\nprint hello\n```";
        assert_eq!(strip_fenced_nl(body), "print hello");
    }

    #[tokio::test]
    async fn summary_rolls() {
        let mut s = RollingContextSummarizer::new(2);
        s.update("let a = 1;").await;
        s.update("import x from 'y';").await;
        s.update("console.log(a)").await;
        let cur = s.current();
        assert!(cur.recent_intents.len() <= 2);
    }

    #[tokio::test]
    async fn prepare_source_translates_pseudocode() {
        let engine = Engine::new(MockProvider, DefaultRiskPolicy::default());
        let (source, translated, risk) = engine
            .prepare_source(
                "create an object and print it.",
                "eval",
                SessionSummary::default(),
                None,
            )
            .await
            .expect("translation should succeed");
        assert!(translated.is_some());
        assert!(source.contains("console.log"));
        assert_eq!(risk.level, RiskLevel::Safe);
    }

    #[tokio::test]
    async fn process_tagged_script_replaces_nl_block() {
        let engine = Engine::new(MockProvider, DefaultRiskPolicy::default());
        let script = r#"
const before = 1;
/*nl
print hello from nl
*/
const after = 2;
"#;
        let (processed, warnings) = engine
            .process_tagged_script(script, SessionSummary::default(), None)
            .await
            .expect("processing should succeed");
        assert!(warnings.is_empty());
        assert!(processed.contains("console.log"));
        assert!(processed.contains("const before = 1;"));
        assert!(processed.contains("const after = 2;"));
    }

    #[tokio::test]
    async fn execution_blocks_without_allow_net() {
        let req = ExecutionRequest {
            source: "await fetch('https://example.com')".to_string(),
            deno_permissions: crate::types::DenoPermissions::default(),
            origin: "eval".to_string(),
        };
        let err = execute_request(req)
            .await
            .expect_err("must block without allow-net");
        assert!(err.to_string().contains("--allow-net"));
    }
}
