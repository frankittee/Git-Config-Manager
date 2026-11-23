use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use tempfile::TempDir;

struct TestContext {
    config: TempDir,
    home: TempDir,
    repository: TempDir,
}

impl TestContext {
    fn new(repository: bool) -> Self {
        let context = Self {
            config: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
            repository: tempfile::tempdir().unwrap(),
        };
        if repository {
            let status = Command::new("git")
                .arg("init")
                .arg("-q")
                .current_dir(context.repository.path())
                .status()
                .unwrap();
            assert!(status.success());
        }
        context
    }

    fn g(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_g"))
            .args(args)
            .env("HOME", self.home.path())
            .env("G_CONFIG_DIR", self.config.path())
            .env("GIT_CONFIG_GLOBAL", self.config.path().join("gitconfig"))
            .current_dir(self.repository.path())
            .output()
            .unwrap()
    }

    fn g_in(&self, directory: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_g"))
            .args(args)
            .env("HOME", self.home.path())
            .env("G_CONFIG_DIR", self.config.path())
            .env("GIT_CONFIG_GLOBAL", self.config.path().join("gitconfig"))
            .current_dir(directory)
            .output()
            .unwrap()
    }

    fn g_with_stdin(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_g"))
            .args(args)
            .env("HOME", self.home.path())
            .env("G_CONFIG_DIR", self.config.path())
            .env("GIT_CONFIG_GLOBAL", self.config.path().join("gitconfig"))
            .current_dir(self.repository.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn g_from_home(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_g"))
            .args(args)
            .env("HOME", self.repository.path())
            .env("G_CONFIG_DIR", self.config.path())
            .env("GIT_CONFIG_GLOBAL", self.config.path().join("gitconfig"))
            .current_dir(self.repository.path())
            .output()
            .unwrap()
    }

    fn git_value(&self, key: &str) -> Option<String> {
        let output = Command::new("git")
            .args(["config", "--local", "--get", key])
            .current_dir(self.repository.path())
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
    }

    fn global_git_value(&self, key: &str) -> Option<String> {
        let output = Command::new("git")
            .args(["config", "--global", "--get", key])
            .env("HOME", self.repository.path())
            .env("GIT_CONFIG_GLOBAL", self.config.path().join("gitconfig"))
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
    }

    fn git_values(&self, key: &str) -> Vec<String> {
        let output = Command::new("git")
            .args(["config", "--local", "--get-all", key])
            .current_dir(self.repository.path())
            .output()
            .unwrap();
        if output.status.success() {
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        }
    }

    fn ssh_config(&self, contents: &str) {
        self.ssh_config_file("config", contents);
    }

    fn ssh_config_file(&self, path: &str, contents: &str) {
        let directory = self.home.path().join(".ssh");
        let path = directory.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(self.repository.path())
            .status()
            .unwrap();
        assert!(status.success());
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", stderr(output));
}

fn add(context: &TestContext, profile: &str, name: &str, email: &str) {
    assert_success(&context.g(&["add", profile, "--name", name, "--email", email]));
}

#[test]
fn bare_command_requires_a_terminal() {
    let context = TestContext::new(true);
    let output = context.g(&[]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("TUI requires an interactive terminal"));
}

#[test]
fn use_completes_saved_profile_names() {
    let context = TestContext::new(false);
    add(&context, "personal", "Personal", "personal@example.com");
    add(&context, "work", "Work", "work@example.com");

    for (line, expected) in [
        ("g use ", "personal\nwork\n"),
        ("g use wo", "work\n"),
        ("g -C . use wo", "work\n"),
        ("./target/release/g use wo", "work\n"),
    ] {
        let output = context.g(&["__complete_word__", "--shell", "fish", "--line", line]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(stdout(&output), expected);
    }
}

#[test]
fn profile_management_round_trip() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");
    add(&context, "alpha", "Alpha User", "alpha@example.com");

    assert_eq!(stdout(&context.g(&["list"])), "alpha\nwork\n");
    let shown = stdout(&context.g(&["show", "work"]));
    assert!(shown.contains("name=Work User\n"));
    assert!(shown.contains("email=work@example.com\n"));

    let duplicate = context.g(&[
        "add",
        "work",
        "--name",
        "New User",
        "--email",
        "new@example.com",
    ]);
    assert!(!duplicate.status.success());
    assert!(stderr(&duplicate).contains("g edit work"));
    assert!(stdout(&context.g(&["show", "work"])).contains("name=Work User"));

    assert_success(&context.g(&["remove", "work"]));
    assert!(!context.g(&["show", "work"]).status.success());
}

#[test]
fn edits_profile_fields_non_interactively() {
    let context = TestContext::new(true);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Old User",
        "--email",
        "old@example.com",
        "--signing-key",
        "OLDKEY",
    ]));

