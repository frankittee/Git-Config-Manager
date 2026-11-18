use std::{
    collections::BTreeSet,
    env, fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::BaseDirs;
use ssh2_config_rs::{ParseRule, SshConfig};
use tempfile::NamedTempFile;

use crate::profiles::Profile;

const IDENTITY_KEYS: [&str; 5] = [
    "user.name",
    "user.email",
    "user.signingkey",
    "commit.gpgsign",
    "gpg.format",
];
const SSH_COMMAND: &str = "core.sshCommand";
const SSH_MANAGED: &str = "g.sshAuthKeyManaged";
const SSH_PREVIOUS: &str = "g.previousSshCommand";

#[derive(Clone, Copy)]
enum ConfigScope {
    Local,
    Global,
}

impl ConfigScope {
    fn argument(self) -> &'static str {
        match self {
            Self::Local => "--local",
            Self::Global => "--global",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
    pub signing_key: Option<String>,
    pub gpg_sign: Option<String>,
    pub gpg_format: Option<String>,
}

impl GitIdentity {
    pub fn matches(&self, profile: &Profile) -> bool {
        self.name.as_deref() == Some(profile.name.as_str())
            && self.email.as_deref() == Some(profile.email.as_str())
            && match &profile.signing_key {
                Some(key) => {
                    self.signing_key.as_deref() == Some(key.as_str())
                        && self
                            .gpg_sign
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
                        && self
                            .gpg_format
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case(signing_format(key)))
                }
                None => {
                    self.signing_key.is_none()
                        && self.gpg_sign.is_none()
                        && self.gpg_format.is_none()
                }
            }
    }

    pub fn print(&self) {
        print_optional("user.name", &self.name);
        print_optional("user.email", &self.email);
        print_optional("user.signingkey", &self.signing_key);
        print_optional("commit.gpgsign", &self.gpg_sign);
        print_optional("gpg.format", &self.gpg_format);
    }
}

pub fn ensure_repository() -> Result<()> {
    let output = run_git(&["rev-parse", "--git-dir"])?;
    if !output.status.success() {
        bail!("current directory is not inside a Git repository");
    }
    Ok(())
}

pub fn read_identity() -> Result<GitIdentity> {
    read_identity_at(ConfigScope::Local)
}

pub fn read_current_identity() -> Result<GitIdentity> {
    let scope = config_scope()?;
    if matches!(scope, ConfigScope::Local) {
        ensure_repository()?;
    }
    read_identity_at(scope)
}

pub fn current_scope_label() -> Result<&'static str> {
    Ok(match config_scope()? {
        ConfigScope::Local => "LOCAL",
        ConfigScope::Global => "GLOBAL",
    })
}

pub fn apply_profile(profile: &Profile) -> Result<String> {
    let scope = config_scope()?;
    if matches!(scope, ConfigScope::Local) {
        ensure_repository()?;
    }
    if let Some(ssh_host) = &profile.ssh_host {
        ensure_ssh_host_exists(ssh_host)?;
    }

    let remote_updates = remote_updates(scope, profile.ssh_host.as_deref())?;
    let mut keys: Vec<String> = IDENTITY_KEYS.iter().map(|key| (*key).to_owned()).collect();
    keys.extend(remote_updates.iter().map(|update| update.key.clone()));
    let managed = matches!(scope, ConfigScope::Local) && ssh_auth_key_managed(scope)?;
    let ssh_auth_key = match &profile.ssh_auth_key {
        Some(path) if matches!(scope, ConfigScope::Local) => {
            Some(prepare_ssh_auth_key(path, profile.ssh_host.is_some())?)
        }
        Some(_) => bail!("an SSH auth key requires a Git repository"),
        None => None,
    };
    if managed || ssh_auth_key.is_some() {
        keys.extend([SSH_COMMAND, SSH_MANAGED, SSH_PREVIOUS].map(str::to_owned));
    }
    let original = snapshot(scope, &keys)?;
    let result = apply(scope, profile);
    let result = result.and_then(|()| apply_remote_updates(scope, &remote_updates));
    let result = result.and_then(|()| {
        let origin = config_origin(scope, "user.name")?;
        if let Some((key, path, command)) = ssh_auth_key {
            if !managed {
                for value in get_all(scope, SSH_COMMAND)? {
                    add(scope, SSH_PREVIOUS, &value)?;
                }
                replace(scope, SSH_MANAGED, "true")?;
            }
            replace(scope, SSH_COMMAND, &command)?;
            key.persist(&path)
                .with_context(|| format!("could not install public key at {}", path.display()))?;
        } else if managed {
            restore_ssh_command(scope)?;
        }
        Ok(origin)
    });
    if let Err(error) = result {
        if let Err(rollback_error) = restore(scope, &original) {
            return Err(error.context(format!("rollback also failed: {rollback_error:#}")));
        }
        return Err(error.context("Git configuration was restored"));
    }
    result
}

