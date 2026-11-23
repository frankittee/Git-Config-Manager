use std::{
    collections::BTreeSet,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::BaseDirs;
use ssh2_config_rs::{ParseRule, SshConfig};
use tempfile::NamedTempFile;

use crate::profiles::Profile;

const IDENTITY_KEYS: [(&str, &str); 5] = [
    ("user.name", "g.previousUserName"),
    ("user.email", "g.previousUserEmail"),
    ("user.signingkey", "g.previousSigningKey"),
    ("commit.gpgsign", "g.previousCommitGpgSign"),
    ("gpg.format", "g.previousGpgFormat"),
];
const MANAGED_IDENTITY: &str = "g.identityManaged";
const SSH_COMMAND: &str = "core.sshCommand";
const ALLOWED_SIGNERS: &str = "gpg.ssh.allowedSignersFile";
const MANAGED_SIGNERS: &str = "g.managedAllowedSignersFile";
const SSH_KEY_PREFIXES: [&str; 3] = ["ssh-", "ecdsa-", "sk-"];
const SSH_MANAGED: &str = "g.sshAuthKeyManaged";
const SSH_PREVIOUS: &str = "g.previousSshCommand";
const SSH_KEYS: [&str; 3] = [SSH_COMMAND, SSH_MANAGED, SSH_PREVIOUS];
const SIGNER_KEYS: [&str; 2] = [ALLOWED_SIGNERS, MANAGED_SIGNERS];
const AUTH_KEY_FILE: &str = "g-auth.pub";

/// Config keys with all of their values, in the order Git reports them.
type ConfigValues = Vec<(String, Vec<String>)>;

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

    fn is_local(self) -> bool {
        matches!(self, Self::Local)
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
                    self.signing_key.as_deref().map(signing_config_value)
                        == Some(signing_config_value(key))
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
    if scope.is_local() {
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
    if scope.is_local() {
        ensure_repository()?;
    }
    if let Some(ssh_host) = &profile.ssh_host {
        ensure_ssh_host_exists(ssh_host)?;
    }

    let remote_updates = remote_updates(scope, profile.ssh_host.as_deref())?;
    let mut keys = identity_config_keys();
    keys.extend(remote_updates.iter().map(|(key, _)| key.clone()));
    let managed = scope.is_local() && is_true(scope, SSH_MANAGED)?;
    let ssh_auth_key = match &profile.ssh_auth_key {
        Some(path) if scope.is_local() => {
            Some(prepare_ssh_auth_key(path, profile.ssh_host.is_some())?)
        }
        Some(_) => bail!("an SSH auth key requires a Git repository"),
        None => None,
    };
    if managed || ssh_auth_key.is_some() {
        keys.extend(SSH_KEYS.map(str::to_owned));
    }
    let allowed_signers = if scope.is_local() {
        prepare_allowed_signers(profile)?
    } else {
        None
    };
    if allowed_signers.is_some() {
        keys.extend(SIGNER_KEYS.map(str::to_owned));
    }
    let original = snapshot(scope, &keys)?;
    with_rollback(scope, &original, || {
        save_identity(scope)?;
        apply(scope, profile)?;
        restore(scope, &remote_updates)?;
        let origin = config_origin(scope, "user.name")?;
        if let Some((_, _, value)) = &allowed_signers {
            replace(scope, ALLOWED_SIGNERS, value)?;
            replace(scope, MANAGED_SIGNERS, value)?;
        }
        if let Some((_, _, command)) = &ssh_auth_key {
            if !managed {
                for value in get_all(scope, SSH_COMMAND)? {
                    add(scope, SSH_PREVIOUS, &value)?;
                }
                replace(scope, SSH_MANAGED, "true")?;
            }
            replace(scope, SSH_COMMAND, command)?;
        } else if managed {
            restore_ssh_command(scope)?;
        }
        // Touch files only after every config write has succeeded, because rollback restores
        // config but not files.
        if let Some((key, path, _)) = ssh_auth_key {
            key.persist(&path)
                .with_context(|| format!("could not install public key at {}", path.display()))?;
        } else if managed {
            remove_ssh_auth_key()?;
        }
        if let Some((file, path, _)) = allowed_signers {
            file.persist(&path).with_context(|| {
                format!("could not install allowed signers at {}", path.display())
            })?;
        }
        Ok(origin)
    })
}

/// Restore the identity keys and SSH command that `apply_profile` manages.
/// Returns the config file that was changed, or `None` when nothing was set.
pub fn remove_identity() -> Result<Option<String>> {
    let scope = config_scope()?;
    if scope.is_local() {
        ensure_repository()?;
    }
    let managed = scope.is_local() && is_true(scope, SSH_MANAGED)?;
    let identity_managed = is_true(scope, MANAGED_IDENTITY)?;
    let mut keys = if identity_managed {
        identity_config_keys()
    } else {
        Vec::new()
    };
    if managed {
        keys.extend(SSH_KEYS.map(str::to_owned));
    }
    let managed_signers = if scope.is_local() {
        get_all(scope, MANAGED_SIGNERS)?.pop()
    } else {
        None
    };
    if managed_signers.is_some() {
        keys.extend(SIGNER_KEYS.map(str::to_owned));
    }
    let remote_updates = remote_updates(scope, None)?;
    keys.extend(remote_updates.iter().map(|(key, _)| key.clone()));
    let original = snapshot(scope, &keys)?;
    let Some((origin_key, _)) = original.iter().find(|(_, values)| !values.is_empty()) else {
        return Ok(None);
    };
    let config_file = config_origin(scope, origin_key)?;
    with_rollback(scope, &original, || {
        if identity_managed {
            for (key, backup) in IDENTITY_KEYS {
                set_all(scope, key, &get_all(scope, backup)?)?;
                unset(scope, backup)?;
            }
            unset(scope, MANAGED_IDENTITY)?;
        }
        if managed {
            restore_ssh_command(scope)?;
        }
        if let Some(value) = managed_signers {
            if get_all(scope, ALLOWED_SIGNERS)? == [value] {
                unset(scope, ALLOWED_SIGNERS)?;
            }
            unset(scope, MANAGED_SIGNERS)?;
        }
        restore(scope, &remote_updates)?;
        if managed {
            remove_ssh_auth_key()?;
        }
        Ok(())
    })?;
    Ok(Some(config_file))
}

/// Run `change`, restoring `original` if any step fails.
fn with_rollback<T>(
    scope: ConfigScope,
    original: &ConfigValues,
    change: impl FnOnce() -> Result<T>,
) -> Result<T> {
    change().map_err(|error| match restore(scope, original) {
        Ok(()) => error.context("Git configuration was restored"),
        Err(rollback_error) => error.context(format!("rollback also failed: {rollback_error:#}")),
    })
}

fn identity_config_keys() -> Vec<String> {
    IDENTITY_KEYS
        .into_iter()
        .flat_map(|(key, backup)| [key, backup])
        .chain([MANAGED_IDENTITY])
        .map(str::to_owned)
        .collect()
}

fn save_identity(scope: ConfigScope) -> Result<()> {
    if is_true(scope, MANAGED_IDENTITY)? {
        return Ok(());
    }
    for (key, backup) in IDENTITY_KEYS {
        set_all(scope, backup, &get_all(scope, key)?)?;
    }
    replace(scope, MANAGED_IDENTITY, "true")
}

/// Whether the effective (last) value of a flag written by g is `true`.
fn is_true(scope: ConfigScope, key: &str) -> Result<bool> {
    Ok(get_all(scope, key)?
        .last()
        .is_some_and(|value| value == "true"))
}

fn restore_ssh_command(scope: ConfigScope) -> Result<()> {
    set_all(scope, SSH_COMMAND, &get_all(scope, SSH_PREVIOUS)?)?;
    unset(scope, SSH_PREVIOUS)?;
    unset(scope, SSH_MANAGED)
}

fn remove_ssh_auth_key() -> Result<()> {
    // Resolve only the fixed managed path, including for repositories configured by older versions.
    let path = repository_ssh_dir()?.join(AUTH_KEY_FILE);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("could not remove {}", path.display())),
    }
}