    assert_success(&context.g(&["edit", "work", "--email", "new@example.com"]));
    let shown = stdout(&context.g(&["show", "work"]));
    assert!(shown.contains("name=Old User\n"));
    assert!(shown.contains("email=new@example.com\n"));
    assert!(shown.contains("signing_key=OLDKEY\n"));

    assert_success(&context.g(&["edit", "work", "--name", "New User", "--no-signing"]));
    let shown = stdout(&context.g(&["show", "work"]));
    assert!(shown.contains("name=New User\n"));
    assert!(!shown.contains("signing_key="));
}

#[test]
fn edits_and_clears_ssh_host() {
    let context = TestContext::new(true);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "github-work",
    ]));
    assert!(stdout(&context.g(&["show", "work"])).contains("ssh_host=github-work\n"));

    assert_success(&context.g(&["edit", "work", "--no-ssh-host"]));
    assert!(!stdout(&context.g(&["show", "work"])).contains("ssh_host="));
}

#[test]
fn edit_validates_target_and_non_terminal_input() {
    let context = TestContext::new(true);
    let missing = context.g(&["edit", "missing", "--name", "New User"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("does not exist"));

    add(&context, "work", "Work User", "work@example.com");
    let interactive = context.g(&["edit", "work"]);
    assert!(!interactive.status.success());
    assert!(stderr(&interactive).contains("interactive input requires a terminal"));

    let conflicting = context.g(&["edit", "work", "--signing-key", "KEY", "--no-signing"]);
    assert!(!conflicting.status.success());
    assert!(stderr(&conflicting).contains("cannot be used with"));
}

#[test]
fn switches_ssh_auth_keys_and_restores_the_previous_command() {
    let context = TestContext::new(true);
    let previous = "ssh -o BatchMode=yes";
    context.git(&["config", "--local", "core.sshCommand", previous]);
    let installed = context.repository.path().join(".git/ssh/g-auth.pub");
    for profile in ["personal", "work"] {
        let private_key = context.home.path().join(format!("{profile} key"));
        assert_success(
            &Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&private_key)
                .output()
                .unwrap(),
        );
        let public_key = private_key.with_extension("pub");
        let expected = fs::read_to_string(&public_key).unwrap();
        let ssh_auth_key = if profile == "personal" {
            format!("~/{profile} key.pub")
        } else {
            fs::remove_file(&public_key).unwrap();
            expected.clone()
        };
        add(&context, profile, profile, "git@example.com");
        assert_success(&context.g(&["edit", profile, "--ssh-auth-key", &ssh_auth_key]));
        assert_success(&context.g(&["use", profile]));
        assert_eq!(fs::read_to_string(&installed).unwrap(), expected);
        let command = context.git_value("core.sshCommand").unwrap();
        let config = Command::new("sh")
            .args(["-c", &format!("{command} -G git@github.com")])
            .output()
            .unwrap();
        assert_success(&config);
        let config = stdout(&config);
        assert!(config.contains("identitiesonly yes"));
        let identity = format!(
            "identityfile {}",
            installed.canonicalize().unwrap().display()
        );
        assert_eq!(
            config
                .lines()
                .filter(|line| line.starts_with("identityfile "))
                .collect::<Vec<_>>(),
            [identity.as_str()]
        );
    }

    let previous_key = fs::read(&installed).unwrap();
    assert_success(&context.g(&["edit", "personal", "--ssh-auth-key", "~/personal key"]));
    assert!(!context.g(&["use", "personal"]).status.success());
    assert_eq!(fs::read(&installed).unwrap(), previous_key);
    assert_eq!(context.git_value("user.name").as_deref(), Some("work"));

    assert_success(&context.g(&["edit", "personal", "--no-ssh-auth-key"]));
    assert_success(&context.g(&["use", "personal"]));
    assert_eq!(
        context.git_value("core.sshCommand").as_deref(),
        Some(previous)
    );
    assert!(!installed.exists());
    assert_success(&context.g(&["use", "work"]));
    assert_success(&context.g(&["unuse"]));
    assert_eq!(
        context.git_value("core.sshCommand").as_deref(),
        Some(previous)
    );
    assert!(!installed.exists());
}

