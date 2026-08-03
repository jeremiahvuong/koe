//! Shell integration.
//!
//! A generated command runs in a child process, so `cd`, `export`, and shell
//! function definitions cannot outlive it. The wrapper printed here calls koe
//! with `--print` (which sends only the approved command to stdout, keeping all
//! prompts on stderr) and evaluates the result in the *calling* shell, so state
//! changes stick.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
}

const ZSH: &str = r#"koe() {
  local __koe_cmd
  __koe_cmd="$(command koe --print "$@")" || return $?
  [ -n "$__koe_cmd" ] || return 0
  print -s -- "$__koe_cmd"
  eval "$__koe_cmd"
}
"#;

const BASH: &str = r#"koe() {
  local __koe_cmd
  __koe_cmd="$(command koe --print "$@")" || return $?
  [ -n "$__koe_cmd" ] || return 0
  history -s "$__koe_cmd"
  eval "$__koe_cmd"
}
"#;

const FISH: &str = r#"function koe
    set -l __koe_cmd (command koe --print $argv)
    or return $status
    test -n "$__koe_cmd"; or return 0
    eval $__koe_cmd
end
"#;

impl ShellKind {
    pub fn init_script(self) -> &'static str {
        match self {
            ShellKind::Zsh => ZSH,
            ShellKind::Bash => BASH,
            ShellKind::Fish => FISH,
        }
    }

    pub fn rc_hint(self) -> &'static str {
        match self {
            ShellKind::Zsh => "eval \"$(koe init zsh)\"   # add to ~/.zshrc",
            ShellKind::Bash => "eval \"$(koe init bash)\"  # add to ~/.bashrc",
            ShellKind::Fish => "koe init fish | source     # add to ~/.config/fish/config.fish",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wrapper_calls_koe_with_print_and_evals() {
        for kind in [ShellKind::Zsh, ShellKind::Bash, ShellKind::Fish] {
            let script = kind.init_script();
            assert!(script.contains("command koe --print"), "{kind:?}");
            assert!(script.contains("eval"), "{kind:?}");
            // `command` prevents the wrapper from calling itself.
            assert!(script.contains("command koe"), "{kind:?}");
        }
    }

    #[test]
    fn wrappers_return_early_on_an_empty_command() {
        // An empty stdout means the user declined; the wrapper must not eval "".
        assert!(ShellKind::Zsh.init_script().contains("-n \"$__koe_cmd\""));
        assert!(ShellKind::Bash.init_script().contains("-n \"$__koe_cmd\""));
        assert!(ShellKind::Fish.init_script().contains("test -n"));
    }
}
