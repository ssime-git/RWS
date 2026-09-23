//! Durable, private installation state for the macOS RWS client.
use crate::config::Config;
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

/// The private Application Support tree used by a durable RWS installation.
/// `at` exists so tests (and callers embedding RWS) never need to write to a
/// real home directory.
#[derive(Clone, Debug)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn macos(home: &Path) -> Self {
        Self::at(home.join("Library/Application Support/RWS"))
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    /// Stable path used by integrations such as the login LaunchAgent.
    pub fn binary(&self) -> PathBuf {
        self.root.join("bin/rws")
    }

    pub fn require_binary(&self) -> Result<PathBuf, String> {
        let binary = self.binary();
        validate_executable(&binary, "managed RWS binary")?;
        Ok(binary)
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn releases(&self) -> PathBuf {
        self.root.join("releases")
    }

    fn mount_state(&self) -> PathBuf {
        self.root.join("mount-state")
    }
}

#[derive(Clone, Debug)]
pub struct Installation {
    pub release: PathBuf,
    pub rws: PathBuf,
    pub sshfs: PathBuf,
    pub generation: String,
}

/// Stage an already-resolved RWS executable and effective SSHFS executable,
/// then atomically activate the resulting configuration.  Nothing references
/// the new release before the final config rename succeeds.
pub fn install(
    layout: &Layout,
    source_config: &Path,
    rws: &Path,
    sshfs: &Path,
) -> Result<Installation, String> {
    let source = Config::load_existing(source_config)?;
    let source_receipts = source_receipt_directory(source_config, &source);
    validate_executable(rws, "RWS")?;
    validate_executable(sshfs, "SSHFS")?;
    prepare_private_directory(&layout.root)?;
    prepare_private_directory(&layout.bin())?;
    prepare_private_directory(&layout.releases())?;
    prepare_private_directory(&layout.mount_state())?;

    let generation = new_generation();
    let release = layout.releases().join(&generation);
    create_fresh_private_directory(&release)?;
    let staged_rws = layout
        .binary()
        .with_extension(format!("{}.tmp", std::process::id()));
    let staged_sshfs_directory = release.join("sshfs");
    create_fresh_private_directory(&staged_sshfs_directory)?;
    let staged_sshfs = staged_sshfs_directory.join("sshfs");
    copy_executable(rws, &staged_rws)?;
    copy_executable(sshfs, &staged_sshfs)?;
    validate_executable(&staged_rws, "staged RWS")?;
    validate_executable(&staged_sshfs, "staged SSHFS")?;

    let mut activated = source;
    activated.mount.sshfs = Some(path_string(&staged_sshfs)?);
    activated.mount_state_generation = Some(generation.clone());
    let destination_state = state_directory(&layout.config_path(), &generation);
    let state_generation = layout.mount_state().join(&generation);
    create_fresh_private_directory(&state_generation)?;
    create_fresh_private_directory(&destination_state)?;
    migrate_receipts(&source_receipts, &activated, &destination_state)?;

    // Replace the stable integration target before activating the new config.
    // The config replacement below remains the sole activation point.
    fs::rename(&staged_rws, layout.binary()).map_err(|e| {
        format!(
            "activate managed RWS executable {}: {e}",
            layout.binary().display()
        )
    })?;

    // This is deliberately the final write: an unsuccessful install leaves
    // the old config and its release untouched, while an old release is never
    // removed by a later successful installation.
    write_config_atomically(&layout.config_path(), &activated)?;
    Ok(Installation {
        release,
        rws: layout.binary(),
        sshfs: staged_sshfs,
        generation,
    })
}

/// Convenience entry point for the normal executable.  CLI wiring can use it
/// later without reimplementing the effective SSHFS selection rules.
pub fn install_current(layout: &Layout, source_config: &Path) -> Result<Installation, String> {
    let config = Config::load_existing(source_config)?;
    let rws = std::env::current_exe().map_err(|e| format!("find current RWS executable: {e}"))?;
    let sshfs = effective_sshfs(source_config, &config)?;
    // Relaunching the same app must not rotate receipts or accumulate identical
    // SSHFS releases. Only reuse a complete canonical installation.
    if same_config_file(source_config, &layout.config_path())
        && let Some(generation) = config.mount_state_generation.as_ref()
        && sshfs == layout.releases().join(generation).join("sshfs/sshfs")
        && validate_executable(&layout.binary(), "managed RWS binary").is_ok()
        && validate_executable(&sshfs, "managed SSHFS").is_ok()
        && fs::read(&rws).map_err(|e| e.to_string())?
            == fs::read(layout.binary()).map_err(|e| e.to_string())?
    {
        return Ok(Installation {
            release: layout.releases().join(generation),
            rws: layout.binary(),
            sshfs,
            generation: generation.clone(),
        });
    }
    install(layout, source_config, &rws, &sshfs)
}