#[test]
fn applies_profiles_and_reports_current() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");

    let use_output = context.g(&["use", "work"]);
    assert_success(&use_output);
    assert_eq!(stdout(&use_output), "Applied 'work' to .git/config\n");
    assert_eq!(context.git_value("user.name").as_deref(), Some("Work User"));
    assert_eq!(
        context.git_value("user.email").as_deref(),
        Some("work@example.com")
    );
    assert_eq!(stdout(&context.g(&["info"])), "work\n");
}

#[test]
fn use_from_home_applies_profile_globally() {
    let context = TestContext::new(false);
    add(&context, "work", "Work User", "work@example.com");

    let use_output = context.g_from_home(&["use", "work"]);
    assert_success(&use_output);
    assert_eq!(
        stdout(&use_output),
        format!(
            "Applied 'work' to {}\n",
            context.config.path().join("gitconfig").display()
        )
    );
    assert_eq!(
        context.global_git_value("user.name").as_deref(),
        Some("Work User")
    );
    assert_eq!(
        context.global_git_value("user.email").as_deref(),
        Some("work@example.com")
    );
}

#[test]
fn inline_ssh_signing_keys_accumulate_in_repository_allowed_signers() {
    let context = TestContext::new(true);
    let mut expected = String::new();
    for profile in ["personal", "work"] {
        let email = format!("{profile}@example.com");
        let private_key = context.home.path().join(profile);
        assert_success(
            &Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&private_key)
                .output()
                .unwrap(),
        );
        let public_key = fs::read_to_string(private_key.with_extension("pub")).unwrap();
        let public_key = public_key.trim();
        let bare_key = public_key
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        expected.push_str(&format!("{email} {bare_key}\n"));
        add(&context, profile, profile, &email);
        assert_success(&context.g(&["edit", profile, "--signing-key", public_key]));
        assert_success(&context.g(&["use", profile]));
        assert_eq!(
            context.git_value("user.signingkey"),
            Some(format!("key::{public_key}"))
        );
        let path = context.git_value("gpg.ssh.allowedSignersFile").unwrap();
        assert_eq!(path, ".git/ssh/g-allowed-signers");
        assert_eq!(
            context.git_value("g.managedAllowedSignersFile").as_deref(),
            Some(path.as_str())
        );
        assert_eq!(
            fs::read_to_string(context.repository.path().join(path)).unwrap(),
            expected
        );
        context.git(&[
            "-c",
            &format!("user.signingkey={}", private_key.display()),
            "commit",
            "--allow-empty",
            "-m",
            profile,
        ]);
    }
    assert_success(&context.g(&["use", "personal"]));
    let path = context.git_value("gpg.ssh.allowedSignersFile").unwrap();
    let file = context.repository.path().join(path);
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    context.git(&["verify-commit", "HEAD", "HEAD^"]);
    let worktree = context.home.path().join("linked");
    context.git(&["worktree", "add", "--detach", worktree.to_str().unwrap()]);
    assert_success(&context.g_in(&worktree, &["use", "work"]));
    let absolute = file.canonicalize().unwrap();
    assert_eq!(
        context.git_value("gpg.ssh.allowedSignersFile").as_deref(),
        absolute.to_str()
    );
    assert_success(&context.g(&["use", "personal"]));
    assert_eq!(
        context.git_value("gpg.ssh.allowedSignersFile").as_deref(),
        Some(".git/ssh/g-allowed-signers")
    );
    assert_success(&context.g(&["unuse"]));
    assert_eq!(context.git_value("gpg.ssh.allowedSignersFile"), None);
    assert_eq!(context.git_value("g.managedAllowedSignersFile"), None);
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);

    // Migrate the previous absolute-path configuration on the next application.
    context.git(&[
        "config",
        "--local",
        "gpg.ssh.allowedSignersFile",
        file.canonicalize().unwrap().to_str().unwrap(),
    ]);
    assert_success(&context.g(&["use", "work"]));
    assert_eq!(
        context.git_value("gpg.ssh.allowedSignersFile").as_deref(),
        Some(".git/ssh/g-allowed-signers")
    );

    context.git(&[
        "config",
        "--local",
        "gpg.ssh.allowedSignersFile",
        "custom-signers",
    ]);
    assert_success(&context.g(&["use", "work"]));
    assert_eq!(
        context.git_value("gpg.ssh.allowedSignersFile").as_deref(),
        Some("custom-signers")
    );
    assert_success(&context.g(&["unuse"]));
    assert_eq!(
        context.git_value("gpg.ssh.allowedSignersFile").as_deref(),
        Some("custom-signers")
    );
    assert_eq!(context.git_value("g.managedAllowedSignersFile"), None);
}