fn prepare_ssh_auth_key(
    source: &str,
    use_ssh_config: bool,
) -> Result<(NamedTempFile, PathBuf, String)> {
    let (contents, label) = if inline_public_key(source).is_some() {
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
    if parse_public_key(&contents).is_none() {
        bail!("{label} is not an OpenSSH public key");
    }
    let contents = format!("{}\n", contents.trim());
    validate_ssh_public_key(&contents, &label)?;

    let directory = repository_ssh_dir()?;
    let key = stage_file(&directory, &contents, "SSH public key")?;
    let path = directory.join(AUTH_KEY_FILE);
    let identity = path.to_str().context("SSH public key path is not UTF-8")?;
    if identity.contains("${") {
        bail!("SSH public key installation path contains an OpenSSH variable expression");
    }
    let identity = identity.replace('%', "%%").replace('\'', "'\\''");
    let config = if use_ssh_config { "" } else { " -F /dev/null" };
    let command = format!("ssh{config} -o IdentitiesOnly=yes -i '{identity}'");
    Ok((key, path, command))
}

fn repository_ssh_dir() -> Result<PathBuf> {
    let output = run_git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    if !output.status.success() {
        return Err(git_failure("locate", "git-common-dir", &output));
    }
    let git_dir = String::from_utf8(output.stdout).context("Git returned a non-UTF-8 directory")?;
    Ok(PathBuf::from(git_dir.strip_suffix('\n').unwrap_or(&git_dir)).join("ssh"))
}

/// Write `contents` to a temporary file in `directory`, ready to be persisted atomically.
fn stage_file(directory: &Path, contents: &str, what: &str) -> Result<NamedTempFile> {
    fs::create_dir_all(directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let mut file =
        NamedTempFile::new_in(directory).with_context(|| format!("could not stage {what}"))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("could not write {what}"))?;
    Ok(file)
}

fn validate_ssh_public_key(contents: &str, label: &str) -> Result<()> {
    let mut child = Command::new("ssh-keygen")
        .args(["-lf", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("could not run ssh-keygen to validate the SSH public key")?;
    let mut stdin = child
        .stdin
        .take()
        .context("could not open ssh-keygen input")?;
    stdin
        .write_all(contents.as_bytes())
        .context("could not pass the SSH public key to ssh-keygen")?;
    drop(stdin);
    let status = child.wait().context("could not wait for ssh-keygen")?;
    if !status.success() {
        bail!("{label} is not a valid OpenSSH public key");
    }
    Ok(())
}

/// Returns the trimmed key when `value` starts with an OpenSSH key type such as `ssh-ed25519`.
fn inline_public_key(value: &str) -> Option<&str> {
    let value = value.trim();
    value
        .split_once(char::is_whitespace)
        .is_some_and(|(kind, _)| {
            SSH_KEY_PREFIXES
                .iter()
                .any(|prefix| kind.starts_with(prefix))
        })
        .then_some(value)
}

/// Splits a single-line OpenSSH public key into its type and base64 blob.
fn parse_public_key(contents: &str) -> Option<(&str, &str)> {
    let contents = inline_public_key(contents)?;
    let mut fields = contents.split_whitespace();
    let (kind, encoded) = (fields.next()?, fields.next()?);
    let valid = contents.lines().count() == 1
        && encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte));
    valid.then_some((kind, encoded))
}

fn inline_signing_key(key: &str) -> Option<&str> {
    let key = key.trim();
    inline_public_key(key.strip_prefix("key::").unwrap_or(key))
}

fn signing_config_value(key: &str) -> std::borrow::Cow<'_, str> {
    match inline_signing_key(key) {
        Some(public_key) => format!("key::{public_key}").into(),
        None => key.into(),
    }
}

