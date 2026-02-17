use beeno_core::engine::{execute_request, DefaultRiskPolicy, Engine, EngineError};
use beeno_core::providers::{
    HttpProvider, MockProvider, OllamaProvider, OpenAICompatProvider, TranslatorProvider,
};
use beeno_core::repl::run_repl;
use beeno_core::types::{
    AppConfig, DenoPermissions, ExecutionRequest, FileMetadata, JsonEnvelope, SessionSummary,
};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

#[derive(Debug, Parser)]
#[command(
    name = "beeno",
    version,
    about = "LLM-assisted pseudocode on top of Deno core"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    InitConfig {
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    Repl {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    Eval {
        input: String,
        #[arg(long = "allow-read")]
        allow_read: Vec<String>,
        #[arg(long = "allow-write")]
        allow_write: Vec<String>,
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        #[arg(long = "allow-env", default_value_t = false)]
        allow_env: bool,
        #[arg(long = "allow-run", default_value_t = false)]
        allow_run: bool,
    },
    Run {
        file: PathBuf,
        #[arg(long = "allow-read")]
        allow_read: Vec<String>,
        #[arg(long = "allow-write")]
        allow_write: Vec<String>,
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        #[arg(long = "allow-env", default_value_t = false)]
        allow_env: bool,
        #[arg(long = "allow-run", default_value_t = false)]
        allow_run: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Commands::InitConfig { force } => {
            init_config_file(Path::new(".beeno.toml"), force)?;
            println!("initialized .beeno.toml");
            return Ok(());
        }
        _ => {}
    }

    let mut cfg = load_config()?;

    match cli.cmd {
        Commands::InitConfig { .. } => {}
        Commands::Repl {
            provider,
            model,
            policy,
        } => {
            if let Some(p) = provider {
                cfg.llm.provider = p;
            }
            if let Some(m) = model {
                cfg.llm.model = m;
            }
            if let Some(path) = policy {
                cfg.policy.policy_path = Some(path.to_string_lossy().to_string());
            }

            let provider = build_provider(&cfg, |k| std::env::var(k).ok());
            run_repl(provider, cfg.policy.confirm_risky).await?;
        }
        Commands::Eval {
            input,
            allow_read,
            allow_write,
            allow_net,
            allow_env,
            allow_run,
        } => {
            execute_with_provider(
                &cfg,
                &input,
                "eval",
                None,
                DenoPermissions {
                    allow_read,
                    allow_write,
                    allow_net,
                    allow_env,
                    allow_run,
                },
                cli.json,
            )
            .await?;
        }
        Commands::Run {
            file,
            allow_read,
            allow_write,
            allow_net,
            allow_env,
            allow_run,
        } => {
            let script = fs::read_to_string(&file)?;
            execute_run_with_provider(
                &cfg,
                &script,
                file,
                DenoPermissions {
                    allow_read,
                    allow_write,
                    allow_net,
                    allow_env,
                    allow_run,
                },
                cli.json,
            )
            .await?;
        }
    }

    Ok(())
}

async fn execute_with_provider(
    cfg: &AppConfig,
    input: &str,
    mode: &str,
    file_metadata: Option<FileMetadata>,
    permissions: DenoPermissions,
    json_output: bool,
) -> anyhow::Result<()> {
    let provider = build_provider(cfg, |k| std::env::var(k).ok());
    execute_pipeline(
        Engine::new(provider, policy_from_cfg(cfg)?),
        input,
        mode,
        file_metadata,
        permissions,
        json_output,
    )
    .await
}

async fn execute_pipeline<P: TranslatorProvider>(
    engine: Engine<P, DefaultRiskPolicy>,
    input: &str,
    mode: &str,
    file_metadata: Option<FileMetadata>,
    permissions: DenoPermissions,
    json_output: bool,
) -> anyhow::Result<()> {
    let (source, _, risk) = engine
        .prepare_source(input, mode, SessionSummary::default(), file_metadata)
        .await
        .map_err(render_engine_error)?;

    if risk.requires_confirmation {
        eprintln!("risky output detected; add interactive repl to confirm.");
    }

    execute_request(ExecutionRequest {
        source,
        deno_permissions: permissions,
        origin: mode.to_string(),
    })
    .await
    .map_err(render_engine_error)?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&JsonEnvelope {
                status: "ok".to_string(),
                phase: "execute".to_string(),
                message: "execution completed".to_string(),
                details: json!({"mode": mode}),
            })?
        );
    }

    Ok(())
}