#[test]
fn signing_is_enabled_and_then_cleared() {
    let context = TestContext::new(true);
    assert_success(&context.g(&[
        "add",
        "signed",
        "--name",
        "Signed User",
        "--email",
        "signed@example.com",
        "--signing-key",
        "~/.ssh/id_ed25519.pub",
    ]));
    add(&context, "plain", "Plain User", "plain@example.com");

    assert_success(&context.g(&["use", "signed"]));
    assert_eq!(
        context.git_value("user.signingkey").as_deref(),
        Some("~/.ssh/id_ed25519.pub")
    );
    assert_eq!(context.git_value("commit.gpgsign").as_deref(), Some("true"));
    assert_eq!(context.git_value("gpg.format").as_deref(), Some("ssh"));

    assert_success(&context.g(&["use", "plain"]));
    assert_eq!(context.git_value("user.signingkey"), None);
    assert_eq!(context.git_value("commit.gpgsign"), None);
    assert_eq!(context.git_value("gpg.format"), None);
}

#[test]
fn hex_signing_key_uses_openpgp_format() {
    let context = TestContext::new(true);
    assert_success(&context.g(&[
        "add",
        "gpg",
        "--name",
        "Gpg User",
        "--email",
        "gpg@example.com",
        "--signing-key",
        "ABC123DE",
    ]));

    assert_success(&context.g(&["use", "gpg"]));
    assert_eq!(context.git_value("commit.gpgsign").as_deref(), Some("true"));
    assert_eq!(context.git_value("gpg.format").as_deref(), Some("openpgp"));
}

#[test]
fn use_rewrites_all_ssh_remote_urls() {
    let context = TestContext::new(true);
    context.ssh_config("Host github-work github-personal\n  HostName github.com\n");
    context.git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:owner/project.git",
    ]);
    context.git(&[
        "config",
        "--local",
        "--add",
        "remote.origin.pushurl",
        "ssh://git@gitlab.com:2222/group/project.git",
    ]);
    context.git(&[
        "config",
        "--local",
        "--add",
        "remote.origin.pushurl",
        "git@github.com:owner/project-push.git",
    ]);
    context.git(&[
        "remote",
        "add",
        "upstream",
        "https://github.com/upstream/project.git",
    ]);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "github-work",
    ]));

    assert_success(&context.g(&["use", "work"]));
    assert_eq!(
        context.git_values("g.remote-6f726967696e-original-url"),
        vec!["git@github.com:owner/project.git"]
    );
    assert_eq!(
        context.git_values("g.remote-6f726967696e-applied-url"),
        vec!["git@github-work:owner/project.git"]
    );
    assert!(
        context
            .git_values("remote.origin.g-original-url")
            .is_empty()
    );

    assert_eq!(
        context.git_values("remote.origin.url"),
        vec!["git@github-work:owner/project.git"]
    );
    assert_eq!(
        context.git_values("remote.origin.pushurl"),
        vec![
            "ssh://git@github-work:2222/group/project.git",
            "git@github-work:owner/project-push.git",
        ]
    );
    assert_eq!(
        context.git_values("remote.upstream.url"),
        vec!["https://github.com/upstream/project.git"]
    );
    assert_success(&context.g(&["use", "work"]));
    assert_success(&context.g(&["edit", "work", "--ssh-host", "github-personal"]));
    assert_success(&context.g(&["use", "work"]));
    assert_eq!(
        context.git_values("remote.origin.url"),
        vec!["git@github-personal:owner/project.git"]
    );
    assert_success(&context.g(&["edit", "work", "--no-ssh-host"]));
    assert_success(&context.g(&["use", "work"]));
    assert_eq!(
        context.git_values("remote.origin.url"),
        vec!["git@github.com:owner/project.git"]
    );
    assert_eq!(
        context.git_values("remote.origin.pushurl"),
        vec![
            "ssh://git@gitlab.com:2222/group/project.git",
            "git@github.com:owner/project-push.git"
        ]
    );
    assert!(
        context
            .git_values("g.remote-6f726967696e-original-url")
            .is_empty()
    );
    assert!(
        context
            .git_values("g.remote-6f726967696e-applied-pushurl")
            .is_empty()
    );

    assert_success(&context.g(&["edit", "work", "--ssh-host", "github-work"]));
    assert_success(&context.g(&["use", "work"]));
    context.git(&[
        "remote",
        "set-url",
        "origin",
        "git@github.com:new/project.git",
    ]);
    assert_success(&context.g(&["unuse"]));
    assert_eq!(
        context.git_values("remote.origin.url"),
        vec!["git@github.com:new/project.git"]
    );
    assert_eq!(
        context.git_values("remote.origin.pushurl"),
        vec![
            "ssh://git@gitlab.com:2222/group/project.git",
            "git@github.com:owner/project-push.git"
        ]
    );
    for key in [
        "original-url",
        "applied-url",
        "original-pushurl",
        "applied-pushurl",
    ] {
        assert!(
            context
                .git_values(&format!("g.remote-6f726967696e-{key}"))
                .is_empty()
        );
    }
}

