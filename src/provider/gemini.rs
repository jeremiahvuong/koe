use anyhow::{Context, Result};
use async_trait::async_trait;
use gemini_rust::Gemini;

use super::{Proposal, Provider, Request, parse_proposal, response_schema};

pub struct GeminiProvider {
    client: Gemini,
    model: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Gemini::with_model(api_key, model.clone()),
            model,
        }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    async fn propose(&self, req: &Request) -> Result<Proposal> {
        let mut builder = self
            .client
            .generate_content()
            .with_system_prompt(req.system_prompt.as_str())
            .with_temperature(0.0)
            .with_response_mime_type("application/json")
            .with_response_schema(response_schema());

        for (task, response) in &req.examples {
            builder = builder
                .with_user_message(task.as_str())
                .with_model_message(response.as_str());
        }

        let response = builder
            .with_user_message(req.task.as_str())
            .execute()
            .await
            .context("Gemini request failed")?;

        parse_proposal(&response.text())
    }

    fn describe(&self) -> String {
        format!("gemini {}", self.model)
    }
}
