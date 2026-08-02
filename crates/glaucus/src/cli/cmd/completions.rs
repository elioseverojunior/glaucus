// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus completions` — emit a shell completion script.

use crate::cli::Cli;
use crate::cli::env::Env;
use crate::cli::exit;
use clap::CommandFactory;

/// Arguments for `glaucus completions`.
#[derive(clap::Args, Debug)]
pub(crate) struct CompletionsArgs {
    /// Shell to generate for.
    pub shell: clap_complete::Shell,
}

/// Runs the command, returning the exit code.
#[must_use]
pub(crate) fn run(args: &CompletionsArgs, env: &mut Env<'_>) -> u8 {
    let mut cmd = Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "glaucus", env.stdout);
    exit::OK
}

#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn bash_completions_go_to_stdout() {
        let (code, out, err) = drive(&["glaucus", "completions", "bash"], "");
        assert_eq!(code, exit::OK);
        assert!(out.contains("glaucus"), "no script:\n{out}");
        assert!(err.is_empty());
    }

    #[test]
    fn zsh_and_fish_are_supported() {
        for shell in ["zsh", "fish"] {
            let (code, out, _e) = drive(&["glaucus", "completions", shell], "");
            assert_eq!(code, exit::OK, "shell {shell} failed");
            assert!(!out.is_empty());
        }
    }

    #[test]
    fn unknown_shell_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "completions", "cmd.exe"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("invalid value"));
    }

    #[test]
    fn comp_alias_works() {
        let (code, _o, _e) = drive(&["glaucus", "comp", "bash"], "");
        assert_eq!(code, exit::OK);
    }
}