/// Select SSHFS for one invocation. A durable configuration is authoritative:
/// accepting a conflicting inherited override there could reintroduce a
/// dependency on a deleted checkout at login.
pub fn select_sshfs(config_path: &Path, config: &Config) -> Result<PathBuf, String> {
    let configured = config
        .mount
        .sshfs
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("sshfs"));
    let inherited = std::env::var_os("RWS_SSHFS").map(PathBuf::from);
    if is_canonical_config(config_path) {
        if let Some(inherited) = inherited
            && inherited != configured
        {
            return Err(format!(
                "RWS_SSHFS={} conflicts with the canonical configured SSHFS {}; unset RWS_SSHFS or use the managed configuration",
                inherited.display(),
                configured.display()
            ));
        }
        return Ok(configured);
    }
    Ok(inherited.unwrap_or(configured))
}

fn is_canonical_config(config_path: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let canonical = Layout::macos(Path::new(&home)).config_path();
    // Both paths must exist to prove identity. In particular, resolving the
    // requested path closes `.`/`..` and symlink aliases; treating a missing
    // config as canonical would change the existing missing-config behavior.
    match (fs::canonicalize(config_path), fs::canonicalize(canonical)) {
        (Ok(requested), Ok(canonical)) => same_config_file(&requested, &canonical),
        _ => false,
    }
}

#[cfg(unix)]
fn same_config_file(requested: &Path, canonical: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    if requested == canonical {
        return true;
    }
    match (fs::metadata(requested), fs::metadata(canonical)) {
        (Ok(requested), Ok(canonical)) => {
            requested.dev() == canonical.dev() && requested.ino() == canonical.ino()
        }
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_config_file(requested: &Path, canonical: &Path) -> bool {
    requested == canonical
}

fn effective_sshfs(config_path: &Path, config: &Config) -> Result<PathBuf, String> {
    let configured = select_sshfs(config_path, config)?;
    if configured.is_absolute() {
        return Ok(configured);
    }
    let path = std::env::var_os("PATH").ok_or("PATH is unset; configure an absolute SSHFS path")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(&configured))
        .find(|candidate| validate_executable(candidate, "SSHFS").is_ok())
        .ok_or_else(|| {
            format!(
                "cannot find executable SSHFS program {}",
                configured.display()
            )
        })
}

fn validate_executable(path: &Path, label: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{label} executable must be an absolute path"));
    }
    let metadata = fs::metadata(path)
        .map_err(|e| format!("inspect {label} executable {}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "{label} executable must be a regular file: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!(
            "{label} executable is not executable: {}",
            path.display()
        ));
    }
    Ok(())
}

fn path_string(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("staged SSHFS path is not absolute".into());
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or("staged SSHFS path is not UTF-8".into())
}

fn new_generation() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("release-{}-{nanos}", std::process::id())
}

fn inspect_private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("inspect private directory {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("private directory is unsafe: {}", path.display()));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("protect private directory {}: {e}", path.display()))?;
    }
    Ok(())
}

fn create_fresh_private_directory(path: &Path) -> Result<(), String> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(format!(
                "refusing to reuse existing immutable release: {}",
                path.display()
            ));
        }
        Err(e) => return Err(format!("create private directory {}: {e}", path.display())),
    }
    inspect_private_directory(path)
}

fn prepare_private_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| format!("create private directory {}: {e}", path.display()))?;
    }
    inspect_private_directory(path)
}

fn copy_executable(source: &Path, destination: &Path) -> Result<(), String> {
    fs::copy(source, destination)
        .map_err(|e| format!("stage executable {}: {e}", source.display()))?;
    #[cfg(unix)]
    fs::set_permissions(destination, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("protect staged executable {}: {e}", destination.display()))?;
    Ok(())
}

fn state_directory(config: &Path, generation: &str) -> PathBuf {
    let filename = config
        .file_name()
        .unwrap_or(config.as_os_str())
        .as_encoded_bytes();
    let namespace = filename
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    config
        .parent()
        .unwrap_or(Path::new("."))
        .join("mount-state")
        .join(generation)
        .join(format!("config-{namespace}"))
}

fn source_receipt_directory(config: &Path, source: &Config) -> PathBuf {
    match &source.mount_state_generation {
        Some(generation) => state_directory(config, generation),
        None => {
            let mut directory = config.as_os_str().to_os_string();
            directory.push(".mount-state");
            PathBuf::from(directory)
        }
    }
}

#[derive(Deserialize)]
struct ReceiptBinding {
    host: String,
    remote: String,
    root: PathBuf,
}

fn migrate_receipts(source: &Path, config: &Config, destination: &Path) -> Result<(), String> {
    for workspace in &config.workspaces {
        let receipt = source.join(format!("{}.json", workspace.name));
        let metadata = match fs::symlink_metadata(&receipt) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
            Ok(_) | Err(_) => continue,
        };
        if metadata.len() > 1024 * 1024 {
            continue;
        }
        let bytes = match fs::read(&receipt) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let Ok(binding) = serde_json::from_slice::<ReceiptBinding>(&bytes) else {
            continue;
        };
        if binding.host != workspace.host
            || binding.remote != workspace.remote_root
            || binding.root != workspace.mount_root
        {
            continue;
        }
        write_private_file(
            &destination.join(format!("{}.json", workspace.name)),
            &bytes,
        )?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temporary)
        .map_err(|e| format!("create private file {}: {e}", temporary.display()))?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    if let Err(e) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("write private file {}: {e}", path.display()));
    }
    fs::rename(&temporary, path)
        .map_err(|e| format!("activate private file {}: {e}", path.display()))
}

