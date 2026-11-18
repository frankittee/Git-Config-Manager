---
name: g
description: Manage multiple Git identities with the g CLI. Use when a user wants to create, inspect, apply, remove, import, or export Git profiles (work, personal, open-source user.name, user.email, signing key, SSH host alias), switch the identity of a repository, or asks which g profile is active.
---

# g

Use `g` rather than editing `~/.config/git-config-switch/config.toml` or running `git config user.*` by hand.

## Safety and scope

- Profile data is personal. Do not echo emails or signing keys unnecessarily, and do not paste `g show` or `g export` output into commits, issues, or logs.
- `g add`, `edit`, `remove`, and `import` modify the profile store (`$HOME/.config/git-config-switch/config.toml`, or `$G_CONFIG_DIR/config.toml`). Confirm with the user before removing or overwriting (`import --force`) a profile.
- `g use` and `g unuse` change Git configuration. In a repository they change only that repository's local config. When run from the user's home directory they change the global Git config instead, so check `pwd` first and never run them from `$HOME` unless a global change is intended.
- Before changing a repository, identify the target with `git rev-parse --show-toplevel`, or pass it explicitly with `g -C /path/to/repo ...` (the option goes before the subcommand).
- Local scope requires a Git repository; `add`, `edit`, `list`, `show`, `export`, and `import` work from any directory.
- Prompts and the bare `g` terminal UI need a TTY. In automation always pass `--name` and `--email` to `add`, and an option such as `--email` to `edit`. Do not run bare `g`.

## Check availability

Run `command -v g` or `g --version` first. If missing, tell the user and offer the installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/frankittee/Git-Config-Manager/main/install.sh | sh
```

## Manage profiles

```sh
g list
g add work --name "Jane Doe" --email jane@company.example
g add work-signed --name "Jane Doe" --email jane@company.example --signing-key ABC123 --ssh-host github-work
g edit work --email new@company.example
g edit work --no-signing
g edit work --no-ssh-host
g show work
g remove work
```

- `g list` prints one name per line; with no profiles it prints a hint on stderr and exits 0.
- A missing profile exits 1 and suggests `g list`.
- `--signing-key` also enables `commit.gpgsign` and sets `gpg.format` automatically: `openpgp` for hex key IDs or fingerprints (such as `ABC123DE`), `ssh` otherwise.
- In interactive add, leave `Signing key` blank to skip signing.
- `--ssh-host` must be a literal `Host` alias declared in `~/.ssh/config` (or an included file); wildcards are rejected.
- Use `g add <profile> --name <name> --email <email> --ssh-auth-key <key-or-path>` or `g edit <profile> --ssh-auth-key <key-or-path>` to save a quoted OpenSSH public key line or a public key file path for authentication. Load its matching private key into `ssh-agent`. Use `g edit <profile> --no-ssh-auth-key` to clear the key.

## Apply, verify, and remove

```sh
g use work
g info
g info --short
g doctor
g -C /path/to/repo use work
g -C /path/to/repo info
g unuse
```

- `g use` sets `user.name` and `user.email`. With a signing key it also sets `user.signingkey`, `commit.gpgsign=true`, and `gpg.format` (`openpgp` for hex key IDs, `ssh` otherwise); without one it removes those three keys.
- With an `ssh_host`, `g use` rewrites SSH fetch and push URLs of every remote in a repository to go through that alias. HTTPS and local URLs are left alone. Global scope never rewrites remotes.
- To restore the original remote URLs after removing a profile's alias, run `g edit <profile> --no-ssh-host` followed by `g use <profile>`. This uses backups recorded by earlier rewrites in the repository's Git config.
- With a `ssh_auth_key`, `g use` requires repository scope, copies the public key into `.git/ssh/g-auth.pub`, and sets `core.sshCommand` for both SSH push and fetch. OpenSSH skips its config files unless an `ssh_host` is configured. Applying a profile without an SSH authentication key or running `g unuse` restores the former SSH command.
- `g info` prints the matching profile name, or `unmanaged` plus the current values. `g info --short` prints only the name and never fails, which suits shell prompts.
- `g doctor` checks every saved profile for a missing SSH signing key file, an undeclared `ssh_host` alias, and an unset `gpg.ssh.allowedSignersFile` (warning only). It exits 1 when any error is found.
- `g unuse` unsets the five identity keys in the current scope and prints which file changed, or says nothing needed removing. It does not undo remote URL rewrites; use `g use <other-profile>` to rewrite them to another alias.
- Writes are rolled back automatically if any step fails.

## Import and export

```sh
g export work > work.toml
g import work-copy work.toml
g export work | g import work2
g import work work.toml --force
```

The format is plain TOML with `name`, `email`, and optional `signing_key`, `ssh_host`, and `ssh_auth_key`. `g import` reads standard input when the file is omitted or `-`, validates the profile, and refuses to overwrite an existing profile without `--force`. Keep exported files private.

## Completion

`g completion <bash|zsh|fish|elvish|nu|powershell>` prints a completion script. Only install one if the user asks.