/// Remove the identity keys and SSH command that `apply_profile` manages.
/// Returns the config file that was changed, or `None` when nothing was set.
pub fn remove_identity() -> Result<Option<String>> {
    let scope = config_scope()?;
    if matches!(scope, ConfigScope::Local) {
        ensure_repository()?;
    }
    let managed = matches!(scope, ConfigScope::Local) && ssh_auth_key_managed(scope)?;
    let mut keys: Vec<String> = IDENTITY_KEYS.iter().map(|key| (*key).to_owned()).collect();
    if managed {
        keys.extend([SSH_COMMAND, SSH_MANAGED, SSH_PREVIOUS].map(str::to_owned));
    }
    let original = snapshot(scope, &keys)?;
    let Some(origin_key) = original
        .iter()
        .find(|(_, values)| !values.is_empty())
        .map(|(key, _)| key.clone())
    else {
        return Ok(None);
    };
    let config_file = config_origin(scope, &origin_key)?;
    let result = IDENTITY_KEYS
        .iter()
        .try_for_each(|key| unset(scope, key))
        .and_then(|()| {
            if managed {
                restore_ssh_command(scope)?;
            }
            Ok(())
        });
    if let Err(error) = result {
        if let Err(rollback_error) = restore(scope, &original) {
            return Err(error.context(format!("rollback also failed: {rollback_error:#}")));
        }
        return Err(error.context("Git configuration was restored"));
    }
    Ok(Some(config_file))
}

fn ssh_auth_key_managed(scope: ConfigScope) -> Result<bool> {
    Ok(get_all(scope, SSH_MANAGED)?
        .last()
        .is_some_and(|value| value == "true"))
}

fn restore_ssh_command(scope: ConfigScope) -> Result<()> {
    let previous = get_all(scope, SSH_PREVIOUS)?;
    restore(scope, &[(SSH_COMMAND.to_owned(), previous)])?;
    unset(scope, SSH_PREVIOUS)?;
    unset(scope, SSH_MANAGED)
}

fn prepare_ssh_auth_key(
    source: &str,
    use_ssh_config: bool,
) -> Result<(NamedTempFile, PathBuf, String)> {
    let key_prefixes = ["ssh-", "ecdsa-", "sk-"];
    let (contents, label) = if source
        .trim()
        .split_once(char::is_whitespace)
        .is_some_and(|(kind, _)| key_prefixes.iter().any(|prefix| kind.starts_with(prefix)))
    {
        (source.to_owned(), "inline SSH auth key".to_owned())
    } else {
        let path = match source.strip_prefix("~/") {
            Some(relative) => home_dir()?.join(relative),
            None => PathBuf::from(source),
        };
        let label = format!("SSH public key file {}", path.display());
        let contents =
            fs::read_to_string(&path).with_context(|| format!("could not read {label}"))?;
        (contents, label)
    };
    let mut fields = contents.split_whitespace();
    let (Some(kind), Some(encoded)) = (fields.next(), fields.next()) else {
        bail!("{label} is not an OpenSSH public key");
    };
    if !key_prefixes.iter().any(|prefix| kind.starts_with(prefix))
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte))
        || contents.trim().lines().count() != 1
    {
        bail!("{label} is not an OpenSSH public key");
    }

    let output = run_git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    if !output.status.success() {
        return Err(git_failure("locate", "git-common-dir", &output));
    }
    let git_dir = String::from_utf8(output.stdout).context("Git returned a non-UTF-8 directory")?;
    let directory = PathBuf::from(git_dir.trim_end_matches('\n')).join("ssh");
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let mut key = NamedTempFile::new_in(&directory).context("could not stage SSH public key")?;
    writeln!(key, "{}", contents.trim()).context("could not write SSH public key")?;
    let status = Command::new("ssh-keygen")
        .args(["-lf", "-"])
        .stdin(key.reopen().context("could not reopen SSH public key")?)
        .output()
        .context("could not run ssh-keygen to validate the SSH public key")?
        .status;
    if !status.success() {
        bail!("{label} is not a valid OpenSSH public key");
    }
    let path = directory.join("g-auth.pub");
    let identity = path.to_str().context("SSH public key path is not UTF-8")?;
    if identity.contains("${") {
        bail!("SSH public key installation path contains an OpenSSH variable expression");
    }
    let identity = identity.replace('%', "%%").replace('\'', "'\\''");
    let config = if use_ssh_config { "" } else { " -F /dev/null" };
    let command = format!("ssh{config} -o IdentitiesOnly=yes -i '{identity}'");
    Ok((key, path, command))
}

