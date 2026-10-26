use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const BEGIN: &str = "<!-- BEGIN RWS REMOTE EXECUTION -->";
const END: &str = "<!-- END RWS REMOTE EXECUTION -->";

/// Compare the complete managed block, including correctly quoted paths and
/// current instructions. Unrelated personal rules do not affect this check.
pub fn references_current(config: &Path, binary: &Path, target: &Path) -> Result<bool, String> {
    let expected = managed_rules(config, binary)?;
    let contents = match fs::read_to_string(target) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("read rules: {e}")),
    };
    let begins: Vec<_> = contents.match_indices(BEGIN).map(|(i, _)| i).collect();
    let ends: Vec<_> = contents.match_indices(END).map(|(i, _)| i).collect();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(false),
        ([start], [end]) if start < end => Ok(contents[*start..end + END.len()] == expected),
        _ => Err("malformed or duplicate RWS remote-execution markers".into()),
    }
}

/// Find the personal rules file Delta actually selects, or its default new file.
pub fn default_delta_rules_path() -> Result<PathBuf, String> {
    if let Some(directory) = std::env::var_os("DELTA_CONFIG_DIR") {
        if directory.is_empty() {
            return Err("DELTA_CONFIG_DIR is empty".into());
        }
        return choose_path(&PathBuf::from(directory), None);
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    choose_path(
        &home.join(".config/delta"),
        Some(&home.join("Library/Application Support/delta")),
    )
}

/// Coding agents whose personal (global) rule files RWS knows how to manage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Pi,
    Cline,
    Devin,
    Delta,
}

impl Agent {
    pub const ALL: [Agent; 6] = [
        Agent::Claude,
        Agent::Codex,
        Agent::Pi,
        Agent::Cline,
        Agent::Devin,
        Agent::Delta,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Cline => "cline",
            Agent::Devin => "devin",
            Agent::Delta => "delta",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Pi => "pi",
            Agent::Cline => "Cline",
            Agent::Devin => "Devin CLI",
            Agent::Delta => "Delta",
        }
    }

    /// Directories whose existence shows the agent is set up on this machine.
    pub fn config_dirs(self) -> Result<Vec<PathBuf>, String> {
        let home = home()?;
        Ok(match self {
            Agent::Claude => vec![env_dir("CLAUDE_CONFIG_DIR")?.unwrap_or(home.join(".claude"))],
            Agent::Codex => vec![env_dir("CODEX_HOME")?.unwrap_or(home.join(".codex"))],
            Agent::Pi => vec![env_dir("PI_CODING_AGENT_DIR")?.unwrap_or(home.join(".pi/agent"))],
            Agent::Cline => vec![
                env_dir("CLINE_DIR")?.unwrap_or(home.join(".cline")),
                home.join("Documents/Cline"),
            ],
            Agent::Devin => vec![home.join(".config/devin")],
            Agent::Delta => match env_dir("DELTA_CONFIG_DIR")? {
                Some(directory) => vec![directory],
                None => vec![
                    home.join(".config/delta"),
                    home.join("Library/Application Support/delta"),
                ],
            },
        })
    }

    pub fn detected(self) -> Result<bool, String> {
        Ok(self.config_dirs()?.iter().any(|d| d.is_dir()))
    }

    /// The personal rules file this agent actually loads, or its default new file.
    pub fn default_rules_path(self) -> Result<PathBuf, String> {
        let directory = || -> Result<PathBuf, String> { Ok(self.config_dirs()?.remove(0)) };
        match self {
            Agent::Claude => Ok(directory()?.join("CLAUDE.md")),
            // Codex reads AGENTS.override.md instead of AGENTS.md when present.
            Agent::Codex => {
                first_existing(&directory()?, &["AGENTS.override.md"], "AGENTS.md", true)
            }
            // pi loads the first existing file of this list from its agent directory.
            Agent::Pi => first_existing(
                &directory()?,
                &[
                    "AGENTS.override.md",
                    "AGENTS.md",
                    "AGENTS.MD",
                    "CLAUDE.md",
                    "CLAUDE.MD",
                ],
                "AGENTS.md",
                false,
            ),
            // A dedicated file in Cline's global rules directory.
            Agent::Cline => Ok(home()?.join("Documents/Cline/Rules/rws-remote-execution.md")),
            Agent::Devin => {
                first_existing(&directory()?, &["AGENTS.md", "AGENT.md"], "AGENTS.md", true)
            }
            Agent::Delta => default_delta_rules_path(),
        }
    }
}

