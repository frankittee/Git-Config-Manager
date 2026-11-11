use std::path::PathBuf;

use usage::{Cli as CliParser, Subcommands, ValueEnum};

#[derive(Debug, CliParser)]
#[usage(
    bin = "g",
    version,
    about = "Switch Git identity profiles in the current repository",
    after_help = "Examples:\n  g add work\n  g edit work\n  g edit work --email new@example.com\n  g add personal --name \"Ada Lovelace\" --email ada@example.com\n  g use work\n  g -C ../repo use work\n  g unuse\n  g info",
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
    Info,
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
