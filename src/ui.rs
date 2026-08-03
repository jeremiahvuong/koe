//! Terminal interaction.
//!
//! Everything here writes to **stderr**. stdout carries only the command
//! itself (in `--print` and `--dry-run` modes) so that `koe ... | pbcopy` and
//! the shell wrapper installed by `koe init` both work.

use crate::provider::Risk;
use anyhow::Result;
use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn stderr_is_tty() -> bool {
    std::io::stderr().is_terminal()
}

fn paint(text: &str, code: &str) -> String {
    if stderr_is_tty() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    paint(text, "1")
}

pub fn dim(text: &str) -> String {
    paint(text, "2")
}

/// A "Thinking…" indicator, shown only when stderr is a terminal.
pub struct Spinner {
    running: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Spinner {
    pub fn start(message: &'static str) -> Self {
        if !stderr_is_tty() {
            return Self {
                running: Arc::new(AtomicBool::new(false)),
                handle: None,
            };
        }
        let running = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&running);
        let handle = tokio::spawn(async move {
            let mut frame = 0usize;
            while flag.load(Ordering::Relaxed) {
                let mut err = std::io::stderr();
                let _ = write!(err, "\r{} {message} ", SPINNER_FRAMES[frame]);
                let _ = err.flush();
                frame = (frame + 1) % SPINNER_FRAMES.len();
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            }
        });
        Self {
            running,
            handle: Some(handle),
        }
    }

    pub async fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
            let mut err = std::io::stderr();
            // Return to column 0 and clear to end of line.
            let _ = write!(err, "\r\x1b[K");
            let _ = err.flush();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Run,
    /// The user rewrote the command; the replacement is the payload. This is
    /// also the most valuable signal in the history log.
    Edit(String),
    Cancel,
}

/// Print the proposed command and the reasons it was flagged.
pub fn show_proposal(command: &str, risk: Risk, reasons: &[String], explanation: &str) {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "{}", bold(command));

    if !explanation.is_empty() {
        let _ = writeln!(err, "{}", dim(explanation));
    }
    if risk > Risk::Safe && !reasons.is_empty() {
        let _ = writeln!(
            err,
            "{} {}",
            paint(&format!("[{}]", risk.label()), risk.color()),
            dim(&reasons.join("; "))
        );
    }
}

/// Ask whether to run the command. Returns [`Decision::Cancel`] on EOF or when
/// stdin is not a terminal, rather than looping forever on unreadable input.
pub fn confirm() -> Result<Decision> {
    if !std::io::stdin().is_terminal() {
        eprintln!("Not a terminal; refusing to run unattended. Pass --yes to allow this.");
        return Ok(Decision::Cancel);
    }

    loop {
        eprint!("Execute? [y/N/e] ");
        std::io::stderr().flush()?;

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input)? == 0 {
            eprintln!();
            return Ok(Decision::Cancel);
        }

        match input.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(Decision::Run),
            "" | "n" | "no" => return Ok(Decision::Cancel),
            "e" | "edit" => {
                eprint!("Command: ");
                std::io::stderr().flush()?;
                let mut edited = String::new();
                if std::io::stdin().read_line(&mut edited)? == 0 {
                    eprintln!();
                    return Ok(Decision::Cancel);
                }
                let edited = edited.trim().to_string();
                if edited.is_empty() {
                    return Ok(Decision::Cancel);
                }
                return Ok(Decision::Edit(edited));
            }
            _ => eprintln!("Please answer y (run), n (cancel), or e (edit)."),
        }
    }
}
