use anyhow::{Context, Result};
use std::process::Command;

/// The shell used to run generated commands, plus its "run this string" flag.
///
/// Honors `$SHELL` so commands match the syntax the user actually writes.
pub fn shell() -> (String, &'static str) {
    if cfg!(windows) {
        (
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
            "/C",
        )
    } else {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string());
        (shell, "-c")
    }
}

/// Run a command with stdio inherited, returning its exit code.
///
/// Inheriting rather than capturing is what makes interactive commands work:
/// `sudo` can prompt for a password, `vim` gets a terminal, long-running
/// commands stream output as it is produced, and stderr reaches the user
/// instead of being swallowed.
pub fn run(command: &str) -> Result<i32> {
    let (program, flag) = shell();
    let status = Command::new(&program)
        .arg(flag)
        .arg(command)
        .status()
        .with_context(|| format!("failed to run {program}"))?;

    Ok(exit_code(status))
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // Match shell convention for a signal-terminated child.
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_is_resolved() {
        let (program, flag) = shell();
        assert!(!program.is_empty());
        assert!(flag == "-c" || flag == "/C");
    }

    #[test]
    #[cfg(unix)]
    fn propagates_exit_codes() {
        assert_eq!(run("true").unwrap(), 0);
        assert_eq!(run("exit 3").unwrap(), 3);
        assert_ne!(run("false").unwrap(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn signal_termination_maps_to_128_plus_signal() {
        // SIGTERM is 15.
        assert_eq!(run("kill -TERM $$").unwrap(), 143);
    }
}
