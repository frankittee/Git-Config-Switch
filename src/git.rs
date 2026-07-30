use std::{
    env,
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::BaseDirs;

use crate::profiles::Profile;

const KEYS: [&str; 4] = [
    "user.name",
    "user.email",
    "user.signingkey",
    "commit.gpgsign",
];

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
                }
                None => self.signing_key.is_none() && self.gpg_sign.is_none(),
            }
    }

    pub fn print(&self) {
        print_optional("user.name", &self.name);
        print_optional("user.email", &self.email);
        print_optional("user.signingkey", &self.signing_key);
        print_optional("commit.gpgsign", &self.gpg_sign);
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
    Ok(GitIdentity {
        name: get_first(ConfigScope::Local, "user.name")?,
        email: get_first(ConfigScope::Local, "user.email")?,
        signing_key: get_first(ConfigScope::Local, "user.signingkey")?,
        gpg_sign: get_first(ConfigScope::Local, "commit.gpgsign")?,
    })
}

pub fn apply_profile(profile: &Profile) -> Result<String> {
    let scope = config_scope()?;
    if matches!(scope, ConfigScope::Local) {
        ensure_repository()?;
    }
    let original = snapshot(scope)?;
    let result = apply(scope, profile);
    if let Err(error) = result {
        if let Err(rollback_error) = restore(scope, &original) {
            return Err(error.context(format!("rollback also failed: {rollback_error:#}")));
        }
        return Err(error.context("Git configuration was restored"));
    }
    config_origin(scope, "user.name")
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

fn apply(scope: ConfigScope, profile: &Profile) -> Result<()> {
    replace(scope, "user.name", &profile.name)?;
    replace(scope, "user.email", &profile.email)?;
    match &profile.signing_key {
        Some(key) => {
            replace(scope, "user.signingkey", key)?;
            replace(scope, "commit.gpgsign", "true")?;
        }
        None => {
            unset(scope, "user.signingkey")?;
            unset(scope, "commit.gpgsign")?;
        }
    }
    Ok(())
}

fn snapshot(scope: ConfigScope) -> Result<Vec<(&'static str, Vec<String>)>> {
    KEYS.into_iter()
        .map(|key| Ok((key, get_all(scope, key)?)))
        .collect()
}

fn restore(scope: ConfigScope, values: &[(&str, Vec<String>)]) -> Result<()> {
    for (key, stored_values) in values {
        unset(scope, key)?;
        for value in stored_values {
            add(scope, key, value)?;
        }
    }
    Ok(())
}

fn get_first(scope: ConfigScope, key: &str) -> Result<Option<String>> {
    Ok(get_all(scope, key)?.into_iter().next())
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
    let output = run_git(&["config", scope.argument(), "--get-all", key])?;
    if output.status.success() {
        let stdout = String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")?;
        Ok(stdout.lines().map(str::to_owned).collect())
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
        };
        let identity = GitIdentity {
            name: Some("A".into()),
            email: Some("a@example.com".into()),
            signing_key: Some("KEY".into()),
            gpg_sign: Some("TRUE".into()),
        };
        assert!(identity.matches(&profile));
    }
}
