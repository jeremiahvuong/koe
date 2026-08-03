use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{Proposal, Provider, Request, parse_proposal};

/// Talks to any server exposing OpenAI's `/chat/completions` shape: Ollama,
/// llama.cpp, MLX's `mlx_lm.server`, LM Studio, OpenRouter, OpenAI itself.
pub struct OpenAiProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    json_mode: bool,
}

impl OpenAiProvider {
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        model: String,
        json_mode: bool,
    ) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .build()
                .context("failed to build HTTP client")?,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            json_mode,
        })
    }

    fn body(&self, req: &Request, json_mode: bool) -> serde_json::Value {
        let mut messages = vec![json!({"role": "system", "content": req.system_prompt})];
        for (task, response) in &req.examples {
            messages.push(json!({"role": "user", "content": task}));
            messages.push(json!({"role": "assistant", "content": response}));
        }
        messages.push(json!({"role": "user", "content": req.task}));

        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.0,
            "stream": false,
        });
        if json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }
        body
    }

    async fn post(&self, body: &serde_json::Value) -> Result<reqwest::Response> {
        let mut request = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        request.send().await.with_context(|| {
            format!(
                "could not reach {} — is the server running?",
                self.base_url
            )
        })
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn propose(&self, req: &Request) -> Result<Proposal> {
        let mut response = self.post(&self.body(req, self.json_mode)).await?;

        // Not every server accepts `response_format`; retry once without it
        // rather than failing on a backend that would otherwise work.
        if self.json_mode && response.status() == reqwest::StatusCode::BAD_REQUEST {
            response = self.post(&self.body(req, false)).await?;
        }

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("{} returned {}: {}", self.base_url, status, body.trim());
        }

        let parsed: ChatResponse = response
            .json()
            .await
            .context("could not decode chat completion response")?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| anyhow!("response contained no choices"))?;

        parse_proposal(&content)
    }

    fn describe(&self) -> String {
        format!("{} {}", self.base_url, self.model)
    }
}