async fn execute_run_with_provider(
    cfg: &AppConfig,
    script: &str,
    file: PathBuf,
    permissions: DenoPermissions,
    json_output: bool,
) -> anyhow::Result<()> {
    let policy = policy_from_cfg(cfg)?;
    let provider = build_provider(cfg, |k| std::env::var(k).ok());
    let engine = Engine::new(provider, policy);
    let (processed, warnings) = engine
        .process_tagged_script(
            script,
            SessionSummary::default(),
            Some(file.to_string_lossy().to_string()),
        )
        .await
        .map_err(render_engine_error)?;
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
    execute_request(ExecutionRequest {
        source: processed,
        deno_permissions: permissions,
        origin: "run".to_string(),
    })
    .await
    .map_err(render_engine_error)?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&JsonEnvelope {
                status: "ok".to_string(),
                phase: "execute".to_string(),
                message: "run completed".to_string(),
                details: json!({"file": file}),
            })?
        );
    }

    Ok(())
}

fn build_provider<F>(cfg: &AppConfig, env_get: F) -> Box<dyn TranslatorProvider>
where
    F: Fn(&str) -> Option<String> + Copy,
{
    let provider = cfg.llm.provider.to_ascii_lowercase();
    let endpoint = resolve_provider_endpoint(cfg, env_get);
    let api_key = env_get(&cfg.llm.api_key_env_var);

    match provider.as_str() {
        "mock" => Box::new(MockProvider),
        "ollama" => Box::new(OllamaProvider::new(
            endpoint.unwrap_or_else(|| "http://127.0.0.1:11434/api/generate".to_string()),
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        "chatgpt" => Box::new(OpenAICompatProvider::new(
            endpoint.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        "openrouter" => Box::new(OpenAICompatProvider::new(
            endpoint.unwrap_or_else(|| "https://openrouter.ai/api/v1/chat/completions".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        "openai_compat" => Box::new(OpenAICompatProvider::new(
            endpoint.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        _ => Box::new(HttpProvider::new(
            endpoint.unwrap_or_else(|| "http://localhost:8080/translate".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
    }
}

fn resolve_provider_endpoint<F>(cfg: &AppConfig, env_get: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    cfg.llm
        .endpoint
        .clone()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env_get(&cfg.llm.endpoint_env_var))
        .filter(|v| !v.trim().is_empty())
}

fn policy_from_cfg(cfg: &AppConfig) -> anyhow::Result<DefaultRiskPolicy> {
    if let Some(path) = &cfg.policy.policy_path {
        if path.trim().is_empty() {
            return Ok(DefaultRiskPolicy::default());
        }
        DefaultRiskPolicy::from_path(Path::new(path))
    } else {
        Ok(DefaultRiskPolicy::default())
    }
}

fn load_config() -> anyhow::Result<AppConfig> {
    let local_path = PathBuf::from(".beeno.toml");
    let home_path = std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".beeno.toml"));

    let home = match &home_path {
        Some(path) => read_config_value(path)?,
        None => None,
    };
    let local = read_config_value(&local_path)?;

    resolve_config(home, local, |k| std::env::var(k).ok())
}

fn resolve_config<F>(
    home: Option<Value>,
    local: Option<Value>,
    env_get: F,
) -> anyhow::Result<AppConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let mut merged = Value::try_from(AppConfig::default())?;
    if let Some(home_value) = home {
        merge_toml(&mut merged, home_value);
    }
    if let Some(local_value) = local {
        merge_toml(&mut merged, local_value);
    }

    let mut cfg: AppConfig = merged.try_into()?;
    apply_env_overrides(&mut cfg, env_get);
    Ok(cfg)
}

fn read_config_value(path: &Path) -> anyhow::Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)?;
    let parsed = raw.parse::<Value>()?;
    Ok(Some(parsed))
}

fn merge_toml(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Table(base_map), Value::Table(overlay_map)) => {
            for (key, value) in overlay_map {
                if let Some(base_value) = base_map.get_mut(&key) {
                    merge_toml(base_value, value);
                } else {
                    base_map.insert(key, value);
                }
            }
        }
        (base_value, overlay_value) => {
            *base_value = overlay_value;
        }
    }
}

fn apply_env_overrides<F>(cfg: &mut AppConfig, env_get: F)
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(v) = env_get("BEENO_PROVIDER") {
        cfg.llm.provider = v;
    }
    if let Some(v) = env_get("BEENO_MODEL") {
        cfg.llm.model = v;
    }
    if let Some(v) = env_get("BEENO_ENDPOINT") {
        cfg.llm.endpoint = Some(v);
    }
    if let Some(v) = env_get("BEENO_TEMPERATURE").and_then(|v| v.parse::<f32>().ok()) {
        cfg.llm.temperature = v;
    }
    if let Some(v) = env_get("BEENO_MAX_TOKENS").and_then(|v| v.parse::<u32>().ok()) {
        cfg.llm.max_tokens = v;
    }
    if let Some(v) = env_get("BEENO_ENDPOINT_ENV_VAR") {
        cfg.llm.endpoint_env_var = v;
    }
    if let Some(v) = env_get("BEENO_API_KEY_ENV_VAR") {
        cfg.llm.api_key_env_var = v;
    }

    if let Some(v) = env_get("BEENO_POLICY_PATH") {
        cfg.policy.policy_path = Some(v);
    }
    if let Some(v) = env_get("BEENO_CONFIRM_RISKY").and_then(|v| parse_bool(&v)) {
        cfg.policy.confirm_risky = v;
    }

    if let Some(v) = env_get("BEENO_SELF_HEAL_ENABLED").and_then(|v| parse_bool(&v)) {
        cfg.self_heal.enabled = v;
    }
    if let Some(v) = env_get("BEENO_SELF_HEAL_AUTO_ON_RUN_FAILURE").and_then(|v| parse_bool(&v)) {
        cfg.self_heal.auto_on_run_failure = v;
    }
    if let Some(v) = env_get("BEENO_APPLY_FIXES_DEFAULT").and_then(|v| parse_bool(&v)) {
        cfg.self_heal.apply_fixes_default = v;
    }
    if let Some(v) = env_get("BEENO_SELF_HEAL_MAX_ATTEMPTS").and_then(|v| v.parse::<u8>().ok()) {
        cfg.self_heal.max_attempts = v;
    }

    if let Some(v) = env_get("BEENO_ARTIFACT_DIR") {
        cfg.artifacts.dir = v;
    }
    if let Some(v) = env_get("BEENO_ARTIFACT_KEEP_LAST").and_then(|v| v.parse::<usize>().ok()) {
        cfg.artifacts.keep_last = v;
    }

    if let Some(v) = env_get("BEENO_MAX_FILES").and_then(|v| v.parse::<usize>().ok()) {
        cfg.limits.max_files = v;
    }
    if let Some(v) = env_get("BEENO_MAX_CHANGED_LINES").and_then(|v| v.parse::<usize>().ok()) {
        cfg.limits.max_changed_lines = v;
    }

    if let Some(v) = env_get("BEENO_PROTECT_DENY") {
        cfg.protect.deny = v
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn init_config_file(path: &Path, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; re-run with --force to overwrite",
            path.display()
        );
    }
    fs::write(path, config_template())?;
    Ok(())
}

fn config_template() -> &'static str {
    r#"# beeno configuration
# precedence: CLI > env > local .beeno.toml > home ~/.beeno.toml > defaults

[llm]
# provider options: http, mock, ollama, chatgpt, openrouter, openai_compat
provider = "http"
# optional explicit endpoint override (for custom URLs / OpenAI-compatible gateways)
endpoint = ""
model = "gpt-4.1-mini"
temperature = 0.1
max_tokens = 512
endpoint_env_var = "DENO_NL_ENDPOINT"
api_key_env_var = "DENO_NL_API_KEY"

[policy]
policy_path = ""
confirm_risky = true

[self_heal]
enabled = true
auto_on_run_failure = true
apply_fixes_default = false
max_attempts = 3

[artifacts]
dir = ".beeno/suggestions"
keep_last = 20

[limits]
max_files = 10
max_changed_lines = 500

[protect]
deny = [".env", ".env.*", "deno.lock", "Cargo.lock", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"]
"#
}

fn render_engine_error(err: EngineError) -> anyhow::Error {
    match err {
        EngineError::Blocked(reasons) => {
            anyhow::anyhow!(
                "blocked by policy: {}; retry with safer instructions",
                reasons.join(", ")
            )
        }
        other => anyhow::anyhow!(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn config_precedence_cli_env_local_home_defaults() {
        let home = Some(
            r#"
            [llm]
            model = "home-model"

            [artifacts]
            keep_last = 99
            "#
            .parse::<Value>()
            .expect("home parse"),
        );

        let local = Some(
            r#"
            [llm]
            model = "local-model"

            [policy]
            confirm_risky = false
            "#
            .parse::<Value>()
            .expect("local parse"),
        );

        let env = HashMap::from([
            ("BEENO_MODEL".to_string(), "env-model".to_string()),
            ("BEENO_PROVIDER".to_string(), "mock".to_string()),
        ]);

        let cfg = resolve_config(home, local, |k| env.get(k).cloned()).expect("resolve config");

        assert_eq!(cfg.llm.model, "env-model");
        assert_eq!(cfg.llm.provider, "mock");
        assert!(!cfg.policy.confirm_risky);
        assert_eq!(cfg.artifacts.keep_last, 99);
    }

    #[test]
    fn init_config_requires_force_to_overwrite() {
        let base = std::env::temp_dir().join(format!(
            "beeno-cli-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&base).expect("create temp dir");
        let cfg_path = base.join(".beeno.toml");

        init_config_file(&cfg_path, false).expect("must create first config");
        let err = init_config_file(&cfg_path, false).expect_err("must reject overwrite");
        assert!(err.to_string().contains("--force"));

        init_config_file(&cfg_path, true).expect("force overwrite should succeed");
        let content = fs::read_to_string(&cfg_path).expect("read config");
        assert!(content.contains("[self_heal]"));

        fs::remove_dir_all(&base).expect("cleanup temp dir");
    }

    #[test]
    fn provider_endpoint_prefers_config_then_env() {
        let mut cfg = AppConfig::default();
        cfg.llm.endpoint = Some("https://example.invalid/v1/chat/completions".to_string());
        cfg.llm.endpoint_env_var = "CUSTOM_ENDPOINT".to_string();

        let env = HashMap::from([(
            "CUSTOM_ENDPOINT".to_string(),
            "https://env.invalid/v1/chat/completions".to_string(),
        )]);

        let endpoint = resolve_provider_endpoint(&cfg, |k| env.get(k).cloned());
        assert_eq!(
            endpoint.as_deref(),
            Some("https://example.invalid/v1/chat/completions")
        );

        cfg.llm.endpoint = Some("".to_string());
        let endpoint = resolve_provider_endpoint(&cfg, |k| env.get(k).cloned());
        assert_eq!(
            endpoint.as_deref(),
            Some("https://env.invalid/v1/chat/completions")
        );
    }

    #[test]
    fn empty_policy_path_uses_default_policy() {
        let mut cfg = AppConfig::default();
        cfg.policy.policy_path = Some("".to_string());
        let result = policy_from_cfg(&cfg);
        assert!(result.is_ok());
    }
}
