//! Best-effort JSONL log of what koe proposed and what the user did about it.
//!
//! Each line records the task, the proposed command, whether it was accepted,
//! rejected, or rewritten, and the resulting exit code. Rejections and rewrites
//! are the useful part: they are labeled negative examples, which are exactly
//! what a fine-tune or an eval set needs and what a corpus of successful
//! commands cannot provide.

use anyhow::{Context, Result};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub struct Entry<'a> {
    pub ts: u64,
    pub cwd: &'a str,
    pub os: &'a str,
    pub shell: &'a str,
    pub model: &'a str,
    pub task: &'a str,
    /// What the model proposed, if it proposed anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed: Option<&'a str>,
    /// What actually ran, which differs from `proposed` when the user edited it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ran: Option<&'a str>,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ran,
    Edited,
    Cancelled,
    DryRun,
    Printed,
    Unknown,
    Clarify,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `$XDG_STATE_HOME/koe/history.jsonl`, falling back to `~/.local/state`.
pub fn path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("koe").join("history.jsonl"));
    }
    dirs::home_dir().map(|h| {
        h.join(".local")
            .join("state")
            .join("koe")
            .join("history.jsonl")
    })
}

pub fn append(entry: &Entry) -> Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let line = serde_json::to_string(entry)?;
    writeln!(file, "{line}").with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_compactly_and_omits_empty_fields() {
        let entry = Entry {
            ts: 1,
            cwd: "/tmp",
            os: "macos",
            shell: "zsh",
            model: "m",
            task: "count files",
            proposed: Some("ls | wc -l"),
            ran: None,
            outcome: Outcome::Cancelled,
            risk: Some("safe"),
            exit_code: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""outcome":"cancelled""#));
        assert!(json.contains(r#""proposed":"ls | wc -l""#));
        assert!(!json.contains("\"ran\""));
        assert!(!json.contains("exit_code"));
        assert!(!json.contains('\n'));
    }

    #[test]
    fn outcome_names_are_stable() {
        assert_eq!(serde_json::to_string(&Outcome::DryRun).unwrap(), "\"dry-run\"");
        assert_eq!(serde_json::to_string(&Outcome::Ran).unwrap(), "\"ran\"");
    }
}
