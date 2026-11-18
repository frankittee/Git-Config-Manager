mod cli;
mod doctor;
mod git;
mod input;
mod profiles;
mod prompt;
mod tui;

use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::Path,
};

use crossterm::style::Stylize;

use anyhow::{Context, Result, bail};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    if let Some(directory) = &cli.directory {
        std::env::set_current_dir(directory)
            .with_context(|| format!("could not change directory to {}", directory.display()))?;
    }
    let store = profiles::ProfileStore::from_environment()?;

    match cli.command {
        None => {
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                bail!("TUI requires an interactive terminal; use a subcommand for automation");
            }
            tui::run(store)?;
        }
        Some(cli::Command::Add {
            profile,
            name,
            email,
            signing_key,
            ssh_host,
            ssh_auth_key,
        }) => {
            let interactive = name.is_none() || email.is_none();
            let value = if interactive {
                store.ensure_available(&profile)?;
                if !io::stdin().is_terminal() {
                    bail!("interactive input requires a terminal; provide both --name and --email");
                }
                if io::stderr().is_terminal() {
                    let signing = signing_key.is_some();
                    let initial = profiles::Profile {
                        name: name.unwrap_or_default(),
                        email: email.unwrap_or_default(),
                        signing_key,
                        ssh_host,
                        ssh_auth_key: None,
                    };
                    let mut value = prompt::run(initial, signing, ssh_aliases())?;
                    value.ssh_auth_key = match ssh_auth_key {
                        Some(key) => Some(key),
                        None => prompt::ssh_auth_key()?,
                    };
                    value
                } else {
                    let mut stdin = io::stdin().lock();
                    let mut stderr = io::stderr().lock();
                    input::collect_profile(
                        name,
                        email,
                        signing_key,
                        ssh_host,
                        ssh_auth_key,
                        &mut stdin,
                        &mut stderr,
                    )?
                }
            } else {
                profiles::Profile {
                    name: name.expect("name is present in non-interactive mode"),
                    email: email.expect("email is present in non-interactive mode"),
                    signing_key,
                    ssh_host,
                    ssh_auth_key,
                }
            };
            store.add(profile.clone(), value)?;
            report_saved(&profile, "Saved", interactive);
        }
        Some(cli::Command::Edit {
            profile,
            name,
            email,
            signing_key,
            no_signing,
            ssh_host,
            no_ssh_host,
            ssh_auth_key,
            no_ssh_auth_key,
        }) => {
            let current = store.get(&profile)?;
            let has_options = name.is_some()
                || email.is_some()
                || signing_key.is_some()
                || no_signing
                || ssh_host.is_some()
                || no_ssh_host
                || ssh_auth_key.is_some()
                || no_ssh_auth_key;
            let value = if has_options {
                profiles::Profile {
                    name: name.unwrap_or(current.name),
                    email: email.unwrap_or(current.email),
                    signing_key: if no_signing {
                        None
                    } else {
                        signing_key.or(current.signing_key)
                    },
                    ssh_host: if no_ssh_host {
                        None
                    } else {
                        ssh_host.or(current.ssh_host)
                    },
                    ssh_auth_key: if no_ssh_auth_key {
                        None
                    } else {
                        ssh_auth_key.or(current.ssh_auth_key)
                    },
                }
            } else {
                if !io::stdin().is_terminal() {
                    bail!(
                        "interactive input requires a terminal; provide an edit option such as --name or --email"
                    );
                }
                if io::stderr().is_terminal() {
                    let signing = current.signing_key.is_some();
                    prompt::run(current, signing, ssh_aliases())?
                } else {
                    let mut stdin = io::stdin().lock();
                    let mut stderr = io::stderr().lock();
                    input::edit_profile(current, &mut stdin, &mut stderr)?
                }
            };
            store.update(&profile, value)?;
            report_saved(&profile, "Updated", !has_options);
        }
        Some(cli::Command::List) => {
            let names = store.list()?;
            if names.is_empty() {
                eprintln!("no profiles yet; create one with 'g add <profile>'");
            }
            for name in names {
                println!("{name}");
            }
        }
        Some(cli::Command::Show { profile }) => {
            let value = store.get(&profile)?;
            println!("name={}", value.name);
            println!("email={}", value.email);
            if let Some(signing_key) = value.signing_key {
                println!("signing_key={signing_key}");
            }
            if let Some(ssh_host) = value.ssh_host {
                println!("ssh_host={ssh_host}");
            }
            if let Some(ssh_auth_key) = value.ssh_auth_key {
                println!("ssh_auth_key={ssh_auth_key}");
            }
        }
        Some(cli::Command::Remove { profile }) => {
            store.remove(&profile)?;
            println!("{profile}");
        }
        Some(cli::Command::Use { profile }) => {
            let value = store.get(&profile)?;
            let config_file = git::apply_profile(&value)?;
            println!("Applied '{profile}' to {config_file}");
        }
        Some(cli::Command::Unuse) => match git::remove_identity()? {
            Some(config_file) => println!("Successfully removed identity from {config_file}"),
            None => println!("Nothing to remove; no identity is set"),
        },
        Some(cli::Command::Export { profile }) => {
            print!("{}", store.get(&profile)?.to_toml()?);
        }
        Some(cli::Command::Import {
            profile,
            file,
            force,
        }) => {
            let contents = read_import(file.as_deref())?;
            let value = profiles::Profile::from_toml(&contents)?;
            store.import(profile.clone(), value, force)?;
            println!("{profile}");
        }
        Some(cli::Command::Completion { shell }) => {
            print!("{}", cli::Cli::completion_script(shell.into()));
        }
        Some(cli::Command::Info { short: true }) => {
            // Shell prompts call this on every redraw, so it must never fail loudly.
            if git::ensure_repository().is_ok()
                && let Ok(current) = git::read_identity()
                && let Ok(profiles) = store.load()
                && let Some(name) = profiles.matching_name(&current)
            {
                println!("{name}");
            }
        }
        Some(cli::Command::Doctor) => {
            if !run_doctor(&store)? {
                std::process::exit(1);
            }
        }
        Some(cli::Command::Info { short: false }) => {
            git::ensure_repository()?;
            let current = git::read_identity()?;
            let profiles = store.load()?;
            if let Some(name) = profiles.matching_name(&current) {
                println!("{name}");
            } else {
                println!("unmanaged");
                current.print();
            }
        }
    }

    Ok(())
}

