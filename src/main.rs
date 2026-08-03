mod cli;
mod config;
mod context;
mod exec;
mod history;
mod prompt;
mod provider;
mod risk;
mod shell;
mod ui;

use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use cli::Cli;
use config::{Config, ProviderKind};
use context::Context;
use history::Outcome;
use provider::{Proposal, Provider, Request, Risk};
use shell::ShellKind;
use ui::Decision;

#[tokio::main]
async fn main() {
    match run().await {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("koe: {err:#}");
            std::process::exit(1);
        }
    }
}

/// `init` and `config` are matched before clap sees the arguments.
///
/// koe's positional argument is free-form text, so a real clap subcommand would
/// make `koe config nginx for me` ambiguous. Matching the exact shapes here
/// keeps every other phrasing available as a task.
fn subcommand(args: &[String]) -> Option<Result<i32>> {
    match args.first()?.as_str() {
        "init" if args.len() <= 2 => Some(print_init_script(args.get(1).map(|s| s.as_str()))),
        "config" if args.len() == 1 => Some(show_config()),
        _ => None,
    }
}

fn print_init_script(kind: Option<&str>) -> Result<i32> {
    let Some(kind) = kind else {
        return Err(anyhow!(
            "usage: koe init <zsh|bash|fish>\n\nAdd the output to your shell config:\n  {}",
            ShellKind::Zsh.rc_hint()
        ));
    };
    let kind = ShellKind::from_str(kind, true)
        .map_err(|_| anyhow!("unsupported shell {kind:?}; expected zsh, bash, or fish"))?;
    print!("{}", kind.init_script());
    Ok(0)
}

fn show_config() -> Result<i32> {
    let config = Config::load()?;
    let path = Config::path();

    println!(
        "config file:  {}",
        match &path {
            Some(p) if p.exists() => p.display().to_string(),
            Some(p) => format!("{} (not created yet)", p.display()),
            None => "unavailable".to_string(),
        }
    );
    println!("provider:     {:?}", config.provider);
    println!("model:        {}", config.resolved_model());
    if config.provider == ProviderKind::Openai {
        println!("base url:     {}", config.resolved_base_url());
    }
    println!(
        "api key:      {}",
        match config.api_key() {
            Ok(Some(_)) => "set",
            Ok(None) => "not set (fine for a local server)",
            Err(_) => "not set",
        }
    );
    println!(
        "context:      git={} files={} tools={}",
        config.context.git, config.context.files, config.context.tools
    );
    println!(
        "history log:  {}",
        match (config.log_history, history::path()) {
            (true, Some(p)) => p.display().to_string(),
            (true, None) => "unavailable".to_string(),
            (false, _) => "disabled".to_string(),
        }
    );
    if let Some(p) = prompt::user_examples_path() {
        println!(
            "examples:     {}{}",
            p.display(),
            if p.exists() { "" } else { " (not created yet)" }
        );
    }
    Ok(0)
}

fn build_provider(config: &Config) -> Result<Box<dyn Provider>> {
    let model = config.resolved_model();
    let api_key = config.api_key()?;
    Ok(match config.provider {
        ProviderKind::Gemini => Box::new(provider::gemini::GeminiProvider::new(
            api_key.ok_or_else(|| anyhow!("GEMINI_API_KEY is not set"))?,
            model,
        )),
        ProviderKind::Openai => Box::new(provider::openai::OpenAiProvider::new(
            config.resolved_base_url(),
            api_key,
            model,
            config.json_mode,
        )?),
    })
}

/// Appends one line per invocation to the history log, if enabled.
struct Logger<'a> {
    enabled: bool,
    ctx: &'a Context,
    model: &'a str,
    task: &'a str,
}

impl Logger<'_> {
    fn record(
        &self,
        proposed: Option<&str>,
        ran: Option<&str>,
        outcome: Outcome,
        risk: Option<Risk>,
        exit_code: Option<i32>,
    ) {
        if !self.enabled {
            return;
        }
        let entry = history::Entry {
            ts: history::now(),
            cwd: &self.ctx.cwd,
            os: self.ctx.os,
            shell: &self.ctx.shell,
            model: self.model,
            task: self.task,
            proposed,
            ran,
            outcome,
            risk: risk.map(|r| r.label()),
            exit_code,
        };
        if let Err(e) = history::append(&entry) {
            eprintln!("{}", ui::dim(&format!("koe: could not write history ({e})")));
        }
    }
}