fn prepare_allowed_signers(profile: &Profile) -> Result<Option<(NamedTempFile, PathBuf, String)>> {
    let Some(public_key) = profile.signing_key.as_deref().and_then(inline_signing_key) else {
        return Ok(None);
    };
    let configured = allowed_signers_file()?;
    let directory = repository_ssh_dir()?;
    let path = directory.join("g-allowed-signers");
    let absolute = path.to_str().context("allowed signers path is not UTF-8")?;
    let relative = ".git/ssh/g-allowed-signers";
    let root = run_git(&["rev-parse", "--show-toplevel"])?;
    let ordinary_repository = root.status.success()
        && std::str::from_utf8(&root.stdout)
            .ok()
            .and_then(|root| root.strip_suffix('\n'))
            .is_some_and(|root| PathBuf::from(root).join(relative) == path);
    let value = if ordinary_repository {
        relative
    } else {
        absolute
    };
    let local = get_all(ConfigScope::Local, ALLOWED_SIGNERS)?;
    let managed = get_all(ConfigScope::Local, MANAGED_SIGNERS)?;
    if let Some(configured) = configured.as_deref() {
        let recognized = configured == absolute
            || configured == relative && (ordinary_repository || managed == [relative]);
        if !recognized || local != [configured] {
            return Ok(None);
        }
    } else if !local.is_empty() {
        return Ok(None);
    }
    let value = value.to_owned();
    let email = profile.email.as_str();
    if email.starts_with('#')
        || email
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || ",*!?\"\\".contains(ch))
    {
        bail!("Git author email must be a literal SSH allowed signers principal");
    }
    let Some((kind, encoded)) = parse_public_key(public_key) else {
        bail!("signing key is not an OpenSSH public key");
    };
    validate_ssh_public_key(&format!("{kind} {encoded}\n"), "signing key")?;

    let entry = format!("{email} {kind} {encoded}");
    let mut contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", path.display()));
        }
    };
    if !contents.lines().any(|line| line == entry) {
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&entry);
        contents.push('\n');
    }
    let file = stage_file(&directory, &contents, "allowed signers")?;
    Ok(Some((file, path, value)))
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
            replace(scope, "user.signingkey", &signing_config_value(key))?;
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

