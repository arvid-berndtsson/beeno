use crate::engine::{
    execute_request, ContextSummarizer, DefaultRiskPolicy, Engine, EngineError,
    RollingContextSummarizer,
};
use crate::providers::TranslatorProvider;
use crate::types::{DenoPermissions, ExecutionRequest, SessionSummary};
use std::io::{self, Write};

pub async fn run_repl<P: TranslatorProvider>(
    provider: P,
    confirm_risky: bool,
) -> anyhow::Result<()> {
    let policy = DefaultRiskPolicy::default();
    let engine = Engine::new(provider, policy);
    let mut summarizer = RollingContextSummarizer::new(8);
    let mut last_generated: Option<String> = None;
    let mut last_nl_input: Option<String> = None;

    println!("beeno repl (type :exit to quit)");
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == ":exit" || line == ":quit" {
            break;
        }

        if line == ":show" {
            if let Some(code) = &last_generated {
                println!("{code}");
            } else {
                println!("no generated code yet");
            }
            continue;
        }

        if line == ":context" {
            let ctx = summarizer.current();
            println!("session summary: {ctx:?}");
            continue;
        }

        if line.starts_with(":retry") {
            let hint = line.strip_prefix(":retry").unwrap_or("").trim();
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

        if let Some(code) = line.strip_prefix(":js") {
            let src = code.trim();
            match handle_input(
                &engine,
                &mut summarizer,
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

        if let Some(text) = line.strip_prefix(":nl") {
            let src = text.trim();
            match handle_input(
                &engine,
                &mut summarizer,
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

    Ok(())
}

async fn handle_input<P: TranslatorProvider>(
    engine: &Engine<P, DefaultRiskPolicy>,
    summarizer: &mut RollingContextSummarizer,
    input: &str,
    mode: &str,
    confirm_risky: bool,
    last_generated: &mut Option<String>,
    last_nl_input: &mut Option<String>,
) -> Result<(), EngineError> {
    let summary: SessionSummary = summarizer.current();
    let (source, _translated, risk) = engine.prepare_source(input, mode, summary, None).await?;
    *last_generated = Some(source.clone());
    if mode == "force_nl" || mode == "repl" {
        *last_nl_input = Some(input.to_string());
    }

    if risk.requires_confirmation && confirm_risky && !prompt_confirm()? {
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

fn prompt_confirm() -> anyhow::Result<bool> {
    print!("risky output detected, execute? [y/N]: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES"))
}

fn print_repl_error(err: EngineError) {
    match err {
        EngineError::Blocked(reasons) => {
            println!("blocked by policy:");
            for reason in reasons {
                println!("- {reason}");
            }
            println!("try :retry with a safer instruction or use :js to edit manually");
        }
        other => println!("error: {other}"),
    }
}