fn config_scope() -> Result<ConfigScope> {
    let current_dir = canonical_current_dir()?;
    let home_dir = BaseDirs::new()
        .context("could not determine the user home directory")?
        .home_dir()
        .canonicalize()
        .context("could not resolve the user home directory")?;

    Ok(if current_dir == home_dir {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    })
}

fn canonical_current_dir() -> Result<PathBuf> {
    env::current_dir()
        .context("could not determine the current directory")?
        .canonicalize()
        .context("could not resolve the current directory")
}

/// Hex key IDs and fingerprints (optionally `0x`-prefixed) are OpenPGP keys; everything else
/// (public key paths, `ssh-...` literals, `key::` values) is treated as an SSH key.
pub fn signing_format(key: &str) -> &'static str {
    let hex = key.strip_prefix("0x").unwrap_or(key);
    if (8..=40).contains(&hex.len()) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        "openpgp"
    } else {
        "ssh"
    }
}

fn apply(scope: ConfigScope, profile: &Profile) -> Result<()> {
    replace(scope, "user.name", &profile.name)?;
    replace(scope, "user.email", &profile.email)?;
    match &profile.signing_key {
        Some(key) => {
            replace(scope, "user.signingkey", key)?;
            replace(scope, "commit.gpgsign", "true")?;
            replace(scope, "gpg.format", signing_format(key))?;
        }
        None => {
            unset(scope, "user.signingkey")?;
            unset(scope, "commit.gpgsign")?;
            unset(scope, "gpg.format")?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemoteUpdate {
    key: String,
    values: Vec<String>,
}

fn snapshot(scope: ConfigScope, keys: &[String]) -> Result<Vec<(String, Vec<String>)>> {
    keys.iter()
        .map(|key| Ok((key.clone(), get_all(scope, key)?)))
        .collect()
}

fn restore(scope: ConfigScope, values: &[(String, Vec<String>)]) -> Result<()> {
    for (key, stored_values) in values {
        unset(scope, key)?;
        for value in stored_values {
            add(scope, key, value)?;
        }
    }
    Ok(())
}

fn ensure_ssh_host_exists(alias: &str) -> Result<()> {
    let aliases = ssh_host_aliases()?;
    if aliases
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(alias))
    {
        Ok(())
    } else {
        let path = ssh_config_path()?;
        bail!(
            "SSH host alias '{alias}' is not declared as a literal Host in {}",
            path.display()
        );
    }
}

pub fn ssh_host_aliases() -> Result<Vec<String>> {
    let rules = ParseRule::ALLOW_UNKNOWN_FIELDS | ParseRule::ALLOW_UNSUPPORTED_FIELDS;
    let config = SshConfig::parse_default_file(rules).with_context(|| {
        format!(
            "could not parse SSH configuration {}",
            ssh_config_path()
                .unwrap_or_else(|_| PathBuf::from("~/.ssh/config"))
                .display()
        )
    })?;
    let aliases: BTreeSet<String> = config
        .get_hosts()
        .iter()
        .flat_map(|host| &host.pattern)
        .filter(|clause| !clause.negated && !clause.pattern.contains(['*', '?', '!']))
        .map(|clause| clause.pattern.clone())
        .collect();
    Ok(aliases.into_iter().collect())
}

fn ssh_config_path() -> Result<PathBuf> {
    let base_dirs = BaseDirs::new().context("could not determine the user home directory")?;
    Ok(base_dirs.home_dir().join(".ssh").join("config"))
}

fn remote_updates(scope: ConfigScope, ssh_host: Option<&str>) -> Result<Vec<RemoteUpdate>> {
    let Some(ssh_host) = ssh_host else {
        return Ok(Vec::new());
    };
    if matches!(scope, ConfigScope::Global) {
        return Ok(Vec::new());
    }
    let output = run_git(&["remote"])?;
    if !output.status.success() {
        return Err(git_failure("list", "remotes", &output));
    }
    let names = String::from_utf8(output.stdout).context("Git returned non-UTF-8 remote names")?;
    let mut updates = Vec::new();
    for name in names.lines() {
        for suffix in ["url", "pushurl"] {
            let key = format!("remote.{name}.{suffix}");
            let values = get_all(scope, &key)?;
            let rewritten: Vec<String> = values
                .iter()
                .map(|url| rewrite_ssh_url(url, ssh_host).unwrap_or_else(|| url.clone()))
                .collect();
            if values != rewritten {
                updates.push(RemoteUpdate {
                    key,
                    values: rewritten,
                });
            }
        }
    }
    Ok(updates)
}

fn apply_remote_updates(scope: ConfigScope, updates: &[RemoteUpdate]) -> Result<()> {
    for update in updates {
        unset(scope, &update.key)?;
        for value in &update.values {
            add(scope, &update.key, value)?;
        }
    }
    Ok(())
}

fn rewrite_ssh_url(url: &str, ssh_host: &str) -> Option<String> {
    if let Some(remainder) = url.strip_prefix("ssh://") {
        let (authority, path) = remainder.split_once('/')?;
        let (user, host_port) = authority
            .rsplit_once('@')
            .map_or(("", authority), |(user, host)| (user, host));
        if host_port.is_empty() || host_port.starts_with('[') {
            return None;
        }
        let port = host_port
            .find(':')
            .map(|index| &host_port[index..])
            .unwrap_or("");
        let user = if user.is_empty() {
            String::new()
        } else {
            format!("{user}@")
        };
        return Some(format!("ssh://{user}{ssh_host}{port}/{path}"));
    }

    if url.contains("://") {
        return None;
    }
    let (authority, path) = url.split_once(':')?;
    if authority.is_empty() || authority.contains('/') || path.is_empty() {
        return None;
    }
    let user = authority.rsplit_once('@').map_or("", |(user, _)| user);
    let user = if user.is_empty() {
        String::new()
    } else {
        format!("{user}@")
    };
    Some(format!("{user}{ssh_host}:{path}"))
}

fn get_first(scope: ConfigScope, key: &str) -> Result<Option<String>> {
    Ok(get_all(scope, key)?.into_iter().next())
}

fn read_identity_at(scope: ConfigScope) -> Result<GitIdentity> {
    Ok(GitIdentity {
        name: get_first(scope, "user.name")?,
        email: get_first(scope, "user.email")?,
        signing_key: get_first(scope, "user.signingkey")?,
        gpg_sign: get_first(scope, "commit.gpgsign")?,
        gpg_format: get_first(scope, "gpg.format")?,
    })
}

/// Effective `gpg.ssh.allowedSignersFile` across all scopes, if any.
pub fn allowed_signers_file() -> Result<Option<String>> {
    let output = run_git(&["config", "--get", "gpg.ssh.allowedSignersFile"])?;
    if output.status.success() {
        let value = String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")?;
        Ok(Some(value.trim().to_owned()).filter(|value| !value.is_empty()))
    } else {
        Ok(None)
    }
}

pub fn home_dir() -> Result<PathBuf> {
    Ok(BaseDirs::new()
        .context("could not determine the user home directory")?
        .home_dir()
        .to_path_buf())
}

fn config_origin(scope: ConfigScope, key: &str) -> Result<String> {
    let output = run_git(&["config", scope.argument(), "--show-origin", "--get", key])?;
    if !output.status.success() {
        return Err(git_failure("locate", key, &output));
    }

    let stdout = String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")?;
    let origin = stdout
        .split_once('\t')
        .map(|(origin, _)| origin)
        .context("Git did not report the configuration file")?;
    Ok(origin.strip_prefix("file:").unwrap_or(origin).to_owned())
}

fn get_all(scope: ConfigScope, key: &str) -> Result<Vec<String>> {
    let output = run_git(&["config", scope.argument(), "-z", "--get-all", key])?;
    if output.status.success() {
        let stdout = String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")?;
        Ok(stdout.split_terminator('\0').map(str::to_owned).collect())
    } else if output.status.code() == Some(1) {
        Ok(Vec::new())
    } else {
        Err(git_failure("read", key, &output))
    }
}

fn replace(scope: ConfigScope, key: &str, value: &str) -> Result<()> {
    let output = run_git(&["config", scope.argument(), "--replace-all", key, value])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_failure("set", key, &output))
    }
}

