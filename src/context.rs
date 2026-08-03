//! Environment facts handed to the model alongside the task.
//!
//! Everything here is sent to the configured provider, so each source is
//! individually switchable in the config file.

use crate::config::ContextConfig;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// Files whose presence identifies the kind of project in the directory.
const PROJECT_MARKERS: &[(&str, &str)] = &[
    ("Cargo.toml", "Rust/Cargo"),
    ("package.json", "Node"),
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "Yarn"),
    ("bun.lockb", "Bun"),
    ("pyproject.toml", "Python"),
    ("requirements.txt", "Python"),
    ("uv.lock", "uv"),
    ("go.mod", "Go"),
    ("pom.xml", "Maven"),
    ("build.gradle", "Gradle"),
    ("Gemfile", "Ruby"),
    ("Makefile", "Make"),
    ("Dockerfile", "Docker"),
    ("docker-compose.yml", "Docker Compose"),
];

/// Tools worth telling the model about, because their presence changes which
/// command is idiomatic.
const KNOWN_TOOLS: &[&str] = &[
    "rg", "fd", "jq", "yq", "gh", "git", "docker", "kubectl", "eza", "bat", "fzf", "curl", "wget",
    "python3", "node", "pnpm", "npm", "yarn", "bun", "uv", "cargo", "go", "ffmpeg", "brew",
    "openssl", "tree", "sed", "awk",
];

const MAX_LISTED_ENTRIES: usize = 40;

#[derive(Debug, Default, Clone)]
pub struct Context {
    pub os: &'static str,
    pub arch: &'static str,
    pub shell: String,
    pub cwd: String,
    pub git: Option<GitInfo>,
    pub project: Vec<&'static str>,
    pub tools: Vec<&'static str>,
    pub entries: Vec<String>,
    pub truncated_entries: bool,
}

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
}

/// Look up a program on `PATH` without spawning a process per lookup.
fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

fn git_info(cwd: &Path) -> Option<GitInfo> {
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let branch = String::from_utf8_lossy(&branch.stdout).trim().to_string();
    if branch.is_empty() {
        return None;
    }
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .ok()
        .is_some_and(|o| o.status.success() && !o.stdout.is_empty());
    Some(GitInfo { branch, dirty })
}

fn list_entries(cwd: &Path) -> (Vec<String>, bool) {
    let Ok(read_dir) = std::fs::read_dir(cwd) else {
        return (Vec::new(), false);
    };
    let mut names: Vec<String> = read_dir
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let suffix = if e.file_type().is_ok_and(|t| t.is_dir()) {
                "/"
            } else {
                ""
            };
            Some(format!("{name}{suffix}"))
        })
        .collect();
    names.sort();
    let truncated = names.len() > MAX_LISTED_ENTRIES;
    names.truncate(MAX_LISTED_ENTRIES);
    (names, truncated)
}

impl Context {
    pub fn gather(config: &ContextConfig) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let shell = std::env::var("SHELL")
            .ok()
            .and_then(|s| {
                Path::new(&s)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| if cfg!(windows) { "cmd" } else { "sh" }.to_string());

        let (entries, truncated_entries) = if config.files {
            list_entries(&cwd)
        } else {
            (Vec::new(), false)
        };

        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            shell,
            cwd: cwd.display().to_string(),
            git: if config.git { git_info(&cwd) } else { None },
            project: if config.files {
                PROJECT_MARKERS
                    .iter()
                    .filter(|(file, _)| cwd.join(file).exists())
                    .map(|(_, label)| *label)
                    .collect()
            } else {
                Vec::new()
            },
            tools: if config.tools {
                KNOWN_TOOLS
                    .iter()
                    .copied()
                    .filter(|t| on_path(t))
                    .collect()
            } else {
                Vec::new()
            },
            entries,
            truncated_entries,
        }
    }

    /// Render as the environment block appended to the system prompt.
    pub fn render(&self) -> String {
        let mut out = String::from("\n## Environment\n\n");
        let _ = writeln!(out, "- OS: {} ({})", self.os, self.arch);
        let _ = writeln!(out, "- Shell: {}", self.shell);
        let _ = writeln!(out, "- Working directory: {}", self.cwd);

        if let Some(git) = &self.git {
            let _ = writeln!(
                out,
                "- Git: on branch {} ({})",
                git.branch,
                if git.dirty {
                    "uncommitted changes"
                } else {
                    "clean"
                }
            );
        }
        if !self.project.is_empty() {
            let mut kinds = self.project.clone();
            kinds.dedup();
            let _ = writeln!(out, "- Project type: {}", kinds.join(", "));
        }
        if !self.tools.is_empty() {
            let _ = writeln!(out, "- Available tools: {}", self.tools.join(", "));
        }
        if !self.entries.is_empty() {
            let more = if self.truncated_entries { ", …" } else { "" };
            let _ = writeln!(
                out,
                "- Directory contents: {}{}",
                self.entries.join(", "),
                more
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_includes_the_basics() {
        let ctx = Context {
            os: "macos",
            arch: "aarch64",
            shell: "zsh".into(),
            cwd: "/tmp/x".into(),
            git: Some(GitInfo {
                branch: "main".into(),
                dirty: true,
            }),
            project: vec!["Rust/Cargo"],
            tools: vec!["rg", "jq"],
            entries: vec!["src/".into()],
            truncated_entries: false,
        };
        let rendered = ctx.render();
        assert!(rendered.contains("macos (aarch64)"));
        assert!(rendered.contains("zsh"));
        assert!(rendered.contains("/tmp/x"));
        assert!(rendered.contains("branch main (uncommitted changes)"));
        assert!(rendered.contains("Rust/Cargo"));
        assert!(rendered.contains("rg, jq"));
    }

    #[test]
    fn disabled_sources_are_omitted() {
        let ctx = Context::gather(&ContextConfig {
            git: false,
            files: false,
            tools: false,
        });
        let rendered = ctx.render();
        assert!(ctx.git.is_none());
        assert!(ctx.entries.is_empty());
        assert!(ctx.tools.is_empty());
        assert!(!rendered.contains("Available tools"));
        assert!(!rendered.contains("Directory contents"));
        // The always-on facts survive.
        assert!(rendered.contains("Working directory"));
    }

    #[test]
    fn finds_a_program_that_must_exist() {
        assert!(on_path("sh") || on_path("cmd.exe"));
        assert!(!on_path("definitely-not-a-real-program-xyz"));
    }
}
