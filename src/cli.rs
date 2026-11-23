use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use usage::complete::{Candidate, CompleteCtx};
use usage::{Cli as CliParser, Subcommands, ValueEnum};

use crate::profiles::ProfileStore;

// usage-derive passes its generated, unnameable partial struct first; profile names don't need it.
fn profile_names(_partial: &impl Sized, ctx: &CompleteCtx<'_>) -> Vec<Candidate<'static>> {
    // Supply the unfinished profile argument so the normal parser can resolve -C.
    let mut args: Vec<&OsStr> = ctx.words[..ctx.cword].iter().map(OsStr::new).collect();
    args.push(OsStr::new("profile"));
    let Ok(cli) = Cli::parse_from_argv(&args) else {
        return Vec::new();
    };
    let directory = cli.directory.as_deref().unwrap_or(Path::new("."));
    ProfileStore::from_environment_in(directory)
        .and_then(|store| store.list())
        .unwrap_or_default()
        .into_iter()
        .map(Candidate::new)
        .collect()
}

#[derive(Debug, CliParser)]
#[usage(
    bin = "g",
    version,
    about = "Switch Git identity profiles in the current repository",
    after_help = "Examples:\n  g add work\n  g edit work\n  g edit work --email new@example.com\n  g add personal --name \"Ada Lovelace\" --email ada@example.com\n  g use work\n  g -C ../repo use work\n  g unuse\n  g info\n  g doctor",
    unknown_flags = "error",
    args_override_self = false,
    completion
)]
pub struct Cli {
    /// Run as if started in <path>
    #[usage(short = 'C', long = "directory", global, value_name = "path")]
    pub directory: Option<PathBuf>,
    #[usage(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommands)]
pub enum Command {
    /// Add a new profile
    Add {
        /// Unique profile name
        profile: String,
        /// Git author name
        #[usage(long)]
        name: Option<String>,
        /// Git author email
        #[usage(long)]
        email: Option<String>,
        /// Signing key; also enables commit.gpgsign
        #[usage(long)]
        signing_key: Option<String>,
        /// SSH host alias from ~/.ssh/config
        #[usage(long)]
        ssh_host: Option<String>,
        /// OpenSSH public key text or file path for repository SSH authentication
        #[usage(long)]
        ssh_auth_key: Option<String>,
    },
    /// Edit an existing profile
    Edit {
        /// Saved profile name
        profile: String,
        /// Replace the Git author name
        #[usage(long)]
        name: Option<String>,
        /// Replace the Git author email
        #[usage(long)]
        email: Option<String>,
        /// Set the signing key and enable commit signing
        #[usage(long, conflicts = "no_signing")]
        signing_key: Option<String>,
        /// Remove the signing key and disable commit signing
        #[usage(long)]
        no_signing: bool,
        /// Set the SSH host alias from ~/.ssh/config
        #[usage(long, conflicts = "no_ssh_host")]
        ssh_host: Option<String>,
        /// Remove the SSH host alias
        #[usage(long)]
        no_ssh_host: bool,
        /// Set the repository SSH authentication public key text or file path
        #[usage(long, conflicts = "no_ssh_auth_key")]
        ssh_auth_key: Option<String>,
        /// Clear the SSH authentication public key
        #[usage(long)]
        no_ssh_auth_key: bool,
    },
    /// List saved profile names
    List,
    /// Show a saved profile
    Show {
        /// Saved profile name
        profile: String,
    },
    /// Remove a saved profile
    Remove {
        /// Saved profile name
        profile: String,
    },
    /// Apply a profile to the current Git repository
    Use {
        /// Saved profile name
        #[usage(complete = profile_names)]
        profile: String,
    },
    /// Remove the identity applied by g from the current Git configuration
    Unuse,
    /// Print a saved profile as TOML
    Export {
        /// Saved profile name
        profile: String,
    },
    /// Import a profile from TOML
    Import {
        /// Profile name to create
        profile: String,
        /// TOML file to read; omit or use "-" for standard input
        file: Option<PathBuf>,
        /// Overwrite an existing profile
        #[usage(long)]
        force: bool,
    },
    /// Print a shell completion script
    Completion {
        /// Target shell
        #[usage(value_enum)]
        shell: Shell,
    },
    /// Show the profile matching the current repository configuration
    Info {
        /// Print only the profile name, or nothing when unmanaged or outside a repository
        #[usage(long)]
        short: bool,
    },
    /// Check saved profiles for missing key files, SSH aliases, and signer files
    Doctor,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Nu,
    #[usage(name = "powershell")]
    Pwsh,
    Zsh,
}

impl Shell {
    pub(crate) fn completion_script(self) -> String {
        let script = Cli::completion_script(self.into());
        match self {
            // Fish also matches `complete -c g` when the command is invoked by path, so call the
            // binary that was typed. `-o` (not fish 4's `-x`) keeps fish 3 working.
            Self::Fish => script.replace(
                "    set -l out (command 'g' __complete_word__",
                "    set -l words (commandline -opc)\n    set -l out (command $words[1] __complete_word__",
            ),
            _ => script,
        }
    }
}

impl From<Shell> for usage::complete::Shell {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Bash => Self::Bash,
            Shell::Elvish => Self::Elvish,
            Shell::Fish => Self::Fish,
            Shell::Nu => Self::Nu,
            Shell::Pwsh => Self::PowerShell,
            Shell::Zsh => Self::Zsh,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_completion_calls_the_typed_binary() {
        // Guards the string patch against changes in usage-rs's generated script.
        let script = Shell::Fish.completion_script();
        assert!(script.contains("(command $words[1] __complete_word__"));
        assert!(!script.contains("(command 'g' __complete_word__"));
    }
}
