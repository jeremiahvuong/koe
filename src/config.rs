use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Gemini,
    /// Any OpenAI-compatible server: Ollama, llama.cpp, mlx_lm.server, LM
    /// Studio, OpenRouter, OpenAI.
    Openai,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextConfig {
    /// Include branch name and whether the tree is dirty.
    pub git: bool,
    /// Include a truncated listing of the working directory.
    pub files: bool,
    /// Include which well-known CLI tools are installed.
    pub tools: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            git: true,
            files: true,
            tools: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub provider: ProviderKind,
    /// Defaults per provider; see [`Config::resolved_model`].
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// Name of the environment variable holding the API key.
    pub api_key_env: Option<String>,
    /// Run low-risk commands without prompting.
    pub auto_run: bool,
    /// Send `response_format: json_object`. Disable for servers that reject it.
    pub json_mode: bool,
    /// Append accepted/rejected commands to the history log.
    pub log_history: bool,
    pub context: ContextConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderKind::default(),
            model: None,
            base_url: None,
            api_key_env: None,
            auto_run: false,
            json_mode: true,
            log_history: true,
            context: ContextConfig::default(),
        }
    }
}

const DEFAULT_GEMINI_MODEL: &str = "models/gemini-2.5-flash";
const DEFAULT_LOCAL_MODEL: &str = "qwen2.5-coder:7b";
const DEFAULT_LOCAL_BASE_URL: &str = "http://localhost:11434/v1";

impl Config {
    pub fn resolved_model(&self) -> String {
        self.model.clone().unwrap_or_else(|| {
            match self.provider {
                ProviderKind::Gemini => DEFAULT_GEMINI_MODEL,
                ProviderKind::Openai => DEFAULT_LOCAL_MODEL,
            }
            .to_string()
        })
    }

    pub fn resolved_base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_LOCAL_BASE_URL.to_string())
    }

    /// Look up the API key for the configured provider.
    ///
    /// Gemini requires one. OpenAI-compatible backends do not, because local
    /// servers accept unauthenticated requests.
    pub fn api_key(&self) -> Result<Option<String>> {
        match self.provider {
            ProviderKind::Gemini => {
                let key = std::env::var("GEMINI_API_KEY").map_err(|_| {
                    anyhow::anyhow!(
                        "GEMINI_API_KEY is not set.\n\
                         Get a key at https://aistudio.google.com/app/apikey and run:\n\
                         \n    export GEMINI_API_KEY=your-api-key\n\
                         \nOr run a local model instead: koe --provider openai"
                    )
                })?;
                Ok(Some(key))
            }
            ProviderKind::Openai => {
                let candidates: Vec<String> = match &self.api_key_env {
                    Some(name) => vec![name.clone()],
                    None => vec!["OPENAI_API_KEY".into(), "OPENROUTER_API_KEY".into()],
                };
                Ok(candidates
                    .iter()
                    .find_map(|name| std::env::var(name).ok())
                    .filter(|k| !k.is_empty()))
            }
        }
    }

    /// `$XDG_CONFIG_HOME/koe/config.toml`, falling back to `~/.config/koe`.
    ///
    /// `~/.config` is preferred over the platform config dir on macOS because
    /// that is where users of a CLI tool expect to find it.
    pub fn path() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            return Some(PathBuf::from(xdg).join("koe").join("config.toml"));
        }
        dirs::home_dir().map(|h| h.join(".config").join("koe").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let Some(path) = Self::path() else {
            return Ok(Self::default());
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let mut config: Self =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        config.apply_env();
        Ok(config)
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("KOE_PROVIDER") {
            match v.to_ascii_lowercase().as_str() {
                "gemini" => self.provider = ProviderKind::Gemini,
                "openai" | "ollama" | "local" => self.provider = ProviderKind::Openai,
                _ => {}
            }
        }
        if let Ok(v) = std::env::var("KOE_MODEL")
            && !v.is_empty()
        {
            self.model = Some(v);
        }
        if let Ok(v) = std::env::var("KOE_BASE_URL")
            && !v.is_empty()
        {
            self.base_url = Some(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_gemini_flash() {
        let c = Config::default();
        assert_eq!(c.provider, ProviderKind::Gemini);
        assert_eq!(c.resolved_model(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn openai_provider_defaults_to_a_local_model() {
        let c = Config {
            provider: ProviderKind::Openai,
            ..Default::default()
        };
        assert_eq!(c.resolved_model(), DEFAULT_LOCAL_MODEL);
        assert_eq!(c.resolved_base_url(), DEFAULT_LOCAL_BASE_URL);
    }

    #[test]
    fn parses_a_full_config_file() {
        let toml = r#"
            provider = "openai"
            model = "qwen2.5-coder:1.5b"
            base_url = "http://localhost:8080/v1"
            auto_run = true
            json_mode = false
            log_history = false

            [context]
            git = true
            files = false
            tools = true
        "#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.provider, ProviderKind::Openai);
        assert_eq!(c.resolved_model(), "qwen2.5-coder:1.5b");
        assert!(c.auto_run);
        assert!(!c.json_mode);
        assert!(!c.context.files);
        assert!(c.context.tools);
    }

    #[test]
    fn empty_config_file_is_valid() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.provider, ProviderKind::Gemini);
        assert!(c.context.git);
    }

    #[test]
    fn unknown_keys_are_rejected_rather_than_silently_ignored() {
        assert!(toml::from_str::<Config>("privider = \"gemini\"").is_err());
    }
}