fn write_config_atomically(path: &Path, config: &Config) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|e| format!("serialize installed config: {e}"))?;
    write_private_file(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, MountOptions},
        workspace::Workspace,
    };
    use std::{fs, os::unix::fs::PermissionsExt};

    fn executable(path: &std::path::Path) {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn config() -> Config {
        Config {
            version: 1,
            workspaces: vec![Workspace {
                name: "demo".into(),
                host: "dev@host".into(),
                remote_root: "/srv/demo".into(),
                mount_root: "/Volumes/RWS-demo".into(),
            }],
            mount: MountOptions::default(),
            mount_state_generation: None,
        }
    }

    #[test]
    fn install_stages_executables_and_activates_config_last() {
        let temp = tempfile::tempdir().unwrap();
        let source_config = temp.path().join("source.json");
        fs::write(&source_config, serde_json::to_vec(&config()).unwrap()).unwrap();
        let rws = temp.path().join("rws");
        let sshfs = temp.path().join("sshfs");
        executable(&rws);
        executable(&sshfs);
        let layout = Layout::at(temp.path().join("Library/Application Support/RWS"));

        let installed = install(&layout, &source_config, &rws, &sshfs).unwrap();
        let active = Config::load_existing(&layout.config_path()).unwrap();

        assert_eq!(
            fs::read(&source_config).unwrap(),
            serde_json::to_vec(&config()).unwrap()
        );
        assert_eq!(active.mount.sshfs.as_deref(), installed.sshfs.to_str());
        assert_eq!(
            active.mount_state_generation.as_deref(),
            Some(installed.generation.as_str())
        );
        assert_eq!(installed.rws, layout.binary());
        assert_eq!(installed.sshfs, installed.release.join("sshfs/sshfs"));
        assert!(installed.rws.is_file());
        assert!(installed.sshfs.is_file());
        assert_eq!(
            fs::metadata(&installed.rws).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(layout.config_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn failed_staging_leaves_prior_config_and_release_intact() {
        let temp = tempfile::tempdir().unwrap();
        let source_config = temp.path().join("source.json");
        fs::write(&source_config, serde_json::to_vec(&config()).unwrap()).unwrap();
        let rws = temp.path().join("rws");
        let sshfs = temp.path().join("sshfs");
        executable(&rws);
        executable(&sshfs);
        let layout = Layout::at(temp.path().join("support"));
        let first = install(&layout, &source_config, &rws, &sshfs).unwrap();
        let old_config = fs::read(layout.config_path()).unwrap();
        let bad = temp.path().join("not-executable");
        fs::write(&bad, "not executable").unwrap();

        assert!(install(&layout, &source_config, &rws, &bad).is_err());
        assert_eq!(fs::read(layout.config_path()).unwrap(), old_config);
        assert!(first.release.is_dir());
    }

    #[test]
    fn install_migrates_only_receipts_bound_to_registered_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let source_config = temp.path().join("source.json");
        fs::write(&source_config, serde_json::to_vec(&config()).unwrap()).unwrap();
        let receipt_dir = temp.path().join("source.json.mount-state");
        fs::create_dir(&receipt_dir).unwrap();
        fs::write(
            receipt_dir.join("demo.json"),
            br#"{"host":"dev@host","remote":"/srv/demo","root":"/Volumes/RWS-demo","identity":{"source":"x","filesystem":"x","id":[1]}}"#,
        ).unwrap();
        fs::write(
            receipt_dir.join("unrelated.json"),
            br#"{"host":"attacker","remote":"/","root":"/Volumes/RWS-demo","identity":{}}"#,
        )
        .unwrap();
        let rws = temp.path().join("rws");
        let sshfs = temp.path().join("sshfs");
        executable(&rws);
        executable(&sshfs);
        let layout = Layout::at(temp.path().join("support"));

        let installed = install(&layout, &source_config, &rws, &sshfs).unwrap();
        let active = Config::load_existing(&layout.config_path()).unwrap();
        let state = layout
            .mount_state()
            .join(&installed.generation)
            .join("config-636f6e6669672e6a736f6e");

        assert!(state.join("demo.json").is_file());
        assert!(!state.join("unrelated.json").exists());
        assert_eq!(
            fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(layout.mount_state().join(&installed.generation))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(state.join("demo.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(crate::lifecycle::verified(
            &layout.config_path(),
            &active,
            &active.workspaces[0],
            &crate::lifecycle::MountIdentity {
                source: "x".into(),
                filesystem: "x".into(),
                id: vec![1],
            }
        ));
    }
}
