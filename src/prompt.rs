//! Prompt assembly.
//!
//! The system prompt and few-shot examples live in `prompts/` rather than in
//! source, so they can be edited and diffed as data. Users can extend the
//! examples without rebuilding by adding lines to
//! `~/.config/koe/examples.jsonl`.

use crate::context::Context;
use anyhow::{Context as _, Result};
use serde::Deserialize;
use std::path::PathBuf;

const SYSTEM: &str = include_str!("../prompts/system.md");
const EXAMPLES: &str = include_str!("../prompts/examples.jsonl");

pub fn system_prompt(ctx: &Context) -> String {
    format!("{}{}", SYSTEM.trim_end(), ctx.render())
}

#[derive(Debug, Deserialize)]
struct ExampleLine {
    task: String,
    response: serde_json::Value,
}

pub fn user_examples_path() -> Option<PathBuf> {
    crate::config::Config::path().map(|p| p.with_file_name("examples.jsonl"))
}

fn parse_examples(source: &str, origin: &str) -> Result<Vec<(String, String)>> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|(i, line)| {
            let parsed: ExampleLine = serde_json::from_str(line)
                .with_context(|| format!("{origin} line {}", i + 1))?;
            Ok((parsed.task, parsed.response.to_string()))
        })
        .collect()
}

/// Built-in few-shot examples followed by any the user has added.
pub fn examples() -> Result<Vec<(String, String)>> {
    let mut examples = parse_examples(EXAMPLES, "prompts/examples.jsonl")?;

    if let Some(path) = user_examples_path()
        && let Ok(text) = std::fs::read_to_string(&path)
    {
        examples.extend(parse_examples(&text, &path.display().to_string())?);
    }

    Ok(examples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Proposal, parse_proposal};

    #[test]
    fn builtin_examples_parse() {
        let examples = examples().unwrap();
        assert!(examples.len() >= 8, "expected a meaningful few-shot set");
    }

    /// Every example's assistant turn must survive the same parser the real
    /// responses go through, or we are teaching the model an invalid format.
    #[test]
    fn every_example_response_is_valid() {
        for (task, response) in parse_examples(EXAMPLES, "test").unwrap() {
            parse_proposal(&response)
                .unwrap_or_else(|e| panic!("example {task:?} has an invalid response: {e}"));
        }
    }

    /// Abstention has to be demonstrated or the model learns to always answer.
    #[test]
    fn examples_include_unknown_and_clarify() {
        let parsed: Vec<Proposal> = parse_examples(EXAMPLES, "test")
            .unwrap()
            .iter()
            .map(|(_, r)| parse_proposal(r).unwrap())
            .collect();
        assert!(
            parsed.iter().filter(|p| **p == Proposal::Unknown).count() >= 2,
            "need several `unknown` examples"
        );
        assert!(
            parsed
                .iter()
                .filter(|p| matches!(p, Proposal::Clarify { .. }))
                .count()
                >= 2,
            "need several `clarify` examples"
        );
    }

    #[test]
    fn blank_and_comment_lines_are_skipped() {
        let source = "\n# a comment\n{\"task\":\"x\",\"response\":{\"kind\":\"unknown\"}}\n\n";
        assert_eq!(parse_examples(source, "test").unwrap().len(), 1);
    }

    #[test]
    fn malformed_lines_report_their_position() {
        let err = parse_examples("{\"task\":\"x\"}", "test").unwrap_err();
        assert!(format!("{err}").contains("line 1"));
    }

    #[test]
    fn system_prompt_embeds_the_environment() {
        let ctx = Context::gather(&Default::default());
        let prompt = system_prompt(&ctx);
        assert!(prompt.contains("## Environment"));
        assert!(prompt.contains("Working directory"));
        assert!(prompt.contains("kind"));
    }
}
