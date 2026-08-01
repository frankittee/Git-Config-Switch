use std::{
    collections::BTreeSet,
    env,
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::BaseDirs;
use ssh2_config_rs::{ParseRule, SshConfig};

use crate::profiles::Profile;

const IDENTITY_KEYS: [&str; 4] = [
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
    let original = snapshot(scope, &keys)?;
    let result = apply(scope, profile);
    let result = result.and_then(|()| apply_remote_updates(scope, &remote_updates));
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
    })
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
            ssh_host: None,
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
        };
        let identity = GitIdentity {
            name: Some("A".into()),
            email: Some("a@example.com".into()),
            signing_key: Some("KEY".into()),
            gpg_sign: Some("TRUE".into()),
        };
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
