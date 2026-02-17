use beeno_core::engine::{execute_request, DefaultRiskPolicy, Engine, EngineError};
use beeno_core::providers::{HttpProvider, MockProvider, TranslatorProvider};
use beeno_core::repl::run_repl;
use beeno_core::types::{
    AppConfig, DenoPermissions, ExecutionRequest, FileMetadata, JsonEnvelope, SessionSummary,
};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

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
    let mut cfg = load_config()?;

    match cli.cmd {
        Commands::Repl {
            provider,
            model,
            policy,
        } => {
            if let Some(p) = provider {
                cfg.provider = p;
            }
            if let Some(m) = model {
                cfg.model = m;
            }
            if let Some(path) = policy {
                cfg.policy_path = Some(path.to_string_lossy().to_string());
            }

            if cfg.provider == "mock" {
                run_repl(MockProvider, cfg.confirm_risky).await?;
            } else {
                let endpoint = std::env::var("DENO_NL_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:8080/translate".to_string());
                let api_key = std::env::var("DENO_NL_API_KEY").ok();
                let provider = HttpProvider::new(
                    endpoint,
                    api_key,
                    cfg.model.clone(),
                    cfg.temperature,
                    cfg.max_tokens,
                );
                run_repl(provider, cfg.confirm_risky).await?;
            }
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
    if cfg.provider == "mock" {
        execute_pipeline(
            Engine::new(MockProvider, policy_from_cfg(cfg)?),
            input,
            mode,
            file_metadata,
            permissions,
            json_output,
        )
        .await
    } else {
        let endpoint = std::env::var("DENO_NL_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8080/translate".to_string());
        let api_key = std::env::var("DENO_NL_API_KEY").ok();
        let provider = HttpProvider::new(
            endpoint,
            api_key,
            cfg.model.clone(),
            cfg.temperature,
            cfg.max_tokens,
        );
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
    if cfg.provider == "mock" {
        let engine = Engine::new(MockProvider, policy);
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
    } else {
        let endpoint = std::env::var("DENO_NL_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8080/translate".to_string());
        let api_key = std::env::var("DENO_NL_API_KEY").ok();
        let provider = HttpProvider::new(
            endpoint,
            api_key,
            cfg.model.clone(),
            cfg.temperature,
            cfg.max_tokens,
        );
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
    }

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

fn policy_from_cfg(cfg: &AppConfig) -> anyhow::Result<DefaultRiskPolicy> {
    if let Some(path) = &cfg.policy_path {
        DefaultRiskPolicy::from_path(Path::new(path))
    } else {
        Ok(DefaultRiskPolicy::default())
    }
}

fn load_config() -> anyhow::Result<AppConfig> {
    let local_path = PathBuf::from(".beeno.toml");
    if local_path.exists() {
        return Ok(toml::from_str(&fs::read_to_string(local_path)?)?);
    }

    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home).join(".beeno.toml");
        if home_path.exists() {
            return Ok(toml::from_str(&fs::read_to_string(home_path)?)?);
        }
    }

    Ok(AppConfig::default())
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