impl std::str::FromStr for Agent {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        Agent::ALL
            .into_iter()
            .find(|agent| agent.name() == value)
            .ok_or_else(|| {
                let names: Vec<_> = Agent::ALL.iter().map(|a| a.name()).collect();
                format!(
                    "unknown agent {value:?}; known agents: {}",
                    names.join(", ")
                )
            })
    }
}

fn home() -> Result<PathBuf, String> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").ok_or("HOME is not set")?,
    ))
}

fn env_dir(name: &str) -> Result<Option<PathBuf>, String> {
    match std::env::var_os(name) {
        Some(value) if value.is_empty() => Err(format!("{name} is empty")),
        Some(value) => Ok(Some(PathBuf::from(value))),
        None => Ok(None),
    }
}

/// Pick the first candidate file present (non-empty when `nonempty`), else `default`.
fn first_existing(
    directory: &Path,
    candidates: &[&str],
    default: &str,
    nonempty: bool,
) -> Result<PathBuf, String> {
    for name in candidates {
        let candidate = directory.join(name);
        match fs::read_to_string(&candidate) {
            Ok(contents) if !nonempty || !contents.trim().is_empty() => return Ok(candidate),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) if error.kind() == std::io::ErrorKind::IsADirectory => (),
            Err(error) => return Err(format!("read {}: {error}", candidate.display())),
        }
    }
    Ok(directory.join(default))
}

fn choose_path(primary: &Path, fallback: Option<&Path>) -> Result<PathBuf, String> {
    for directory in std::iter::once(primary).chain(fallback) {
        for name in ["AGENT.md", "AGENTS.md"] {
            let candidate = directory.join(name);
            match fs::read_to_string(&candidate) {
                Ok(contents) if !contents.trim().is_empty() => return Ok(candidate),
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(format!("read {}: {error}", candidate.display())),
            }
        }
    }
    Ok(primary.join("AGENT.md"))
}

fn quoted_absolute(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err(format!("path must be absolute: {}", path.display()));
    }
    let value = path.to_str().ok_or("path must be UTF-8")?;
    if value.contains(['\n', '\r', '\0']) {
        return Err("path contains a control character".into());
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn managed_rules(config: &Path, binary: &Path) -> Result<String, String> {
    let command = format!(
        "{} --config {}",
        quoted_absolute(binary)?,
        quoted_absolute(config)?
    );
    Ok(format!(
        r#"{BEGIN}
## RWS execution routing — all registered projects

These rules apply to any coding agent that reads them. Before the first terminal command in every task, and whenever the terminal checkout or project changes, discover its actual absolute filesystem path locally:

```sh
checkout=$(pwd -P)
```

If this probe fails or the conversation has no checkout, report that exact condition and ask the user to attach an existing checkout; do not infer a machine or use a previous path. Use `$checkout` in the context check:

```sh
{command} context --cwd "$checkout"
```

Read this command's JSON `mode`, `host`, `choice`, and `choice_expires_at`. This context check runs locally; it does not forward a workload. A `worktree://` URI is not a filesystem path. Never infer the machine or the routing from a path, a project name, a previous task, or an earlier checkout. If context fails, is unknown, or the mounted path is inaccessible, stop and report the error. Do not proceed locally as a fallback.

### `mode: local`

Continue the ordinary local workflow and repeat the context check on checkout or project changes.

### `mode: remote` and `choice: null`

The checkout is inside a registered RWS mount and no current execution choice exists. STOP before any shell workload and ask the user exactly one question: should commands run on this Mac, or on the VM named by the JSON `host`? Name that host in the question. Wait for the answer, then record it with the matching command, re-run the context check, and follow its section:

```sh
{command} choice set --cwd "$checkout" --mode local
{command} choice set --cwd "$checkout" --mode remote
```

The choice covers the whole workspace for 24 hours (`choice_expires_at`, Unix seconds). Do not ask again while it is current; once context reports `choice: null` again, ask again. The user can switch at any time by saying so: then run `choice set` with the other mode, re-run the context check, and follow the new section.

### `mode: remote` and `choice: remote`

Route EVERY agent shell workload through:

```sh
{command} exec --cwd "$checkout" --git-context -- sh -c 'entire remote script'
```

Keep the complete script inside the quoted remote payload, including shell expansion, pipes, conditionals, command substitutions, and redirects; none may execute in the Mac shell. Correctly shell-quote any embedded single quotes. This applies to Git, file searches, package installation, interpreters, build tools, tests, and scripts. Native file read/edit tools may access the mounted files directly.

For the first remote workload, run `uname -s; hostname; pwd` through this wrapper and report the resulting OS, host, and directory as evidence. Inspect available Linux interpreters and tools remotely before using them. Do not reuse Mac `/Users/...` executable paths or Mac environment assumptions. Preserve any existing macOS virtual environment; create a separate Linux environment such as `.venv-rws` when needed.

`--git-context` supplies Git metadata only for the current repository. Do not change into another repository or use `git -C` to target another repository within the same invocation. Resolve that repository's actual absolute checkout with a new context check and a new wrapper invocation. Network Git remotes may be used. Mac-local Git remote URL prefixes under the same registered mount are translated through Git `url.insteadOf`; paths under other mounts or outside registered mounts are not automatically mapped.

After any routing, mount, or SSH failure, stop and report it. Never retry the workload locally.

### `mode: remote` and `choice: local`

The user chose to run commands on this Mac. Run them locally in `$checkout` without the wrapper. The files are served from the VM over the RWS mount: file access is slower than a local disk, and anything you create (build output, virtual environments, caches) is written to the VM's workspace. Use Mac tools and keep any macOS environment separate from a Linux one such as `.venv-rws`. If the mounted path becomes inaccessible, stop and report it.

These personal rules guide agent-issued commands across all registered RWS projects. They do not move an agent application's own internal processes (such as automatic preparation or Git operations) to Linux and are not a security boundary.
{END}"#
    ))
}

fn merge(original: &str, block: &str) -> Result<String, String> {
    let begins: Vec<_> = original.match_indices(BEGIN).map(|(i, _)| i).collect();
    let ends: Vec<_> = original.match_indices(END).map(|(i, _)| i).collect();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(format!(
            "{original}{}{block}\n",
            if original.is_empty() || original.ends_with('\n') {
                ""
            } else {
                "\n"
            }
        )),
        ([start], [end]) if start < end => Ok(format!(
            "{}{block}{}",
            &original[..*start],
            &original[end + END.len()..]
        )),
        _ => {
            Err("malformed or duplicate RWS remote-execution markers; rules left unchanged".into())
        }
    }
}

