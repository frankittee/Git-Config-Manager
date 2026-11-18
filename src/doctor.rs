use std::path::{Path, PathBuf};

use crate::{git::signing_format, profiles::Profile};

#[derive(Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Issue {
    pub severity: Severity,
    pub message: String,
}

/// What `check_profile` needs from the environment, kept explicit so the checks stay pure.
pub struct Environment<'a> {
    pub home: &'a Path,
    pub ssh_aliases: &'a [String],
    pub allowed_signers_file: Option<&'a str>,
}

pub fn check_profile(profile: &Profile, environment: &Environment) -> Vec<Issue> {
    let mut issues = Vec::new();
    if let Some(key) = &profile.signing_key
        && signing_format(key) == "ssh"
    {
        if let Some(path) = key_file(key, environment.home)
            && !path.is_file()
        {
            issues.push(error(format!(
                "signing key file {} does not exist",
                path.display()
            )));
        }
        match environment.allowed_signers_file {
            None => issues.push(warning(
                "gpg.ssh.allowedSignersFile is not set; signatures cannot be verified locally"
                    .into(),
            )),
            Some(file) if !expand_home(file, environment.home).is_file() => {
                issues.push(warning(format!(
                    "gpg.ssh.allowedSignersFile {file} does not exist"
                )));
            }
            Some(_) => {}
        }
    }
    if let Some(host) = &profile.ssh_host
        && !environment
            .ssh_aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(host))
    {
        issues.push(error(format!(
            "SSH host alias '{host}' is not declared in ~/.ssh/config"
        )));
    }
    issues
}

/// Returns the key file path, or `None` for inline keys (`ssh-ed25519 ...`, `key::...`).
fn key_file(key: &str, home: &Path) -> Option<PathBuf> {
    if key.starts_with("ssh-") || key.starts_with("ecdsa-") || key.starts_with("key::") {
        None
    } else {
        Some(expand_home(key, home))
    }
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

fn error(message: String) -> Issue {
    Issue {
        severity: Severity::Error,
        message,
    }
}

fn warning(message: String) -> Issue {
    Issue {
        severity: Severity::Warning,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(key: Option<&str>, host: Option<&str>) -> Profile {
        Profile {
            name: "A".into(),
            email: "a@example.com".into(),
            signing_key: key.map(str::to_owned),
            ssh_host: host.map(str::to_owned),
            ssh_auth_key: None,
        }
    }

    #[test]
    fn plain_profile_has_no_issues() {
        let environment = Environment {
            home: Path::new("/nonexistent"),
            ssh_aliases: &[],
            allowed_signers_file: None,
        };
        assert!(check_profile(&profile(None, None), &environment).is_empty());
    }

    #[test]
    fn reports_missing_key_file_signers_and_host() {
        let home = tempfile::tempdir().unwrap();
        let aliases = ["github-work".to_owned()];
        let environment = Environment {
            home: home.path(),
            ssh_aliases: &aliases,
            allowed_signers_file: None,
        };
        let issues = check_profile(
            &profile(Some("~/.ssh/id.pub"), Some("github-personal")),
            &environment,
        );
        let errors = issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .count();
        assert_eq!(errors, 2);
        assert_eq!(issues.len(), 3);
    }

    #[test]
    fn healthy_ssh_profile_passes() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("id.pub"), "ssh-ed25519 AAAA").unwrap();
        std::fs::write(home.path().join("signers"), "").unwrap();
        let aliases = ["github-work".to_owned()];
        let environment = Environment {
            home: home.path(),
            ssh_aliases: &aliases,
            allowed_signers_file: Some("~/signers"),
        };
        let issues = check_profile(
            &profile(Some("~/id.pub"), Some("GitHub-Work")),
            &environment,
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn inline_and_openpgp_keys_skip_file_checks() {
        let environment = Environment {
            home: Path::new("/nonexistent"),
            ssh_aliases: &[],
            allowed_signers_file: Some("/nonexistent"),
        };
        assert!(check_profile(&profile(Some("ABCD1234"), None), &environment).is_empty());
        let inline = check_profile(&profile(Some("ssh-ed25519 AAAA"), None), &environment);
        assert_eq!(inline.len(), 1);
        assert_eq!(inline[0].severity, Severity::Warning);
    }
}