fn snapshot(scope: ConfigScope, keys: &[String]) -> Result<ConfigValues> {
    keys.iter()
        .map(|key| Ok((key.clone(), get_all(scope, key)?)))
        .collect()
}

fn restore(scope: ConfigScope, values: &ConfigValues) -> Result<()> {
    for (key, stored_values) in values {
        set_all(scope, key, stored_values)?;
    }
    Ok(())
}

/// Replace every value of `key`; an empty list leaves the key unset.
fn set_all(scope: ConfigScope, key: &str, values: &[String]) -> Result<()> {
    unset(scope, key)?;
    for value in values {
        add(scope, key, value)?;
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

/// Remote URL keys (and their g backups) whose values must change, with the new values.
fn remote_updates(scope: ConfigScope, ssh_host: Option<&str>) -> Result<ConfigValues> {
    if !scope.is_local() {
        return Ok(Vec::new());
    }
    let output = run_git(&["remote"])?;
    if !output.status.success() {
        return Err(git_failure("list", "remotes", &output));
    }
    let names = String::from_utf8(output.stdout).context("Git returned non-UTF-8 remote names")?;
    let mut updates = Vec::new();
    for name in names.lines() {
        // Git config keys are case-insensitive and accept fewer characters than remote names.
        let encoded_name: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
        for suffix in ["url", "pushurl"] {
            let key = format!("remote.{name}.{suffix}");
            let backup_key = format!("g.remote-{encoded_name}-original-{suffix}");
            let applied_key = format!("g.remote-{encoded_name}-applied-{suffix}");
            let backup = get_all(scope, &backup_key)?;
            let applied = get_all(scope, &applied_key)?;
            let values = get_all(scope, &key)?;
            let original = if !backup.is_empty() && applied == values {
                &backup
            } else {
                &values
            };
            let rewritten: Vec<String> = match ssh_host {
                Some(host) => original
                    .iter()
                    .map(|url| rewrite_ssh_url(url, host).unwrap_or_else(|| url.clone()))
                    .collect(),
                None => original.clone(),
            };
            let (next_backup, next_applied) = if *original == rewritten {
                (Vec::new(), Vec::new())
            } else {
                (original.clone(), rewritten.clone())
            };
            for (key, current, next) in [
                (key, values, rewritten),
                (backup_key, backup, next_backup),
                (applied_key, applied, next_applied),
            ] {
                if current != next {
                    updates.push((key, next));
                }
            }
        }
    }
    Ok(updates)
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
    let output = run_git(&["config", "--get", ALLOWED_SIGNERS])?;
    if output.status.success() {
        let value = String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")?;
        Ok(Some(value.strip_suffix('\n').unwrap_or(&value).to_owned())
            .filter(|value| !value.is_empty()))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        Err(git_failure("read", ALLOWED_SIGNERS, &output))
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