#[test]
fn included_ssh_host_is_accepted() {
    let context = TestContext::new(true);
    context.ssh_config("Include aliases/*.conf\n");
    context.ssh_config_file(
        "aliases/work.conf",
        "Include nested.conf\nHost github-work\n  HostName github.com\n",
    );
    context.ssh_config_file("nested.conf", "Host github-personal\n");
    context.git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:owner/project.git",
    ]);
    context.git(&["config", "--local", "user.name", "Original User"]);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "github-work",
    ]));

    assert_success(&context.g(&["use", "work"]));
    assert_eq!(context.git_value("user.name").as_deref(), Some("Work User"));
    assert_eq!(
        context.git_values("remote.origin.url"),
        vec!["git@github-work:owner/project.git"]
    );
}

#[test]
fn missing_or_non_literal_ssh_host_preserves_configuration() {
    let context = TestContext::new(true);
    context.ssh_config("Host work-*\n  HostName github.com\n");
    context.git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:owner/project.git",
    ]);
    context.git(&["config", "--local", "user.name", "Original User"]);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "work-main",
    ]));

    let output = context.g(&["use", "work"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not declared as a literal Host"));
    assert_eq!(
        context.git_value("user.name").as_deref(),
        Some("Original User")
    );
    assert_eq!(
        context.git_values("remote.origin.url"),
        vec!["git@github.com:owner/project.git"]
    );
}

#[test]
fn unmanaged_and_failure_paths_are_clear() {
    let context = TestContext::new(true);
    let unmanaged = context.g(&["info"]);
    assert_success(&unmanaged);
    assert_eq!(stdout(&unmanaged), "unmanaged\n");
    assert!(!context.g(&["use", "missing"]).status.success());

    let outside = TestContext::new(false);
    add(&outside, "work", "Work User", "work@example.com");
    let use_output = outside.g(&["use", "work"]);
    assert!(!use_output.status.success());
    assert!(stderr(&use_output).contains("not inside a Git repository"));
    let info_output = outside.g(&["info"]);
    assert!(!info_output.status.success());
    assert!(stderr(&info_output).contains("not inside a Git repository"));
}

#[test]
fn required_arguments_are_enforced() {
    let context = TestContext::new(true);
    let output = context.g(&["add", "incomplete", "--name", "Name"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("interactive input requires a terminal"));
    assert!(stderr(&output).contains("--name and --email"));
}

#[test]
fn duplicate_is_rejected_before_interactive_input() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");

    let output = context.g(&["add", "work"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("g edit work"));
    assert!(!stderr(&output).contains("interactive input requires a terminal"));
}

#[test]
fn profile_file_uses_expected_location() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");
    assert!(
        Path::new(context.config.path())
            .join("config.toml")
            .exists()
    );
}

#[test]
fn directory_option_applies_profile_from_outside_repository() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");
    let outside = tempfile::tempdir().unwrap();
    let repository = context.repository.path().to_str().unwrap();

    assert_success(&context.g_in(outside.path(), &["-C", repository, "use", "work"]));
    assert_eq!(context.git_value("user.name").as_deref(), Some("Work User"));

    let info = context.g_in(outside.path(), &["-C", repository, "info"]);
    assert_eq!(stdout(&info), "work\n");

    let missing = context.g_in(outside.path(), &["-C", "/nonexistent/g-test", "info"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("could not change directory"));
}

#[test]
fn unuse_removes_identity_and_signing_keys() {
    let context = TestContext::new(true);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--signing-key",
        "KEY",
    ]));
    assert_success(&context.g(&["use", "work"]));
    assert_eq!(context.git_value("commit.gpgsign").as_deref(), Some("true"));

    let output = context.g(&["unuse"]);
    assert_success(&output);
    assert!(stdout(&output).contains("Successfully removed identity from"));
    for key in [
        "user.name",
        "user.email",
        "user.signingkey",
        "commit.gpgsign",
        "gpg.format",
    ] {
        assert_eq!(context.git_value(key), None, "{key}");
    }
    assert_eq!(stdout(&context.g(&["info"])), "unmanaged\n");
}

