use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::git::GitIdentity;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Profile {
    pub name: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Profiles {
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

impl Profiles {
    pub fn matching_name(&self, identity: &GitIdentity) -> Option<&str> {
        self.profiles
            .iter()
            .find(|(_, profile)| identity.matches(profile))
            .map(|(name, _)| name.as_str())
    }
}

#[derive(Debug)]
pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn from_environment() -> Result<Self> {
        let directory = match env::var_os("GCS_CONFIG_DIR") {
            Some(path) => PathBuf::from(path),
            None => {
                let base =
                    BaseDirs::new().context("could not determine the user home directory")?;
                return Ok(Self::new(default_path(base.home_dir())));
            }
        };
        Ok(Self::new(directory.join("config.toml")))
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Profiles> {
        if !self.path.exists() {
            return Ok(Profiles::default());
        }
        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("could not read {}", self.path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("invalid profile file {}", self.path.display()))
    }

    pub fn add(&self, name: String, profile: Profile) -> Result<()> {
        validate_name(&name)?;
        validate_profile(&profile)?;
        let mut profiles = self.load()?;
        ensure_available(&profiles, &name)?;
        profiles.profiles.insert(name, profile);
        self.save(&profiles)
    }

    pub fn ensure_available(&self, name: &str) -> Result<()> {
        validate_name(name)?;
        ensure_available(&self.load()?, name)
    }

    pub fn update(&self, name: &str, profile: Profile) -> Result<()> {
        validate_profile(&profile)?;
        let mut profiles = self.load()?;
        let stored = profiles
            .profiles
            .get_mut(name)
            .with_context(|| format!("profile '{name}' does not exist"))?;
        *stored = profile;
        self.save(&profiles)
    }

    pub fn list(&self) -> Result<Vec<String>> {
        Ok(self.load()?.profiles.into_keys().collect())
    }

    pub fn get(&self, name: &str) -> Result<Profile> {
        self.load()?
            .profiles
            .remove(name)
            .with_context(|| format!("profile '{name}' does not exist"))
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let mut profiles = self.load()?;
        if profiles.profiles.remove(name).is_none() {
            bail!("profile '{name}' does not exist");
        }
        self.save(&profiles)
    }

    fn save(&self, profiles: &Profiles) -> Result<()> {
        let path = if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            fs::canonicalize(&self.path).with_context(|| {
                format!(
                    "could not resolve profile file symlink {}",
                    self.path.display()
                )
            })?
        } else {
            self.path.clone()
        };
        let parent = path
            .parent()
            .context("profile path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;

        let contents = toml::to_string_pretty(profiles).context("could not serialize profiles")?;
        let mut temporary =
            NamedTempFile::new_in(parent).context("could not create temporary profile file")?;
        temporary
            .write_all(contents.as_bytes())
            .context("could not write temporary profile file")?;
        temporary
            .as_file()
            .sync_all()
            .context("could not sync temporary profile file")?;
        temporary.persist(&path).map_err(|error| {
            anyhow::anyhow!(
                "could not replace profile file {}: {}",
                path.display(),
                error.error
            )
        })?;
        sync_directory(parent)?;
        Ok(())
    }
}

fn default_path(home: &Path) -> PathBuf {
    home.join(".config")
        .join("git-config-switch")
        .join("config.toml")
}

fn ensure_available(profiles: &Profiles, name: &str) -> Result<()> {
    if profiles.profiles.contains_key(name) {
        bail!("profile '{name}' already exists; use 'gcs edit {name}' to change it");
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("profile name must not be empty");
    }
    if name.chars().any(char::is_whitespace) {
        bail!("profile name must not contain whitespace");
    }
    Ok(())
}

fn validate_profile(profile: &Profile) -> Result<()> {
    if profile.name.trim().is_empty() {
        bail!("Git author name must not be empty");
    }
    if profile.email.trim().is_empty() {
        bail!("Git author email must not be empty");
    }
    if profile
        .signing_key
        .as_ref()
        .is_some_and(|key| key.trim().is_empty())
    {
        bail!("signing key must not be empty");
    }
    if profile.ssh_host.as_ref().is_some_and(|host| {
        host.trim().is_empty()
            || host.chars().any(char::is_whitespace)
            || host.contains(['*', '?', '!'])
    }) {
        bail!("SSH host alias must be literal, non-empty, and contain no whitespace");
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("could not sync {}", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn profile(name: &str, email: &str) -> Profile {
        Profile {
            name: name.into(),
            email: email.into(),
            signing_key: None,
            ssh_host: None,
        }
    }

    #[test]
    fn missing_file_is_empty() {
        let directory = tempdir().unwrap();
        let store = ProfileStore::new(directory.path().join("config.toml"));
        assert!(store.load().unwrap().profiles.is_empty());
    }

    #[test]
    fn default_path_is_under_home_config_directory() {
        assert_eq!(
            default_path(Path::new("/home/example")),
            PathBuf::from("/home/example/.config/git-config-switch/config.toml")
        );
    }

    #[test]
    fn add_load_and_list_are_stable() {
        let directory = tempdir().unwrap();
        let store = ProfileStore::new(directory.path().join("config.toml"));
        store
            .add("zeta".into(), profile("Z", "z@example.com"))
            .unwrap();
        store
            .add("alpha".into(), profile("A", "a@example.com"))
            .unwrap();

        assert_eq!(store.list().unwrap(), vec!["alpha", "zeta"]);
        assert_eq!(store.get("alpha").unwrap(), profile("A", "a@example.com"));
    }

    #[test]
    fn duplicate_suggests_edit() {
        let directory = tempdir().unwrap();
        let store = ProfileStore::new(directory.path().join("config.toml"));
        store
            .add("work".into(), profile("Old", "old@example.com"))
            .unwrap();
        let error = store
            .add("work".into(), profile("New", "new@example.com"))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("use 'gcs edit work' to change it")
        );
        assert_eq!(
            store.get("work").unwrap(),
            profile("Old", "old@example.com")
        );
    }

    #[test]
    fn update_requires_existing_profile() {
        let directory = tempdir().unwrap();
        let store = ProfileStore::new(directory.path().join("config.toml"));
        assert!(
            store
                .update("missing", profile("New", "new@example.com"))
                .is_err()
        );

        store
            .add("work".into(), profile("Old", "old@example.com"))
            .unwrap();
        store
            .update("work", profile("New", "new@example.com"))
            .unwrap();
        assert_eq!(store.get("work").unwrap().name, "New");
    }

    #[test]
    fn malformed_file_is_an_error() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "not valid = [").unwrap();
        let store = ProfileStore::new(path);
        assert!(store.load().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn save_updates_symlink_target_without_replacing_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let target_directory = tempdir().unwrap();
        let target = target_directory.path().join("profiles.toml");
        fs::write(&target, "[profiles]\n").unwrap();

        let path = directory.path().join("config.toml");
        symlink(&target, &path).unwrap();
        let store = ProfileStore::new(path.clone());
        store
            .add("work".into(), profile("Work User", "work@example.com"))
            .unwrap();

        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(target)
                .unwrap()
                .contains("work@example.com")
        );
    }
}
