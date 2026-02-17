use beeno_core::engine::{
    execute_request, ContextSummarizer, DefaultRiskPolicy, Engine, EngineError,
    RollingContextSummarizer,
};
#[cfg(feature = "provider-http")]
use beeno_core::providers::HttpProvider;
#[cfg(feature = "provider-ollama")]
use beeno_core::providers::OllamaProvider;
#[cfg(feature = "provider-openai-compat")]
use beeno_core::providers::OpenAICompatProvider;
use beeno_core::providers::{MockProvider, TranslatorProvider};
use beeno_core::repl::run_repl;
use beeno_core::server::ServerManager;
use beeno_core::types::{
    AppConfig, DenoPermissions, ExecutionRequest, FileMetadata, JsonEnvelope, ServerContext,
    SessionSummary,
};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
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
    Dev {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value_t = 8080)]
        port: u16,
        #[arg(long, default_value_t = false)]
        open: bool,
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
        Commands::Dev { file, port, open } => {
            run_dev_with_provider(&cfg, file, port, open).await?;
        }
    }

    Ok(())
}

async fn run_dev_with_provider(
    cfg: &AppConfig,
    file: Option<PathBuf>,
    port: u16,
    open: bool,
) -> anyhow::Result<()> {
    let provider = build_provider(cfg, |k| std::env::var(k).ok());
    let engine = Engine::new(provider, policy_from_cfg(cfg)?);
    let mut summarizer = RollingContextSummarizer::new(cfg.repl.summary_window);
    let mut server_manager = ServerManager::default();

    let (initial_code, mode) = match file {
        Some(path) => {
            let script = fs::read_to_string(&path)?;
            if script.contains("/*nl") {
                let (processed, warnings) = engine
                    .process_tagged_script(
                        &script,
                        current_summary_with_server(&mut summarizer, &mut server_manager),
                        Some(path.to_string_lossy().to_string()),
                    )
                    .await
                    .map_err(render_engine_error)?;
                for warning in warnings {
                    eprintln!("warning: {warning}");
                }
                (processed, "file-nl".to_string())
            } else {
                (script, "file".to_string())
            }
        }
        None => (default_dev_server_source(), "scaffold".to_string()),
    };

    let status = server_manager
        .start_with_code(initial_code, port, &mode)
        .await?;
    println!("Beeno Dev");
    println!("server running at {}", status.url);
    println!("type /help for dev commands");

    if open {
        open_in_browser(&status.url)?;
    } else if prompt_confirm("open hosted webpage in your default browser?")? {
        open_in_browser(&status.url)?;
    }

    loop {
        print!("dev> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "/help" {
            print_dev_help();
            continue;
        }

        if line == "/quit" || line == "/exit" {
            break;
        }

        if line == "/status" {
            if let Some(s) = server_manager.status() {
                println!("running: {} ({})", s.url, s.mode);
            } else {
                println!("server is stopped");
            }
            continue;
        }

        if line == "/open" {
            if let Some(s) = server_manager.status() {
                open_in_browser(&s.url)?;
            } else {
                println!("server is stopped");
            }
            continue;
        }

        if line == "/stop" {
            server_manager.stop().await?;
            println!("server stopped");
            continue;
        }

        if line == "/start" {
            let Some(source) = server_manager.last_source() else {
                println!("no previous server source available");
                continue;
            };
            let s = server_manager
                .start_with_code(source, port, "restart")
                .await?;
            println!("server started: {}", s.url);
            continue;
        }

        if line == "/restart" {
            let Some(source) = server_manager.last_source() else {
                println!("no previous server source available");
                continue;
            };
            let s = server_manager
                .start_with_code(source, port, "restart")
                .await?;
            println!("server restarted: {}", s.url);
            continue;
        }

        if let Some(code) = line.strip_prefix("/hotfix-js") {
            let src = code.trim();
            if src.is_empty() {
                println!("usage: /hotfix-js <code>");
                continue;
            }
            let s = server_manager
                .hotfix_with_code(src.to_string(), "js-hotfix")
                .await?;
            summarizer.update(src).await;
            println!("hotfix applied: {}", s.url);
            continue;
        }

        if let Some(prompt) = line.strip_prefix("/hotfix-nl") {
            let src = prompt.trim();
            if src.is_empty() {
                println!("usage: /hotfix-nl <prompt>");
                continue;
            }
            let summary = current_summary_with_server(&mut summarizer, &mut server_manager);
            let (code, _, risk) = engine
                .prepare_source(src, "force_nl", summary, None)
                .await
                .map_err(render_engine_error)?;
            if risk.requires_confirmation
                && cfg.policy.confirm_risky
                && !prompt_confirm("risky hotfix generated, apply?")?
            {
                println!("hotfix skipped");
                continue;
            }
            let s = server_manager.hotfix_with_code(code, "nl-hotfix").await?;
            summarizer.update(src).await;
            println!("hotfix applied: {}", s.url);
            continue;
        }

        println!("unknown command: {line}. try /help");
    }

    server_manager.stop().await?;
    Ok(())
}

