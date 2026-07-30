mod cli;
mod git;
mod input;
mod profiles;

use std::io::{self, IsTerminal};

use anyhow::{Result, bail};
use clap::Parser;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    let store = profiles::ProfileStore::from_environment()?;

    match cli.command {
        cli::Command::Add {
            profile,
            name,
            email,
            signing_key,
        } => {
            let interactive = name.is_none() || email.is_none();
            let value = if interactive {
                store.ensure_available(&profile)?;
                if !io::stdin().is_terminal() {
                    bail!("interactive input requires a terminal; provide both --name and --email");
                }
                let mut stdin = io::stdin().lock();
                let mut stderr = io::stderr().lock();
                input::collect_profile(name, email, signing_key, &mut stdin, &mut stderr)?
            } else {
                profiles::Profile {
                    name: name.expect("name is present in non-interactive mode"),
                    email: email.expect("email is present in non-interactive mode"),
                    signing_key,
                }
            };
            store.add(profile.clone(), value)?;
            println!("{profile}");
        }
        cli::Command::Edit {
            profile,
            name,
            email,
            signing_key,
            no_signing,
        } => {
            let current = store.get(&profile)?;
            let has_options =
                name.is_some() || email.is_some() || signing_key.is_some() || no_signing;
            let value = if has_options {
                profiles::Profile {
                    name: name.unwrap_or(current.name),
                    email: email.unwrap_or(current.email),
                    signing_key: if no_signing {
                        None
                    } else {
                        signing_key.or(current.signing_key)
                    },
                }
            } else {
                if !io::stdin().is_terminal() {
                    bail!(
                        "interactive input requires a terminal; provide an edit option such as --name or --email"
                    );
                }
                let mut stdin = io::stdin().lock();
                let mut stderr = io::stderr().lock();
                input::edit_profile(current, &mut stdin, &mut stderr)?
            };
            store.update(&profile, value)?;
            println!("{profile}");
        }
        cli::Command::List => {
            for name in store.list()? {
                println!("{name}");
            }
        }
        cli::Command::Show { profile } => {
            let value = store.get(&profile)?;
            println!("name={}", value.name);
            println!("email={}", value.email);
            if let Some(signing_key) = value.signing_key {
                println!("signing_key={signing_key}");
            }
        }
        cli::Command::Remove { profile } => {
            store.remove(&profile)?;
            println!("{profile}");
        }
        cli::Command::Use { profile } => {
            let value = store.get(&profile)?;
            let config_file = git::apply_profile(&value)?;
            println!("Successfully write profiles into {config_file}");
        }
        cli::Command::Info => {
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
