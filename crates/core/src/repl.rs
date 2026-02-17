use crate::engine::{
    execute_request, ContextSummarizer, DefaultRiskPolicy, Engine, EngineError,
    RollingContextSummarizer,
};
use crate::providers::TranslatorProvider;
use crate::server::ServerManager;
use crate::types::{DenoPermissions, ExecutionRequest, ServerContext, SessionSummary};
use std::io::{self, Write};
use std::process::Command;

pub async fn run_repl<P: TranslatorProvider>(
    provider: P,
    confirm_risky: bool,
) -> anyhow::Result<()> {
    let policy = DefaultRiskPolicy::default();
    let engine = Engine::new(provider, policy);
    let mut summarizer = RollingContextSummarizer::new(8);
    let mut last_generated: Option<String> = None;
    let mut last_nl_input: Option<String> = None;
    let mut server_manager = ServerManager::default();
    let mut server_port: u16 = 8080;

    println!("Beeno REPL");
    println!("Type /help for commands. Use /exit to quit.");
    println!("Slash command layout is primary; ':' aliases still work.");
    loop {
        print!("beeno> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "/help" || line == ":help" {
            print_help();
            continue;
        }
        if line == "/exit" || line == "/quit" || line == ":exit" || line == ":quit" {
            break;
        }
        if line == "/clear" || line == ":clear" {
            print!("\x1B[2J\x1B[1;1H");
            io::stdout().flush()?;
            continue;
        }

        if line == "/show" || line == ":show" {
            if let Some(code) = &last_generated {
                println!("{code}");
            } else {
                println!("no generated code yet");
            }
            continue;
        }

        if line == "/context" || line == ":context" {
            let ctx = current_summary_with_server(&mut summarizer, &mut server_manager);
            println!("session summary: {ctx:?}");
            continue;
        }

        if line == "/serve-status" || line == ":serve-status" {
            if let Some(status) = server_manager.status() {
                println!("server running on {} (mode: {})", status.url, status.mode);
            } else {
                println!("server not running");
            }
            continue;
        }

        if line == "/serve-stop" || line == ":serve-stop" {
            server_manager.stop().await?;
            println!("server stopped");
            continue;
        }

        if let Some(value) = line
            .strip_prefix("/serve-port")
            .or_else(|| line.strip_prefix(":serve-port"))
        {
            let raw = value.trim();
            match raw.parse::<u16>() {
                Ok(port) if port > 0 => {
                    server_port = port;
                    println!("server port set to {server_port}");
                }
                _ => println!("invalid port; usage: /serve-port <1-65535>"),
            }
            continue;
        }

        if let Some(code) = line
            .strip_prefix("/serve-js")
            .or_else(|| line.strip_prefix(":serve-js"))
        {
            let src = code.trim();
            if src.is_empty() {
                println!("usage: /serve-js <server code>");
                continue;
            }
            match start_server_from_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                src,
                "force_js",
                server_port,
                "js",
            )
            .await
            {
                Ok(url) => {
                    println!("server started: {url}");
                    maybe_prompt_open_browser(&url)?;
                }
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        if let Some(text) = line
            .strip_prefix("/serve-nl")
            .or_else(|| line.strip_prefix(":serve-nl"))
        {
            let src = text.trim();
            if src.is_empty() {
                println!("usage: /serve-nl <pseudocode>\nexample: /serve-nl create an http server that returns hello world");
                continue;
            }
            match start_server_from_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                src,
                "force_nl",
                server_port,
                "nl",
            )
            .await
            {
                Ok(url) => {
                    last_nl_input = Some(src.to_string());
                    println!("server started: {url}");
                    maybe_prompt_open_browser(&url)?;
                }
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        if let Some(code) = line
            .strip_prefix("/serve-hotfix-js")
            .or_else(|| line.strip_prefix(":serve-hotfix-js"))
        {
            let src = code.trim();
            if src.is_empty() {
                println!("usage: /serve-hotfix-js <updated server code>");
                continue;
            }
            match hotfix_server_from_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                src,
                "force_js",
                "js-hotfix",
            )
            .await
            {
                Ok(url) => println!("server hotfix applied: {url}"),
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        if let Some(text) = line
            .strip_prefix("/serve-hotfix-nl")
            .or_else(|| line.strip_prefix(":serve-hotfix-nl"))
        {
            let src = text.trim();
            if src.is_empty() {
                println!("usage: /serve-hotfix-nl <pseudocode hotfix>");
                continue;
            }
            match hotfix_server_from_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                src,
                "force_nl",
                "nl-hotfix",
            )
            .await
            {
                Ok(url) => println!("server hotfix applied: {url}"),
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        if line.starts_with("/retry") || line.starts_with(":retry") {
            let hint = line
                .strip_prefix("/retry")
                .or_else(|| line.strip_prefix(":retry"))
                .unwrap_or("")
                .trim();
            let Some(previous) = &last_nl_input else {
                println!("no previous pseudocode input to retry");
                continue;
            };
            let retry_input = if hint.is_empty() {
                previous.clone()
            } else {
                format!("{previous}\nRefine with: {hint}")
            };
            match handle_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                &retry_input,
                "force_nl",
                confirm_risky,
                &mut last_generated,
                &mut last_nl_input,
            )
            .await
            {
                Ok(()) => {}
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        if let Some(code) = line
            .strip_prefix("/js")
            .or_else(|| line.strip_prefix(":js"))
        {
            let src = code.trim();
            match handle_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                src,
                "force_js",
                confirm_risky,
                &mut last_generated,
                &mut last_nl_input,
            )
            .await
            {
                Ok(()) => {}
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        if let Some(text) = line
            .strip_prefix("/nl")
            .or_else(|| line.strip_prefix(":nl"))
        {
            let src = text.trim();
            match handle_input(
                &engine,
                &mut summarizer,
                &mut server_manager,
                src,
                "force_nl",
                confirm_risky,
                &mut last_generated,
                &mut last_nl_input,
            )
            .await
            {
                Ok(()) => {}
                Err(e) => print_repl_error(e),
            }
            continue;
        }

        match handle_input(
            &engine,
            &mut summarizer,
            &mut server_manager,
            line,
            "repl",
            confirm_risky,
            &mut last_generated,
            &mut last_nl_input,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => print_repl_error(e),
        }
    }

    server_manager.stop().await?;
    Ok(())
}

async fn handle_input<P: TranslatorProvider>(
    engine: &Engine<P, DefaultRiskPolicy>,
    summarizer: &mut RollingContextSummarizer,
    server_manager: &mut ServerManager,
    input: &str,
    mode: &str,
    confirm_risky: bool,
    last_generated: &mut Option<String>,
    last_nl_input: &mut Option<String>,
) -> Result<(), EngineError> {
    let summary = current_summary_with_server(summarizer, server_manager);
    let (source, _translated, risk) = engine.prepare_source(input, mode, summary, None).await?;
    *last_generated = Some(source.clone());
    if mode == "force_nl" || mode == "repl" {
        *last_nl_input = Some(input.to_string());
    }

    if risk.requires_confirmation
        && confirm_risky
        && !prompt_confirm("risky output detected, execute?")?
    {
        println!("execution skipped by user");
        return Ok(());
    }

    execute_request(ExecutionRequest {
        source,
        deno_permissions: DenoPermissions::default(),
        origin: "repl".to_string(),
    })
    .await?;

    summarizer.update(input).await;
    Ok(())
}

async fn start_server_from_input<P: TranslatorProvider>(
    engine: &Engine<P, DefaultRiskPolicy>,
    summarizer: &mut RollingContextSummarizer,
    server_manager: &mut ServerManager,
    input: &str,
    mode: &str,
    port: u16,
    source_mode: &str,
) -> Result<String, EngineError> {
    let summary = current_summary_with_server(summarizer, server_manager);
    let (source, _, _risk) = engine.prepare_source(input, mode, summary, None).await?;
    let status = server_manager
        .start_with_code(source, port, source_mode)
        .await
        .map_err(|e| EngineError::Execution(e.to_string()))?;
    summarizer.update(input).await;
    Ok(status.url)
}

async fn hotfix_server_from_input<P: TranslatorProvider>(
    engine: &Engine<P, DefaultRiskPolicy>,
    summarizer: &mut RollingContextSummarizer,
    server_manager: &mut ServerManager,
    input: &str,
    mode: &str,
    source_mode: &str,
) -> Result<String, EngineError> {
    let summary = current_summary_with_server(summarizer, server_manager);
    let (source, _, _risk) = engine.prepare_source(input, mode, summary, None).await?;
    let status = server_manager
        .hotfix_with_code(source, source_mode)
        .await
        .map_err(|e| EngineError::Execution(e.to_string()))?;
    summarizer.update(input).await;
    Ok(status.url)
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

fn maybe_prompt_open_browser(url: &str) -> anyhow::Result<()> {
    if !prompt_confirm("open hosted webpage in your default browser?")? {
        return Ok(());
    }

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
        println!("failed to open browser automatically; open manually: {url}");
    }
    Ok(())
}

fn print_repl_error(err: EngineError) {
    match err {
        EngineError::Blocked(reasons) => {
            println!("blocked by policy:");
            for reason in reasons {
                println!("- {reason}");
            }
            println!("try /retry with a safer instruction or use /js to edit manually");
        }
        other => println!("error: {other}"),
    }
}

fn print_help() {
    println!("Beeno REPL Commands");
    println!("  /help                         show this help");
    println!("  /exit | /quit                 exit repl");
    println!("  /clear                        clear terminal");
    println!("  /js <code>                    force native JS/TS execution");
    println!("  /nl <prompt>                  force LLM translation before execution");
    println!("  /retry [hint]                 retry last NL prompt");
    println!("  /show                         show last generated code");
    println!("  /context                      show current session summary");
    println!("  /serve-port <port>            set background server port");
    println!("  /serve-js <code>              start/restart background server from JS/TS");
    println!("  /serve-nl <prompt>            start/restart background server from pseudocode");
    println!("  /serve-hotfix-js <code>       hotfix running server with JS/TS");
    println!("  /serve-hotfix-nl <prompt>     hotfix running server with pseudocode");
    println!("  /serve-status                 show running server state");
    println!("  /serve-stop                   stop running server");
}