fn exclusive_file(directory: &Path, prefix: &str, contents: &[u8]) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    for attempt in 0..100 {
        let path = directory.join(format!(
            "{prefix}-{timestamp}-{}-{attempt}.md",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&path);
                    return Err(format!("write {}: {error}", path.display()));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("create {}: {error}", path.display())),
        }
    }
    Err("could not reserve a unique rules file".into())
}

/// Whether the target already carries an RWS-managed rules block.
pub fn managed_block_present(target: &Path) -> Result<bool, String> {
    match fs::read_to_string(target) {
        Ok(contents) => Ok(contents.contains(BEGIN)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("read rules: {error}")),
    }
}

/// Install Delta's rules (kept for the original `delta-rules` entry point).
pub fn install(config: &Path, binary: &Path, target: &Path) -> Result<(), String> {
    install_as(config, binary, target, Agent::Delta.name())
}

/// Preserve custom rules, back up changed existing content, and atomically install private rules.
/// `label` names the backup so files from different agents stay distinguishable.
pub fn install_as(config: &Path, binary: &Path, target: &Path, label: &str) -> Result<(), String> {
    let block = managed_rules(config, binary)?;
    quoted_absolute(target)?;
    let existing = match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("rules target must not be a symlink".into());
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err("rules target must be a regular file".into());
        }
        Ok(_) => Some(fs::read_to_string(target).map_err(|e| format!("read rules: {e}"))?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("inspect rules: {error}")),
    };
    let updated = merge(existing.as_deref().unwrap_or(""), &block)?;
    if existing.as_deref() == Some(updated.as_str()) {
        return Ok(());
    }
    if let Some(original) = &existing {
        let backups = config
            .parent()
            .ok_or("config must have a parent directory")?
            .join("agent-rule-backups");
        fs::create_dir_all(&backups).map_err(|e| format!("create rules backup directory: {e}"))?;
        exclusive_file(&backups, &format!("{label}-rules"), original.as_bytes())?;
    }
    let parent = target
        .parent()
        .ok_or("rules target must have a parent directory")?;
    fs::create_dir_all(parent).map_err(|e| format!("create rules directory: {e}"))?;
    let temporary = exclusive_file(parent, ".rws-rules", updated.as_bytes())?;
    if let Err(error) = fs::rename(&temporary, target) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("replace rules: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn current_rules_require_the_exact_managed_block() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("AGENT.md");
        let config = temp.path().join("config's.json");
        let binary = temp.path().join("rws's");
        assert!(!references_current(&config, &binary, &target).unwrap());
        install(&config, &binary, &target).unwrap();
        assert!(references_current(&config, &binary, &target).unwrap());
        let current = fs::read_to_string(&target).unwrap();
        fs::write(&target, current.replace("Route EVERY", "Route SOME")).unwrap();
        assert!(!references_current(&config, &binary, &target).unwrap());
        fs::write(&target, format!("{current}\n{BEGIN}")).unwrap();
        assert!(references_current(&config, &binary, &target).is_err());
    }
    struct Fixture(tempfile::TempDir);
    impl Fixture {
        fn new() -> Self {
            Self(tempfile::tempdir().unwrap())
        }
        fn install(&self, target: &Path) -> Result<(), String> {
            install(
                &self.0.path().join("rws config's.toml"),
                &self.0.path().join("rws binary's"),
                target,
            )
        }
    }
    #[test]
    fn preserves_custom_rules_and_replaces_idempotently_with_private_backup() {
        let f = Fixture::new();
        let target = f.0.path().join("delta/AGENTS.md");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "Custom rules.\n").unwrap();
        f.install(&target).unwrap();
        let first = fs::read_to_string(&target).unwrap();
        assert!(first.starts_with("Custom rules.\n"));
        assert_eq!(first.matches(BEGIN).count(), 1);
        assert!(first.contains("rws binary'\\''s' --config '"));
        assert!(first.contains("pwd -P"));
        assert!(!first.contains("Do not ask the user to identify the machine"));
        assert!(first.contains("ask the user exactly one question"));
        assert!(first.contains("choice set --cwd \"$checkout\" --mode remote"));
        assert!(first.contains("24 hours"));
        assert!(!first.contains("Delta"));
        assert!(first.contains("context --cwd"));
        assert!(first.contains("--git-context -- sh -c"));
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let backups: Vec<_> = fs::read_dir(f.0.path().join("agent-rule-backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "Custom rules.\n");
        f.install(&target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), first);
        assert_eq!(
            fs::read_dir(f.0.path().join("agent-rule-backups"))
                .unwrap()
                .count(),
            1
        );
        fs::write(&target, format!("before\n{BEGIN}\nold\n{END}\nafter\n")).unwrap();
        f.install(&target).unwrap();
        let result = fs::read_to_string(target).unwrap();
        assert!(result.starts_with("before\n"));
        assert!(result.ends_with("\nafter\n"));
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "Custom rules.\n");
        assert_eq!(
            fs::read_dir(f.0.path().join("agent-rule-backups"))
                .unwrap()
                .count(),
            2
        );
    }
    #[test]
    fn malformed_markers_and_symlinks_are_not_modified() {
        let f = Fixture::new();
        let target = f.0.path().join("AGENT.md");
        for original in [
            BEGIN.to_string(),
            END.to_string(),
            format!("{END}\n{BEGIN}"),
            format!("{BEGIN}\n{BEGIN}\n{END}"),
        ] {
            fs::write(&target, &original).unwrap();
            assert!(f.install(&target).is_err());
            assert_eq!(fs::read_to_string(&target).unwrap(), original);
        }
        fs::remove_file(&target).unwrap();
        let real = f.0.path().join("real");
        fs::write(&real, "safe").unwrap();
        symlink(&real, &target).unwrap();
        assert!(f.install(&target).is_err());
        assert_eq!(fs::read_to_string(real).unwrap(), "safe");
        assert!(!f.0.path().join("agent-rule-backups").exists());
    }
    #[test]
    fn selects_first_nonempty_personal_file_then_fallback() {
        let f = Fixture::new();
        let primary = f.0.path().join("primary");
        let fallback = f.0.path().join("fallback");
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&fallback).unwrap();
        assert_eq!(
            choose_path(&primary, Some(&fallback)).unwrap(),
            primary.join("AGENT.md")
        );
        fs::write(fallback.join("AGENTS.md"), "fallback").unwrap();
        assert_eq!(
            choose_path(&primary, Some(&fallback)).unwrap(),
            fallback.join("AGENTS.md")
        );
        fs::write(primary.join("AGENT.md"), " \n").unwrap();
        fs::write(primary.join("AGENTS.md"), "plural").unwrap();
        assert_eq!(
            choose_path(&primary, Some(&fallback)).unwrap(),
            primary.join("AGENTS.md")
        );
        fs::write(primary.join("AGENT.md"), "singular").unwrap();
        assert_eq!(
            choose_path(&primary, Some(&fallback)).unwrap(),
            primary.join("AGENT.md")
        );
        let override_dir = f.0.path().join("override");
        assert_eq!(
            choose_path(&override_dir, None).unwrap(),
            override_dir.join("AGENT.md")
        );
    }
}
