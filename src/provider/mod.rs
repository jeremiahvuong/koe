pub mod gemini;
pub mod openai;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// How destructive a command is judged to be.
///
/// Ordering matters: the effective risk of a command is the maximum of what the
/// model reported and what [`crate::risk`] derives statically.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Read-only or trivially reversible.
    #[default]
    Safe,
    /// Writes, installs, or otherwise modifies state.
    Caution,
    /// Destructive, irreversible, elevated, or system-wide.
    Dangerous,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::Caution => "caution",
            Risk::Dangerous => "dangerous",
        }
    }

    /// ANSI color code used when stderr is a terminal.
    pub fn color(self) -> &'static str {
        match self {
            Risk::Safe => "32",
            Risk::Caution => "33",
            Risk::Dangerous => "31",
        }
    }
}

/// One natural-language translation request.
pub struct Request {
    pub system_prompt: String,
    /// Few-shot pairs of (user task, assistant JSON response).
    pub examples: Vec<(String, String)>,
    pub task: String,
}

/// What the model decided to do with a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proposal {
    Command {
        command: String,
        risk: Risk,
        explanation: String,
    },
    /// The task was ambiguous; the model wants one question answered.
    Clarify { question: String },
    /// The model declined to guess.
    Unknown,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn propose(&self, req: &Request) -> Result<Proposal>;
    /// Short human-readable identifier, e.g. "gemini models/gemini-2.5-flash".
    fn describe(&self) -> String;
}

/// JSON schema describing [`Proposal`], sent to backends that support
/// constrained decoding.
pub fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string", "enum": ["command", "clarify", "unknown"] },
            "command": { "type": "string" },
            "question": { "type": "string" },
            "explanation": { "type": "string" },
            "risk": { "type": "string", "enum": ["safe", "caution", "dangerous"] }
        },
        "required": ["kind"]
    })
}

#[derive(Debug, Deserialize)]
struct RawProposal {
    kind: String,
    command: Option<String>,
    question: Option<String>,
    explanation: Option<String>,
    risk: Option<Risk>,
}

/// Strip markdown fences and surrounding whitespace.
///
/// Constrained decoding should make this unnecessary, but local models that
/// ignore the schema still wrap output in ```.
fn strip_fences(text: &str) -> &str {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t;
    };
    // Drop an optional language tag on the opening fence.
    let rest = match rest.find('\n') {
        Some(nl) => &rest[nl + 1..],
        None => rest,
    };
    rest.trim_end().strip_suffix("```").unwrap_or(rest).trim()
}

/// Parse a backend response into a [`Proposal`].
///
/// Falls back to treating a bare single-line response as a command, so models
/// that cannot honor a JSON schema still work.
pub fn parse_proposal(text: &str) -> Result<Proposal> {
    let text = strip_fences(text);
    if text.is_empty() {
        return Err(anyhow!("model returned an empty response"));
    }

    match serde_json::from_str::<RawProposal>(text) {
        Ok(raw) => match raw.kind.as_str() {
            "command" => {
                let command = raw
                    .command
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .ok_or_else(|| anyhow!("model returned kind=command with no command"))?;
                Ok(Proposal::Command {
                    command,
                    // Absent risk is treated as caution, never safe.
                    risk: raw.risk.unwrap_or(Risk::Caution),
                    explanation: raw.explanation.unwrap_or_default().trim().to_string(),
                })
            }
            "clarify" => Ok(Proposal::Clarify {
                question: raw
                    .question
                    .map(|q| q.trim().to_string())
                    .filter(|q| !q.is_empty())
                    .ok_or_else(|| anyhow!("model returned kind=clarify with no question"))?,
            }),
            "unknown" => Ok(Proposal::Unknown),
            other => Err(anyhow!("model returned unrecognized kind {other:?}")),
        },
        Err(_) => {
            // Not JSON: accept a bare command from a schema-less backend.
            if text.eq_ignore_ascii_case("unknown") || text.contains('\n') {
                return Ok(Proposal::Unknown);
            }
            Ok(Proposal::Command {
                command: text.to_string(),
                // The model expressed no opinion, so there is nothing to defer
                // to; `crate::risk` classifies it on its own. Pinning this to
                // Caution instead would make `--yes` useless with local models
                // that cannot honor a JSON schema, without adding real safety.
                risk: Risk::Safe,
                explanation: String::new(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command() {
        let p = parse_proposal(
            r#"{"kind":"command","command":"ls -la","risk":"safe","explanation":"lists files"}"#,
        )
        .unwrap();
        assert_eq!(
            p,
            Proposal::Command {
                command: "ls -la".into(),
                risk: Risk::Safe,
                explanation: "lists files".into()
            }
        );
    }

    #[test]
    fn missing_risk_defaults_to_caution() {
        let p = parse_proposal(r#"{"kind":"command","command":"rm x"}"#).unwrap();
        match p {
            Proposal::Command { risk, .. } => assert_eq!(risk, Risk::Caution),
            other => panic!("expected command, got {other:?}"),
        }
    }

    #[test]
    fn parses_clarify_and_unknown() {
        assert_eq!(
            parse_proposal(r#"{"kind":"unknown"}"#).unwrap(),
            Proposal::Unknown
        );
        assert_eq!(
            parse_proposal(r#"{"kind":"clarify","question":"which one?"}"#).unwrap(),
            Proposal::Clarify {
                question: "which one?".into()
            }
        );
    }

    #[test]
    fn strips_markdown_fences() {
        let p = parse_proposal("```json\n{\"kind\":\"unknown\"}\n```").unwrap();
        assert_eq!(p, Proposal::Unknown);
    }

    /// A schema-less backend returning a bare command still works; the static
    /// classifier, not this default, is what gates dangerous input.
    #[test]
    fn falls_back_to_bare_command() {
        let p = parse_proposal("  ls -la  ").unwrap();
        match p {
            Proposal::Command { command, risk, .. } => {
                assert_eq!(command, "ls -la");
                assert_eq!(risk, Risk::Safe);
            }
            other => panic!("expected command, got {other:?}"),
        }
    }

    /// A bare destructive command must still be caught, since the fallback
    /// contributes no risk signal of its own.
    #[test]
    fn bare_destructive_command_is_still_flagged() {
        let Proposal::Command { command, .. } = parse_proposal("rm -rf /").unwrap() else {
            panic!("expected a command");
        };
        assert_eq!(crate::risk::classify(&command).risk, Risk::Dangerous);
    }

    #[test]
    fn bare_unknown_is_unknown() {
        assert_eq!(parse_proposal("unknown").unwrap(), Proposal::Unknown);
        assert_eq!(parse_proposal("Unknown\n").unwrap(), Proposal::Unknown);
    }

    #[test]
    fn empty_response_is_an_error() {
        assert!(parse_proposal("   ").is_err());
    }

    #[test]
    fn command_kind_without_command_is_an_error() {
        assert!(parse_proposal(r#"{"kind":"command"}"#).is_err());
    }
}