fn default_dev_server_source() -> String {
    r#"const port = Number(Deno.env.get("PORT") ?? "8080");
Deno.serve({ port }, () => new Response("Beeno dev server running"));
console.log(`dev server listening on http://127.0.0.1:${port}`);"#
        .to_string()
}

fn print_dev_help() {
    println!("Beeno Dev Commands");
    println!("  /help                    show command list");
    println!("  /status                  show server status");
    println!("  /open                    open current server URL in browser");
    println!("  /restart                 restart server with current source");
    println!("  /hotfix-js <code>        hotfix server using JS/TS");
    println!("  /hotfix-nl <prompt>      hotfix server using LLM translation");
    println!("  /stop                    stop server");
    println!("  /start                   start stopped server with last source");
    println!("  /quit                    exit dev mode");
}

fn current_summary_with_server(
    summarizer: &mut RollingContextSummarizer,
    server_manager: &mut ServerManager,
) -> SessionSummary {
    let mut summary = summarizer.current();
    summary.server = server_manager.status().map(|status| ServerContext {
        running: status.running,
        url: Some(status.url),
        port: Some(status.port),
        mode: status.mode,
    });
    summary
}

fn prompt_confirm(prompt: &str) -> anyhow::Result<bool> {
    print!("{prompt} [y/N]: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES"))
}

fn open_in_browser(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };

    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", url]);
        c
    };

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("failed to open browser automatically; open manually: {url}");
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
        #[cfg(feature = "provider-ollama")]
        "ollama" => Box::new(OllamaProvider::new(
            endpoint.unwrap_or_else(|| "http://127.0.0.1:11434/api/generate".to_string()),
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        #[cfg(feature = "provider-openai-compat")]
        "chatgpt" => Box::new(OpenAICompatProvider::new(
            endpoint.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        #[cfg(feature = "provider-openai-compat")]
        "openrouter" => Box::new(OpenAICompatProvider::new(
            endpoint.unwrap_or_else(|| "https://openrouter.ai/api/v1/chat/completions".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        #[cfg(feature = "provider-openai-compat")]
        "openai_compat" => Box::new(OpenAICompatProvider::new(
            endpoint.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        #[cfg(feature = "provider-http")]
        _ => Box::new(HttpProvider::new(
            endpoint.unwrap_or_else(|| "http://localhost:8080/translate".to_string()),
            api_key,
            cfg.llm.model.clone(),
            cfg.llm.temperature,
            cfg.llm.max_tokens,
        )),
        #[cfg(not(feature = "provider-http"))]
        _ => Box::new(MockProvider),
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
    use clap::Parser;
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

    #[test]
    fn dev_command_parses_flags() {
        let cli = Cli::try_parse_from([
            "beeno", "dev", "--file", "app.ts", "--port", "3333", "--open",
        ])
        .expect("cli parse");

        match cli.cmd {
            Commands::Dev { file, port, open } => {
                assert_eq!(file, Some(PathBuf::from("app.ts")));
                assert_eq!(port, 3333);
                assert!(open);
            }
            _ => panic!("expected dev command"),
        }
    }

    #[test]
    fn default_dev_source_contains_deno_serve() {
        let src = default_dev_server_source();
        assert!(src.contains("Deno.serve"));
        assert!(src.contains("PORT"));
    }
}