#[test]
fn unuse_restores_identity_from_before_the_first_profile() {
    let context = TestContext::new(true);
    let previous = [
        ("user.name", "Original User"),
        ("user.email", "original@example.com"),
        ("user.signingkey", "0123ABCD"),
        ("commit.gpgsign", "false"),
        ("gpg.format", "openpgp"),
    ];
    for (key, value) in previous {
        context.git(&["config", "--local", key, value]);
    }
    assert_success(&context.g(&["unuse"]));
    assert_eq!(
        context.git_value("user.name").as_deref(),
        Some("Original User")
    );
    context.git(&[
        "config",
        "--local",
        "--add",
        "user.email",
        "second@example.com",
    ]);
    for profile in ["work", "personal"] {
        add(&context, profile, profile, "profile@example.com");
        assert_success(&context.g(&["use", profile]));
    }
    assert_success(&context.g(&["unuse"]));
    for (key, value) in previous {
        if key != "user.email" {
            assert_eq!(context.git_value(key).as_deref(), Some(value), "{key}");
        }
    }
    let emails = Command::new("git")
        .args(["config", "--local", "--get-all", "user.email"])
        .current_dir(context.repository.path())
        .output()
        .unwrap();
    assert_success(&emails);
    assert_eq!(
        stdout(&emails),
        "original@example.com\nsecond@example.com\n"
    );
    assert_eq!(context.git_value("g.identityManaged"), None);
    assert_eq!(context.git_value("g.previousUserName"), None);
    assert_success(&context.g(&["unuse"]));
    assert_eq!(
        context.git_value("user.name").as_deref(),
        Some("Original User")
    );
}

#[test]
fn unuse_with_nothing_set_succeeds() {
    let context = TestContext::new(true);
    let output = context.g(&["unuse"]);
    assert_success(&output);
    assert!(stdout(&output).contains("Nothing to remove"));
}

