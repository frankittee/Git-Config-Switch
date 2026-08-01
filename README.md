<div align="center">

# gcs

**Switch Git identities without leaving your terminal.**

A fast, focused CLI for managing Git profiles per repository.

[![Release](https://img.shields.io/github/v/release/frankittee/Git-Config-Switch?style=flat-square)](https://github.com/frankittee/Git-Config-Switch/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/frankittee/Git-Config-Switch/release.yml?style=flat-square&label=release)](https://github.com/frankittee/Git-Config-Switch/actions/workflows/release.yml)
![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)
[![Rust](https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square)](https://www.rust-lang.org/)

</div>

`gcs` keeps your work, personal, and open-source Git identities separate. Save each identity once, then apply the right name, email, signing key, and SSH host alias to the current repository in one command.

```text
$ gcs use work
Successfully write profiles into .git/config
```

## Why gcs?

- **Repository-first** — changes are scoped to the current repository by default, so one project never leaks its identity into another.
- **Interactive when you want it** — launch the terminal UI with `gcs`, or use explicit subcommands in scripts.
- **Signing-aware** — switches `user.signingkey` and `commit.gpgsign` together.
- **Multiple GitHub accounts** — optionally rewrites SSH remotes through an alias from `~/.ssh/config`.
- **Safe updates** — validates first and rolls back Git configuration if a write fails.

## Install

Install the latest release on Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Switch/main/install.sh | sh
```

The installer verifies the release checksum and places `gcs` in `~/.local/bin`.

```sh
# Choose a different installation directory
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Switch/main/install.sh | \
  GCS_INSTALL_DIR="$HOME/bin" sh

# Install a specific version
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Switch/main/install.sh | \
  GCS_VERSION="0.4.0" sh
```

## Quick start

Create a profile:

```text
$ gcs add work
Git author name: Ada Lovelace
Git author email: ada@company.example
Enable commit signing? [y/N]: y
Signing key: ABC123
SSH host alias (leave blank to skip): github-work
work
```

Apply it inside a Git repository:

```sh
cd my-project
gcs use work
```

Check which saved profile matches the repository:

```sh
gcs info
```

## Terminal UI

Run `gcs` without a subcommand to manage profiles from the interactive terminal interface:

```sh
gcs
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
| `gcs` | Open the terminal UI |
| `gcs add <profile>` | Create a profile |
| `gcs edit <profile>` | Edit a profile |
| `gcs list` | List saved profiles |
| `gcs show <profile>` | Show profile details |
| `gcs use <profile>` | Apply a profile |
| `gcs info` | Show the matching active profile |
| `gcs remove <profile>` | Remove a profile |

All profile fields can also be provided as flags, which makes `gcs` convenient in scripts:

```sh
gcs add personal \
  --name "Ada Lovelace" \
  --email "ada@example.com"

gcs add work \
  --name "Ada Lovelace" \
  --email "ada@company.example" \
  --signing-key "ABC123" \
  --ssh-host github-work

gcs edit work --email "new-address@company.example"
gcs edit work --no-signing
gcs edit work --no-ssh-host
```

Run `gcs <command> --help` for every available option. The terminal UI and prompts require a TTY; in CI, provide both `--name` and `--email` when adding a profile.

## How it works

Profiles are stored in:

```text
$HOME/.config/git-config-switch/config.toml
```

Set `GCS_CONFIG_DIR` to override the containing directory, for example in isolated automation or tests.

By default, `gcs use` updates the current repository's local Git configuration. When run directly from your home directory, it updates the global Git configuration instead.

If a profile has a signing key, `gcs use` sets `user.signingkey` and enables `commit.gpgsign`. Applying a profile without a signing key removes both settings from the selected Git configuration scope.

If a profile has an `ssh_host`, `gcs` first verifies that the literal `Host` alias exists in `~/.ssh/config` or an included configuration file, then rewrites SSH fetch URLs and explicit push URLs for every remote to use that alias. Include paths support paths relative to `~/.ssh`, absolute paths, `~`, nested includes, and glob wildcards. HTTPS and local URLs are left unchanged. Validation or write failures leave both identity and remote configuration unchanged.

<details>
<summary><strong>Release process</strong></summary>

Pushing a `v*` tag builds Linux x86_64/ARM64 musl binaries and native macOS Intel/Apple Silicon binaries. The tag must match the version in `Cargo.toml`.

```sh
# After updating the package version and committing it:
git tag v0.4.0
git push origin v0.4.0
```

Normal branch pushes do not trigger release builds.

</details>

## License

Released under the MIT License.

---

<div align="center">

Built for developers who use more than one Git identity.

</div>
