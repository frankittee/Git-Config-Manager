mod cli;
mod form;
mod git;
mod input;
mod profiles;
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
                    };
                    form::run(form::Form::new(initial, signing, ssh_aliases()))?
                } else {
                    let mut stdin = io::stdin().lock();
                    let mut stderr = io::stderr().lock();
                    input::collect_profile(
                        name,
                        email,
                        signing_key,
                        ssh_host,
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
        }) => {
            let current = store.get(&profile)?;
            let has_options = name.is_some()
                || email.is_some()
                || signing_key.is_some()
                || no_signing
                || ssh_host.is_some()
                || no_ssh_host;
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
                }
            } else {
                if !io::stdin().is_terminal() {
                    bail!(
                        "interactive input requires a terminal; provide an edit option such as --name or --email"
                    );
                }
                if io::stderr().is_terminal() {
                    let signing = current.signing_key.is_some();
                    form::run(form::Form::new(current, signing, ssh_aliases()))?
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
        Some(cli::Command::Info) => {
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

fn ssh_aliases() -> Vec<String> {
    git::ssh_host_aliases().unwrap_or_default()
}

/// Flag-driven runs print the bare profile name on stdout. After the interactive
/// form (stdin and stderr are terminals) a green success line goes to stderr
/// instead; stdout still gets the name when it is redirected, so `$(g add x)` works.
fn report_saved(profile: &str, verb: &str, interactive: bool) {
    let form_shown = interactive && io::stdin().is_terminal() && io::stderr().is_terminal();
    if form_shown {
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