#[test]
fn unuse_requires_repository_for_local_scope() {
    let context = TestContext::new(false);
    let output = context.g(&["unuse"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not inside a Git repository"));
}

#[test]
fn unuse_from_home_removes_global_identity() {
    let context = TestContext::new(false);
    add(&context, "work", "Work User", "work@example.com");
    assert_success(&context.g_from_home(&["use", "work"]));
    assert!(context.global_git_value("user.name").is_some());

    assert_success(&context.g_from_home(&["unuse"]));
    assert_eq!(context.global_git_value("user.name"), None);
}

#[test]
fn export_and_import_round_trip() {
    let context = TestContext::new(false);
    assert_success(&context.g(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--signing-key",
        "KEY",
    ]));
    let exported = context.g(&["export", "work"]);
    assert_success(&exported);

    let imported = context.g_with_stdin(&["import", "work2"], &stdout(&exported));
    assert_success(&imported);
    assert_eq!(
        stdout(&context.g(&["show", "work2"])),
        stdout(&context.g(&["show", "work"]))
    );

    let file = context.config.path().join("profile.toml");
    fs::write(&file, stdout(&exported)).unwrap();
    assert_success(&context.g(&["import", "work3", file.to_str().unwrap()]));
    assert_eq!(
        stdout(&context.g(&["show", "work3"])),
        stdout(&context.g(&["show", "work"]))
    );
}

#[test]
fn import_refuses_duplicate_without_force() {
    let context = TestContext::new(false);
    add(&context, "work", "Work User", "work@example.com");
    let replacement = "name = \"New User\"\nemail = \"new@example.com\"\n";

    let refused = context.g_with_stdin(&["import", "work", "-"], replacement);
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("already exists"));
    assert!(stdout(&context.g(&["show", "work"])).contains("name=Work User"));

    assert_success(&context.g_with_stdin(&["import", "work", "--force"], replacement));
    assert!(stdout(&context.g(&["show", "work"])).contains("name=New User"));
}

#[test]
fn import_validates_profile() {
    let context = TestContext::new(false);
    let invalid = context.g_with_stdin(&["import", "bad"], "name = \" \"\nemail = \"a@b.c\"\n");
    assert!(!invalid.status.success());
    assert!(stderr(&invalid).contains("name must not be empty"));

    let malformed = context.g_with_stdin(&["import", "bad"], "not toml");
    assert!(!malformed.status.success());
    assert!(stderr(&malformed).contains("invalid profile TOML"));
    assert_eq!(stdout(&context.g(&["list"])), "");
}

#[test]
fn list_without_profiles_prints_hint_to_stderr() {
    let context = TestContext::new(false);
    let output = context.g(&["list"]);
    assert_success(&output);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("g add <profile>"));
}

#[test]
fn missing_profile_suggests_list() {
    let context = TestContext::new(true);
    for command in ["use", "show", "remove", "export"] {
        let output = context.g(&[command, "ghost"]);
        assert_eq!(output.status.code(), Some(1), "{command}");
        assert!(stderr(&output).contains("g list"), "{command}");
    }
}

#[test]
fn completion_prints_script_for_each_shell() {
    let context = TestContext::new(false);
    for shell in ["bash", "zsh", "fish"] {
        let output = context.g(&["completion", shell]);
        assert_success(&output);
        assert!(stdout(&output).contains("g"), "{shell}");
    }
    assert!(!context.g(&["completion", "tcsh"]).status.success());
}

#[test]
fn info_short_prints_only_the_matching_profile() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");

    let unmanaged = context.g(&["info", "--short"]);
    assert_success(&unmanaged);
    assert_eq!(stdout(&unmanaged), "");

    assert_success(&context.g(&["use", "work"]));
    assert_eq!(stdout(&context.g(&["info", "--short"])), "work\n");
}

#[test]
fn info_short_is_silent_outside_a_repository() {
    let context = TestContext::new(false);
    let output = context.g(&["info", "--short"]);
    assert_success(&output);
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "");
}

#[test]
fn doctor_reports_problems_and_fails() {
    let context = TestContext::new(false);
    add(&context, "plain", "Plain User", "plain@example.com");
    assert_success(&context.g(&[
        "add",
        "broken",
        "--name",
        "Broken User",
        "--email",
        "broken@example.com",
        "--signing-key",
        "~/.ssh/missing.pub",
    ]));

    let output = context.g(&["doctor"]);
    assert!(!output.status.success());
    let report = stdout(&output);
    assert!(report.contains("plain: ok\n"), "{report}");
    assert!(
        report.contains("broken: error: signing key file"),
        "{report}"
    );
    assert!(
        report.contains("broken: warning: gpg.ssh.allowedSignersFile is not set"),
        "{report}"
    );
}

#[test]
fn doctor_passes_for_healthy_profiles() {
    let context = TestContext::new(false);
    add(&context, "plain", "Plain User", "plain@example.com");
    let output = context.g(&["doctor"]);
    assert_success(&output);
    assert_eq!(stdout(&output), "plain: ok\n");
}
