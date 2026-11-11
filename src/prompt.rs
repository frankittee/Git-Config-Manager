//! Interactive `add`/`edit` prompts built on [demand](https://github.com/jdx/demand).
//!
//! demand renders on stderr, so stdout stays free for scripts. Each field is a
//! separate prompt; validation runs inline and blocks submitting invalid values.

use std::io;

use anyhow::{Context, Result, bail};
use demand::{Confirm, Input};

use crate::profiles::Profile;

// Aliases listed in the SSH host description before collapsing into "+N more".
const MAX_LISTED_ALIASES: usize = 4;

/// Asks for every profile field, prefilled from `initial`. `signing` sets the
/// default answer of the commit-signing confirmation.
pub fn run(initial: Profile, signing: bool, aliases: Vec<String>) -> Result<Profile> {
    let aliases = suggestible_aliases(aliases);

    let name = ask(Input::new("Git author name")
        .placeholder("Ada Lovelace")
        .default_value(initial.name)
        .validation(required))?;
    let email = ask(Input::new("Git author email")
        .placeholder("ada@example.com")
        .default_value(initial.email)
        .validation(required))?;
    let signing = cancellable(
        Confirm::new("Enable commit signing?")
            .description("Sign commits with an SSH key")
            .affirmative("Yes")
            .negative("No")
            .selected(signing)
            .run(),
    )?;
    let signing_key = if signing {
        Some(ask(Input::new("Signing key")
            .description("SSH public key path or literal key")
            .placeholder("~/.ssh/id_ed25519.pub")
            .default_value(initial.signing_key.unwrap_or_default())
            .validation(required))?)
    } else {
        None
    };
    let description = ssh_host_description(&aliases);
    let suggestions = aliases.clone();
    let ssh_host = ask(Input::new("SSH host alias")
        .description(&description)
        .placeholder("github-work")
        .default_value(initial.ssh_host.unwrap_or_default())
        .autocomplete_fn(move |input| Ok(matching_aliases(&suggestions, input)))
        .validation(literal_ssh_host))?;

    Ok(Profile {
        name,
        email,
        signing_key,
        ssh_host: (!ssh_host.is_empty()).then_some(ssh_host),
    })
}

fn ask(input: Input) -> Result<String> {
    cancellable(input.run()).map(|value| value.trim().to_owned())
}

/// Maps demand's cancellation (esc / ctrl+c) to the CLI's `cancelled` error.
fn cancellable<T>(result: io::Result<T>) -> Result<T> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => bail!("cancelled"),
        other => other.context("could not read interactive input"),
    }
}

fn required(value: &str) -> Result<(), &str> {
    if value.trim().is_empty() {
        Err("Value must not be empty")
    } else {
        Ok(())
    }
}

fn literal_ssh_host(value: &str) -> Result<(), &str> {
    let value = value.trim();
    if value.chars().any(char::is_whitespace) || value.contains(['*', '?', '!']) {
        Err("SSH host alias must be literal and contain no whitespace")
    } else {
        Ok(())
    }
}

/// Bare IP addresses are host entries, not aliases worth suggesting.
fn suggestible_aliases(aliases: Vec<String>) -> Vec<String> {
    aliases
        .into_iter()
        .filter(|alias| alias.parse::<std::net::IpAddr>().is_err())
        .collect()
}

fn matching_aliases(aliases: &[String], input: &str) -> Vec<String> {
    let prefix = input.trim().to_ascii_lowercase();
    aliases
        .iter()
        .filter(|alias| alias.to_ascii_lowercase().starts_with(&prefix))
        .cloned()
        .collect()
}

fn ssh_host_description(aliases: &[String]) -> String {
    if aliases.is_empty() {
        return "Leave blank to skip".to_owned();
    }
    let mut listed = aliases
        .iter()
        .take(MAX_LISTED_ALIASES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if aliases.len() > MAX_LISTED_ALIASES {
        listed.push_str(&format!(", +{} more", aliases.len() - MAX_LISTED_ALIASES));
    }
    format!("Leave blank to skip · tab completes: {listed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn required_rejects_blank_values() {
        assert!(required("").is_err());
        assert!(required("   ").is_err());
        assert!(required(" Ada ").is_ok());
    }

    #[test]
    fn ssh_host_must_be_literal() {
        assert!(literal_ssh_host("").is_ok());
        assert!(literal_ssh_host(" github-work ").is_ok());
        for invalid in ["bad host", "github-*", "gh?", "!gh"] {
            assert!(literal_ssh_host(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn suggestions_skip_ip_addresses() {
        let aliases = suggestible_aliases(strings(&["10.0.0.1", "::1", "github-work"]));
        assert_eq!(aliases, ["github-work"]);
    }

    #[test]
    fn suggestions_match_prefix_case_insensitively() {
        let aliases = strings(&["github-work", "GitHub-Home", "gitlab"]);
        assert_eq!(
            matching_aliases(&aliases, "GITHUB-"),
            ["github-work", "GitHub-Home"]
        );
        assert_eq!(matching_aliases(&aliases, ""), aliases);
        assert!(matching_aliases(&aliases, "bitbucket").is_empty());
    }

    #[test]
    fn description_lists_aliases_and_collapses_long_lists() {
        assert_eq!(ssh_host_description(&[]), "Leave blank to skip");
        assert_eq!(
            ssh_host_description(&strings(&["a", "b"])),
            "Leave blank to skip · tab completes: a, b"
        );
        assert_eq!(
            ssh_host_description(&strings(&["a", "b", "c", "d", "e", "f"])),
            "Leave blank to skip · tab completes: a, b, c, d, +2 more"
        );
    }

    #[test]
    fn interrupted_maps_to_cancelled() {
        let interrupted: io::Result<()> = Err(io::Error::new(io::ErrorKind::Interrupted, "x"));
        assert_eq!(
            cancellable(interrupted).unwrap_err().to_string(),
            "cancelled"
        );
        let other: io::Result<()> = Err(io::Error::other("boom"));
        assert!(
            cancellable(other)
                .unwrap_err()
                .to_string()
                .contains("could not read interactive input")
        );
    }
}
