use crate::config::ProviderKind;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "koe",
    version,
    about = "Natural language to shell commands",
    after_help = "Subcommands:\n  koe init <zsh|bash|fish>   Print the shell wrapper (lets `cd` persist)\n  koe config                 Show the effective configuration"
)]
pub struct Cli {
    /// Run without confirming when the command is low risk
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Run without confirming at any risk level
    #[arg(long)]
    pub yolo: bool,

    /// Deprecated alias for --yolo
    #[arg(long = "unsafe", hide = true)]
    pub r#unsafe: bool,

    /// Print the command without running it
    #[arg(long)]
    pub dry_run: bool,

    /// Print the approved command to stdout instead of running it
    #[arg(long)]
    pub print: bool,

    /// Backend to use
    #[arg(long, value_enum)]
    pub provider: Option<ProviderKind>,

    /// Model name, e.g. models/gemini-2.5-flash or qwen2.5-coder:7b
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// OpenAI-compatible endpoint, e.g. http://localhost:11434/v1
    #[arg(long)]
    pub base_url: Option<String>,

    /// The task to turn into a shell command
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub task: Vec<String>,
}

impl Cli {
    pub fn task(&self) -> String {
        self.task.join(" ").trim().to_string()
    }

    /// Whether confirmation is skipped for a command at this risk level.
    pub fn skips_confirmation(&self, risk: crate::provider::Risk, auto_run: bool) -> bool {
        if self.yolo || self.r#unsafe {
            return true;
        }
        (self.yes || auto_run) && risk == crate::provider::Risk::Safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Risk;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("koe").chain(args.iter().copied()))
    }

    #[test]
    fn collects_the_task_from_loose_words() {
        assert_eq!(parse(&["how", "many", "files"]).task(), "how many files");
    }

    #[test]
    fn flags_precede_the_task() {
        let cli = parse(&["-y", "list", "files"]);
        assert!(cli.yes);
        assert_eq!(cli.task(), "list files");
    }

    #[test]
    fn hyphens_inside_the_task_are_not_flags() {
        let cli = parse(&["show", "me", "ls", "-la", "output"]);
        assert_eq!(cli.task(), "show me ls -la output");
        assert!(!cli.yes);
    }

    #[test]
    fn yes_only_auto_runs_safe_commands() {
        let cli = parse(&["-y", "x"]);
        assert!(cli.skips_confirmation(Risk::Safe, false));
        assert!(!cli.skips_confirmation(Risk::Caution, false));
        assert!(!cli.skips_confirmation(Risk::Dangerous, false));
    }

    #[test]
    fn yolo_auto_runs_everything() {
        let cli = parse(&["--yolo", "x"]);
        assert!(cli.skips_confirmation(Risk::Dangerous, false));
    }

    #[test]
    fn deprecated_unsafe_flag_still_works() {
        let cli = parse(&["--unsafe", "x"]);
        assert!(cli.skips_confirmation(Risk::Dangerous, false));
    }

    #[test]
    fn auto_run_config_matches_the_yes_flag() {
        let cli = parse(&["x"]);
        assert!(cli.skips_confirmation(Risk::Safe, true));
        assert!(!cli.skips_confirmation(Risk::Caution, true));
    }

    #[test]
    fn nothing_auto_runs_by_default() {
        let cli = parse(&["x"]);
        assert!(!cli.skips_confirmation(Risk::Safe, false));
    }
}
