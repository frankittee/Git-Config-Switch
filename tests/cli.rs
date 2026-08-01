use std::{
    fs,
    path::Path,
    process::{Command, Output},
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

    fn gcs(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_gcs"))
            .args(args)
            .env("HOME", self.home.path())
            .env("GCS_CONFIG_DIR", self.config.path())
            .env("GIT_CONFIG_GLOBAL", self.config.path().join("gitconfig"))
            .current_dir(self.repository.path())
            .output()
            .unwrap()
    }

    fn gcs_from_home(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_gcs"))
            .args(args)
            .env("HOME", self.repository.path())
            .env("GCS_CONFIG_DIR", self.config.path())
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
    assert_success(&context.gcs(&["add", profile, "--name", name, "--email", email]));
}

#[test]
fn bare_command_requires_a_terminal() {
    let context = TestContext::new(true);
    let output = context.gcs(&[]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("TUI requires an interactive terminal"));
}

#[test]
fn profile_management_round_trip() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");
    add(&context, "alpha", "Alpha User", "alpha@example.com");

    assert_eq!(stdout(&context.gcs(&["list"])), "alpha\nwork\n");
    let shown = stdout(&context.gcs(&["show", "work"]));
    assert!(shown.contains("name=Work User\n"));
    assert!(shown.contains("email=work@example.com\n"));

    let duplicate = context.gcs(&[
        "add",
        "work",
        "--name",
        "New User",
        "--email",
        "new@example.com",
    ]);
    assert!(!duplicate.status.success());
    assert!(stderr(&duplicate).contains("gcs edit work"));
    assert!(stdout(&context.gcs(&["show", "work"])).contains("name=Work User"));

    assert_success(&context.gcs(&["remove", "work"]));
    assert!(!context.gcs(&["show", "work"]).status.success());
}

#[test]
fn edits_profile_fields_non_interactively() {
    let context = TestContext::new(true);
    assert_success(&context.gcs(&[
        "add",
        "work",
        "--name",
        "Old User",
        "--email",
        "old@example.com",
        "--signing-key",
        "OLDKEY",
    ]));

    assert_success(&context.gcs(&["edit", "work", "--email", "new@example.com"]));
    let shown = stdout(&context.gcs(&["show", "work"]));
    assert!(shown.contains("name=Old User\n"));
    assert!(shown.contains("email=new@example.com\n"));
    assert!(shown.contains("signing_key=OLDKEY\n"));

    assert_success(&context.gcs(&["edit", "work", "--name", "New User", "--no-signing"]));
    let shown = stdout(&context.gcs(&["show", "work"]));
    assert!(shown.contains("name=New User\n"));
    assert!(!shown.contains("signing_key="));
}

#[test]
fn edits_and_clears_ssh_host() {
    let context = TestContext::new(true);
    assert_success(&context.gcs(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "github-work",
    ]));
    assert!(stdout(&context.gcs(&["show", "work"])).contains("ssh_host=github-work\n"));

    assert_success(&context.gcs(&["edit", "work", "--no-ssh-host"]));
    assert!(!stdout(&context.gcs(&["show", "work"])).contains("ssh_host="));
}

#[test]
fn edit_validates_target_and_non_terminal_input() {
    let context = TestContext::new(true);
    let missing = context.gcs(&["edit", "missing", "--name", "New User"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("does not exist"));

    add(&context, "work", "Work User", "work@example.com");
    let interactive = context.gcs(&["edit", "work"]);
    assert!(!interactive.status.success());
    assert!(stderr(&interactive).contains("interactive input requires a terminal"));

    let conflicting = context.gcs(&["edit", "work", "--signing-key", "KEY", "--no-signing"]);
    assert!(!conflicting.status.success());
    assert!(stderr(&conflicting).contains("cannot be used with"));
}

#[test]
fn applies_profiles_and_reports_current() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");

    let use_output = context.gcs(&["use", "work"]);
    assert_success(&use_output);
    assert_eq!(
        stdout(&use_output),
        "Successfully write profiles into .git/config\n"
    );
    assert_eq!(context.git_value("user.name").as_deref(), Some("Work User"));
    assert_eq!(
        context.git_value("user.email").as_deref(),
        Some("work@example.com")
    );
    assert_eq!(stdout(&context.gcs(&["info"])), "work\n");
}

#[test]
fn use_from_home_applies_profile_globally() {
    let context = TestContext::new(false);
    add(&context, "work", "Work User", "work@example.com");

    let use_output = context.gcs_from_home(&["use", "work"]);
    assert_success(&use_output);
    assert_eq!(
        stdout(&use_output),
        format!(
            "Successfully write profiles into {}\n",
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
fn signing_is_enabled_and_then_cleared() {
    let context = TestContext::new(true);
    assert_success(&context.gcs(&[
        "add",
        "signed",
        "--name",
        "Signed User",
        "--email",
        "signed@example.com",
        "--signing-key",
        "ABC123",
    ]));
    add(&context, "plain", "Plain User", "plain@example.com");

    assert_success(&context.gcs(&["use", "signed"]));
    assert_eq!(
        context.git_value("user.signingkey").as_deref(),
        Some("ABC123")
    );
    assert_eq!(context.git_value("commit.gpgsign").as_deref(), Some("true"));

    assert_success(&context.gcs(&["use", "plain"]));
    assert_eq!(context.git_value("user.signingkey"), None);
    assert_eq!(context.git_value("commit.gpgsign"), None);
}

#[test]
fn use_rewrites_all_ssh_remote_urls() {
    let context = TestContext::new(true);
    context.ssh_config("Host github-work\n  HostName github.com\n");
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
    assert_success(&context.gcs(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "github-work",
    ]));

    assert_success(&context.gcs(&["use", "work"]));

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
    assert_success(&context.gcs(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "github-work",
    ]));

    assert_success(&context.gcs(&["use", "work"]));
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
    assert_success(&context.gcs(&[
        "add",
        "work",
        "--name",
        "Work User",
        "--email",
        "work@example.com",
        "--ssh-host",
        "work-main",
    ]));

    let output = context.gcs(&["use", "work"]);
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
    let unmanaged = context.gcs(&["info"]);
    assert_success(&unmanaged);
    assert_eq!(stdout(&unmanaged), "unmanaged\n");
    assert!(!context.gcs(&["use", "missing"]).status.success());

    let outside = TestContext::new(false);
    add(&outside, "work", "Work User", "work@example.com");
    let use_output = outside.gcs(&["use", "work"]);
    assert!(!use_output.status.success());
    assert!(stderr(&use_output).contains("not inside a Git repository"));
    let info_output = outside.gcs(&["info"]);
    assert!(!info_output.status.success());
    assert!(stderr(&info_output).contains("not inside a Git repository"));
}

#[test]
fn required_arguments_are_enforced() {
    let context = TestContext::new(true);
    let output = context.gcs(&["add", "incomplete", "--name", "Name"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("interactive input requires a terminal"));
    assert!(stderr(&output).contains("--name and --email"));
}

#[test]
fn duplicate_is_rejected_before_interactive_input() {
    let context = TestContext::new(true);
    add(&context, "work", "Work User", "work@example.com");

    let output = context.gcs(&["add", "work"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("gcs edit work"));
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