fn add(scope: ConfigScope, key: &str, value: &str) -> Result<()> {
    let output = run_git(&["config", scope.argument(), "--add", key, value])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_failure("restore", key, &output))
    }
}

fn unset(scope: ConfigScope, key: &str) -> Result<()> {
    let output = run_git(&["config", scope.argument(), "--unset-all", key])?;
    if output.status.success() || matches!(output.status.code(), Some(1 | 5)) {
        Ok(())
    } else {
        Err(git_failure("unset", key, &output))
    }
}

fn run_git(arguments: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(arguments)
        .output()
        .context("could not run Git; ensure 'git' is installed and available in PATH")
}

fn git_failure(action: &str, key: &str, output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow!(
        "could not {action} local Git key '{key}': {}",
        stderr.trim()
    )
}

fn print_optional(key: &str, value: &Option<String>) {
    if let Some(value) = value {
        println!("{key}={value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_profile_requires_absent_signing_values() {
        let profile = Profile {
            name: "A".into(),
            email: "a@example.com".into(),
            signing_key: None,
            ssh_host: None,
            ssh_auth_key: None,
        };
        let matching = GitIdentity {
            name: Some("A".into()),
            email: Some("a@example.com".into()),
            ..GitIdentity::default()
        };
        assert!(matching.matches(&profile));

        let signed = GitIdentity {
            signing_key: Some("KEY".into()),
            ..matching
        };
        assert!(!signed.matches(&profile));
    }

    #[test]
    fn signed_profile_requires_key_and_true() {
        let profile = Profile {
            name: "A".into(),
            email: "a@example.com".into(),
            signing_key: Some("KEY".into()),
            ssh_host: None,
            ssh_auth_key: None,
        };
        let identity = GitIdentity {
            name: Some("A".into()),
            email: Some("a@example.com".into()),
            signing_key: Some("KEY".into()),
            gpg_sign: Some("TRUE".into()),
            gpg_format: Some("ssh".into()),
        };
        assert!(identity.matches(&profile));
    }

    #[test]
    fn detects_signing_format_from_key() {
        assert_eq!(signing_format("ABCD1234"), "openpgp");
        assert_eq!(signing_format("0xABCD1234ABCD1234"), "openpgp");
        assert_eq!(signing_format("~/.ssh/id_ed25519.pub"), "ssh");
        assert_eq!(signing_format("ssh-ed25519 AAAA"), "ssh");
        assert_eq!(signing_format("deadbeef.pub"), "ssh");
    }

    #[test]
    fn openpgp_profile_requires_openpgp_format() {
        let profile = Profile {
            name: "A".into(),
            email: "a@example.com".into(),
            signing_key: Some("ABCD1234".into()),
            ssh_host: None,
            ssh_auth_key: None,
        };
        let mut identity = GitIdentity {
            name: Some("A".into()),
            email: Some("a@example.com".into()),
            signing_key: Some("ABCD1234".into()),
            gpg_sign: Some("true".into()),
            gpg_format: Some("ssh".into()),
        };
        assert!(!identity.matches(&profile));
        identity.gpg_format = Some("openpgp".into());
        assert!(identity.matches(&profile));
    }

    #[test]
    fn rewrites_only_ssh_url_hosts() {
        assert_eq!(
            rewrite_ssh_url("git@github.com:owner/repo.git", "github-work"),
            Some("git@github-work:owner/repo.git".into())
        );
        assert_eq!(
            rewrite_ssh_url("ssh://git@github.com:2222/owner/repo.git", "github-work"),
            Some("ssh://git@github-work:2222/owner/repo.git".into())
        );
        assert_eq!(
            rewrite_ssh_url("https://github.com/owner/repo.git", "github-work"),
            None
        );
    }
}