async fn run() -> Result<i32> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = subcommand(&argv) {
        return result;
    }

    let cli = Cli::parse();

    let mut config = Config::load()?;
    if let Some(provider) = cli.provider {
        config.provider = provider;
    }
    if let Some(model) = &cli.model {
        config.model = Some(model.clone());
    }
    if let Some(base_url) = &cli.base_url {
        config.base_url = Some(base_url.clone());
    }

    let task = cli.task();
    if task.is_empty() {
        eprintln!("Usage: koe <task>");
        eprintln!("   e.g. koe how many files in my downloads folder");
        eprintln!("Run `koe --help` for options.");
        return Ok(2);
    }

    let ctx = Context::gather(&config.context);
    let request = Request {
        system_prompt: prompt::system_prompt(&ctx),
        examples: prompt::examples()?,
        task: task.clone(),
    };

    let backend = build_provider(&config)?;
    let model = backend.describe();
    let log = Logger {
        enabled: config.log_history,
        ctx: &ctx,
        model: &model,
        task: &task,
    };

    let spinner = ui::Spinner::start("Thinking...");
    let proposal = backend.propose(&request).await;
    spinner.stop().await;

    let (command, model_risk, explanation) = match proposal? {
        Proposal::Unknown => {
            eprintln!(
                "Koe could not turn that into a command it trusts. Try rephrasing with more detail."
            );
            log.record(None, None, Outcome::Unknown, None, None);
            return Ok(1);
        }
        Proposal::Clarify { question } => {
            eprintln!("{question}");
            log.record(None, None, Outcome::Clarify, None, None);
            return Ok(1);
        }
        Proposal::Command {
            command,
            risk,
            explanation,
        } => (command, risk, explanation),
    };

    // The model's own risk rating is a hint; the static classifier is the check.
    let assessment = risk::classify(&command);
    let effective_risk = model_risk.max(assessment.risk);

    ui::show_proposal(&command, effective_risk, &assessment.reasons, &explanation);

    if cli.dry_run {
        println!("{command}");
        log.record(
            Some(&command),
            None,
            Outcome::DryRun,
            Some(effective_risk),
            None,
        );
        return Ok(0);
    }

    let decision = if cli.skips_confirmation(effective_risk, config.auto_run) {
        Decision::Run
    } else {
        ui::confirm()?
    };

    let final_command = match decision {
        Decision::Run => command.clone(),
        Decision::Edit(edited) => edited,
        Decision::Cancel => {
            eprintln!("Not executed.");
            log.record(
                Some(&command),
                None,
                Outcome::Cancelled,
                Some(effective_risk),
                None,
            );
            // Under `koe init`, empty stdout tells the wrapper to do nothing;
            // a nonzero exit would make it report a spurious failure.
            return Ok(if cli.print { 0 } else { 1 });
        }
    };

    let was_edited = final_command != command;

    if cli.print {
        println!("{final_command}");
        log.record(
            Some(&command),
            Some(&final_command),
            if was_edited {
                Outcome::Edited
            } else {
                Outcome::Printed
            },
            Some(effective_risk),
            None,
        );
        return Ok(0);
    }

    let exit_code = exec::run(&final_command)?;
    log.record(
        Some(&command),
        Some(&final_command),
        if was_edited {
            Outcome::Edited
        } else {
            Outcome::Ran
        },
        Some(effective_risk),
        Some(exit_code),
    );
    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recognizes_the_two_subcommands() {
        assert!(subcommand(&args(&["init", "zsh"])).is_some());
        assert!(subcommand(&args(&["config"])).is_some());
    }

    #[test]
    fn free_form_tasks_are_not_mistaken_for_subcommands() {
        // These must reach the model, not the subcommand dispatcher.
        assert!(subcommand(&args(&["config", "nginx", "for", "me"])).is_none());
        assert!(subcommand(&args(&["init", "a", "git", "repo"])).is_none());
        assert!(subcommand(&args(&["list", "files"])).is_none());
        assert!(subcommand(&args(&[])).is_none());
    }

    #[test]
    fn init_without_a_shell_explains_itself() {
        let err = print_init_script(None).unwrap_err();
        assert!(format!("{err}").contains("zsh|bash|fish"));
    }

    #[test]
    fn init_rejects_an_unknown_shell() {
        assert!(print_init_script(Some("tcsh")).is_err());
    }
}
