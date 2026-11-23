<div align="center">

<img src="assets/readme-title.svg" alt="Git-Config-Manager" width="560">

**Switch Git identities without leaving your terminal.**

A fast, focused CLI for managing Git profiles per repository.

[![Release](https://img.shields.io/github/v/release/frankittee/Git-Config-Manager?style=flat-square)](https://github.com/frankittee/Git-Config-Manager/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/frankittee/Git-Config-Manager/ci.yml?style=flat-square&label=ci)](https://github.com/frankittee/Git-Config-Manager/actions/workflows/ci.yml)
[![Release build](https://img.shields.io/github/actions/workflow/status/frankittee/Git-Config-Manager/release.yml?style=flat-square&label=release)](https://github.com/frankittee/Git-Config-Manager/actions/workflows/release.yml)
![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)
[![Rust](https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square)](https://www.rust-lang.org/)

</div>

`g` keeps your work, personal, and open-source Git identities separate. Save each identity once, then apply the right name, email, signing key, and SSH host alias to the current repository in one command.

![g demo](demo/demo.gif)

```text
$ g use work
Applied 'work' to .git/config
```

## Why g?

- **Repository-first** — changes are scoped to the current repository by default, so one project never leaks its identity into another.
- **Interactive when you want it** — launch the terminal UI with `g`, or use explicit subcommands in scripts.
- **Signing-aware** — switches `user.signingkey` and `commit.gpgsign` together.
- **Multiple GitHub accounts** — optionally rewrites SSH remotes through an alias from `~/.ssh/config`.
- **Safe updates** — validates first and rolls back Git configuration if a write fails.

## Install

Install the latest release on Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Manager/main/install.sh | sh
```

The installer verifies the release checksum and places `g` in `~/.local/bin`. It prints the [shell completion setup](#shell-completion) commands after installation.

```sh
# Choose a different installation directory
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Manager/main/install.sh | \
  G_INSTALL_DIR="$HOME/bin" sh

# Install a specific version
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Manager/main/install.sh | \
  G_VERSION="0.4.0" sh
```

## Quick start

Create a profile. In a terminal, `g add` asks for each field in turn with inline [demand](https://github.com/jdx/demand) prompts below the shell prompt (no fullscreen); answered fields collapse to a single summary line:

```text
$ g add work
Git author name Ada Lovelace
Git author email ada@company.example
Signing key ~/.ssh/id_ed25519.pub
SSH host alias
Leave blank to skip · tab completes: github-personal, github-work
> github-w
 → github-work
```

Press `enter` to submit the current field. Leave `Signing key` blank to skip commit signing. Press `tab` to complete the highlighted SSH host alias from `~/.ssh/config` and `↑` or `↓` to move through suggestions. Use `ctrl+u` to clear a field or `esc` or `ctrl+c` to cancel. Flags such as `--name` prefill the prompts. After the last field a success line is printed:

```text
✓ Saved profile `work`
```

`g edit work` without options shows the same prompts prefilled with the current values (clear the SSH host alias to remove it) and ends with `✓ Updated profile `work``. The success line goes to stderr; stdout receives the bare profile name only when it is redirected, and non-terminal or flag-only runs print just the name as before.

Apply it inside a Git repository:

```sh
cd my-project
g use work
```

Check which saved profile matches the repository:

```sh
g info
g info --short   # profile name only; empty when unmanaged (for shell prompts)
g doctor
```

## Terminal UI

Run `g` without a subcommand to manage profiles from the interactive terminal interface:

```sh
g
```

| Key | Action |
| --- | --- |
| `↑` / `↓`, `j` / `k` | Navigate profiles |
| `Enter`, `u` | Apply selected profile |
| `a` | Add a profile |
| `e` | Edit selected profile |
| `d` | Delete selected profile |
| `q` | Quit |

When adding or editing a profile, use `Tab` to move between fields, `Space` to toggle commit signing, `Enter` to save, and `Esc` to cancel.

## Command reference

| Command | Description |
| --- | --- |
| `g` | Open the terminal UI |
| `g add <profile>` | Create a profile |
| `g edit <profile>` | Edit a profile |
| `g list` | List saved profiles |
| `g show <profile>` | Show profile details |
| `g use <profile>` | Apply a profile |
| `g unuse` | Remove the identity applied by `g` |
| `g info` | Show the matching active profile |
| `g info --short` | Print only the profile name, or nothing when unmanaged or outside a repository |
| `g doctor` | Check saved profiles for missing signing key files, undeclared SSH host aliases, and a missing `gpg.ssh.allowedSignersFile`; exits 1 on errors |
| `g remove <profile>` | Remove a profile |
| `g export <profile>` | Print a profile as TOML |
| `g import <profile> [file]` | Import a profile from TOML (file or stdin) |
| `g completion <shell>` | Print a shell completion script |
| `g -C <path> <command>` | Run a command as if started in `<path>` |

All profile fields can also be provided as flags, which makes `g` convenient in scripts:

```sh
g add personal \
  --name "Ada Lovelace" \
  --email "ada@example.com"

g add work \
  --name "Ada Lovelace" \
  --email "ada@company.example" \
  --signing-key "ABC123" \
  --ssh-host github-work

g edit work --email "new-address@company.example"
g edit work --no-signing
g edit work --no-ssh-host
```

Run `g <command> --help` for every available option. The terminal UI and prompts require a TTY; in CI, provide both `--name` and `--email` when adding a profile.

## Running from another directory

Like `git -C`, the global `-C <path>` option runs a command as if `g` had been started in `<path>`. Put it before the subcommand:

```sh
g -C ../my-project use work
g -C ../my-project info
```

## Removing an identity

`g use` records the existing `user.name`, `user.email`, `user.signingkey`, `commit.gpgsign`, and `gpg.format` values in the `[g]` section before applying the first profile. `g unuse` restores those values, removes keys that were originally absent, and clears the backup. Identity settings without a management record are left in place. It also restores the previous local SSH command when an SSH authentication key is active. It uses the same scope rules as `g use` (the repository's local configuration, or the global configuration from your home directory), restores the previous configuration if a write fails, and succeeds with a message when nothing is set. It also restores remote URL lists rewritten by `ssh_host` when they still match the last values applied by `g`.

## Sharing profiles

`g export` prints a profile as TOML, and `g import` reads it from a file or from standard input (omit the file or pass `-`). Import validates the profile and refuses to overwrite an existing one unless you pass `--force`.

```sh
g export work > work.toml
g import work-copy work.toml
g export work | g import work2
```

Exported files contain your email address and signing key, so keep them private.

## Shell completion

Add the matching line to your shell configuration after `g` is on your `PATH`. Open a new shell, then type `g use` followed by a space and press Tab to list saved profile names or after a partial name to complete it.

| Shell | Configuration file | Line to add |
| --- | --- | --- |
| Bash | `~/.bashrc` | `eval "$(g completion bash)"` |
| Zsh | `~/.zshrc`, after `compinit` | `eval "$(g completion zsh)"` |
| Fish | `~/.config/fish/config.fish` | `g completion fish \| source` |

If Zsh has no `compinit` call, add `autoload -Uz compinit` and `compinit` on separate lines before loading the `g` completion script.

Bash, Zsh, Fish, Elvish, Nushell, and PowerShell are supported.

## AI agents

[`skills/g/SKILL.md`](skills/g/SKILL.md) is an agent skill that teaches coding agents to manage identities with `g` safely. Copy or symlink the `skills/g` directory into your agent's skills directory, for example `~/.claude/skills/g`.

## How it works

Profiles are stored in:

```text
$HOME/.config/git-config-switch/config.toml
```

Set `G_CONFIG_DIR` to override the containing directory, for example in isolated automation or tests.

By default, `g use` updates the current repository's local Git configuration. When run directly from your home directory, it updates the global Git configuration instead.

If a profile has a signing key, `g use` sets `user.signingkey`, enables `commit.gpgsign`, and sets `gpg.format` to `openpgp` for hex key IDs or fingerprints (such as `ABC123DE`) or `ssh` for everything else (such as `~/.ssh/id_ed25519.pub`). Applying a profile without a signing key removes these settings from the selected Git configuration scope.

You can paste an OpenSSH public key directly into a profile. `g use` configures it as an inline `user.signingkey`, and signing uses the matching private key from `ssh-agent`. In a repository, `g use` also adds the profile email and public key to `.git/ssh/g-allowed-signers` and configures `gpg.ssh.allowedSignersFile` locally so `git log --show-signature` can verify signatures. Ordinary repositories use a relative path, while linked worktrees and separate Git directories use an absolute path. The local `[g]` section records the managed path so `g unuse` can remove the setting if it still points to that path. The file retains previously added signers for the next `g use`. An existing custom allowed signers configuration takes precedence.

If a profile has an `ssh_host`, `g` first verifies that the literal `Host` alias exists in `~/.ssh/config` or an included configuration file, then rewrites SSH fetch URLs and explicit push URLs for every remote to use that alias. Include paths support paths relative to `~/.ssh`, absolute paths, `~`, nested includes, and glob wildcards. HTTPS and local URLs are left unchanged. Validation or write failures leave both identity and remote configuration unchanged.

`g use` saves the original and most recently applied URLs in the local Git configuration’s `[g]` section. Run `g unuse` or apply a profile without an alias to restore the saved URLs and clear the backup records. A URL list edited manually since the last rewrite keeps its current values. Repositories rewritten before URL backups were introduced need a one-time manual correction if the original address is already missing.

### SSH authentication keys for multiple GitHub accounts

`g add work` prompts for an optional SSH authentication public key. Paste the public key, enter a `.pub` file path, or press Enter to skip.

In the TUI, press `a` to add a profile or `e` to edit the selected one. Use `Tab` to reach **SSH authentication public key**, enter the key or path, and press `Enter` to save. Both key fields accept an empty value.

Load the matching private key into `ssh-agent`, add the public key to the corresponding GitHub account, and use an SSH remote such as `git@github.com:owner/repo.git`.

```fish
ssh-add ~/.ssh/github-work
g edit work --ssh-auth-key ~/.ssh/github-work.pub
g use work
```

`g use` copies the public key to `.git/ssh/g-auth.pub` and sets the repository's `core.sshCommand`. This applies to SSH push and fetch. Without `--ssh-host`, OpenSSH skips SSH configuration files and offers the selected key. With `--ssh-host`, OpenSSH loads its normal configuration so the alias works. Keep the alias's `IdentityFile` entries limited to the same account, since OpenSSH can also offer those identities.

`--ssh-auth-key` also accepts the complete OpenSSH public key line as a quoted argument. For example, `g edit work --ssh-auth-key "$(cat ~/.ssh/github-work.pub)"` saves the key contents in the profile. `g edit work --no-ssh-auth-key` clears the saved key. Applying a profile without an SSH authentication key or running `g unuse` restores the repository's previous SSH command and removes the managed public key copy. The local `[g]` section records the copy's path relative to the Git common directory. Public key paths support `~/`. Relative paths are resolved from the directory where `g use` runs. Profiles with an SSH authentication key require repository scope.

<details>
<summary><strong>Release process</strong></summary>

Pushing a `v*` tag builds Linux x86_64/ARM64 musl binaries and native macOS Apple Silicon binaries. Intel Macs are no longer built after v1.2.0; install v1.2.0 with `G_VERSION=1.2.0` or build from source with `cargo install --git https://github.com/frankittee/Git-Config-Manager`. The tag must match the version in `Cargo.toml`.

```sh
# Runs checks, bumps Cargo.toml and Cargo.lock, prepends the git-cliff changelog section, commits, and creates a v* tag.
mise run release
git push --follow-tags
```

Normal branch pushes do not trigger release builds.

</details>

## License

Released under the MIT License.

---

<div align="center">

Built for developers who use more than one Git identity.

</div>