/// Prints one line per issue and returns whether every profile is free of errors.
fn run_doctor(store: &profiles::ProfileStore) -> Result<bool> {
    let profiles = store.load()?;
    if profiles.profiles.is_empty() {
        println!("no profiles yet; create one with 'g add <profile>'");
        return Ok(true);
    }
    let home = git::home_dir()?;
    let aliases = ssh_aliases();
    let allowed_signers = git::allowed_signers_file()?;
    let environment = doctor::Environment {
        home: &home,
        ssh_aliases: &aliases,
        allowed_signers_file: allowed_signers.as_deref(),
    };
    let mut healthy = true;
    for (name, profile) in &profiles.profiles {
        let issues = doctor::check_profile(profile, &environment);
        if issues.is_empty() {
            println!("{name}: ok");
        }
        for issue in issues {
            let label = match issue.severity {
                doctor::Severity::Error => {
                    healthy = false;
                    "error"
                }
                doctor::Severity::Warning => "warning",
            };
            println!("{name}: {label}: {}", issue.message);
        }
    }
    Ok(healthy)
}

fn ssh_aliases() -> Vec<String> {
    git::ssh_host_aliases().unwrap_or_default()
}

/// Flag-driven runs print the bare profile name on stdout. After the interactive
/// prompts (stdin and stderr are terminals) a green success line goes to stderr
/// instead; stdout still gets the name when it is redirected, so `$(g add x)` works.
fn report_saved(profile: &str, verb: &str, interactive: bool) {
    let prompted = interactive && io::stdin().is_terminal() && io::stderr().is_terminal();
    if prompted {
        eprintln!("{}", format!("✓ {verb} profile `{profile}`").green().bold());
        if !io::stdout().is_terminal() {
            println!("{profile}");
        }
    } else {
        println!("{profile}");
    }
}

fn read_import(file: Option<&Path>) -> Result<String> {
    match file {
        Some(path) if path != Path::new("-") => {
            fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))
        }
        _ => {
            let mut contents = String::new();
            io::stdin()
                .read_to_string(&mut contents)
                .context("could not read standard input")?;
            Ok(contents)
        }
    }
}
