//! Generate the zsh integration that switches into `rws shell` on entering a mount.
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Marker comment identifying the .zshrc line managed by `rws hook install`.
pub const MARKER: &str = "# RWS auto-shell hook";

/// A single active managed hook must match the expected command. Indentation
/// and trailing personal comments are preserved and do not indicate drift.
pub fn references_current(zshrc: &Path, rws: &Path, config: Option<&Path>) -> Result<bool, String> {
    let expected = install_line(rws, config)?;
    let bytes = match fs::read(zshrc) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("read {}: {e}", zshrc.display())),
    };
    let mut active_count = 0;
    let mut matches = false;
    for bytes in bytes.split_inclusive(|b| *b == b'\n') {
        if let Ok(line) = std::str::from_utf8(bytes)
            && let Some((start, end)) = managed_range(line)
        {
            active_count += 1;
            matches = line[start..end] == expected;
        }
    }
    Ok(active_count == 1 && matches)
}

fn managed_range(line: &str) -> Option<(usize, usize)> {
    let active = line.trim_start_matches([' ', '\t']);
    if !active.starts_with("eval \"$(") {
        return None;
    }
    let marker = active.find(MARKER)?;
    let start = line.len() - active.len();
    Some((start, start + marker + MARKER.len()))
}

/// Refresh active managed hooks only. Return whether any bytes changed.
pub fn refresh_existing(zshrc: &Path, rws: &Path, config: Option<&Path>) -> Result<bool, String> {
    update(zshrc, &install_line(rws, config)?, false)
}

fn quote(label: &str, path: &Path) -> Result<String, String> {
    let raw = path.to_str().ok_or(format!("{label} path must be UTF-8"))?;
    if raw.contains('\n') || raw.contains('\r') || raw.contains('\0') {
        return Err(format!("{label} path must not contain newlines or NUL"));
    }
    Ok(format!("'{}'", raw.replace('\'', r"'\''")))
}

/// Return a zsh snippet for `eval "$(rws hook zsh)"`. The snippet embeds the
/// given rws executable path and never falls back to a PATH lookup. A config
/// path is baked in when given; `RWS_CONFIG` in the environment overrides it.
pub fn zsh_snippet(rws: &Path, config: Option<&Path>) -> Result<String, String> {
    let quoted = quote("rws executable", rws)?;
    let config_selection = match config {
        Some(config) => format!(
            "if [[ -n ${{RWS_CONFIG-}} ]]; then\n    rws_cmd+=(--config \"$RWS_CONFIG\")\n  else\n    rws_cmd+=(--config {})\n  fi",
            quote("configuration", config)?
        ),
        None => "[[ -n ${RWS_CONFIG-} ]] && rws_cmd+=(--config \"$RWS_CONFIG\")".into(),
    };
    Ok(format!(
        r#"# RWS: switch into the workspace's remote shell when entering its mount.
_rws_auto_shell() {{
  [[ -n ${{RWS_NO_AUTO_SHELL-}} ]] && return 0
  local prefix=${{RWS_AUTO_PREFIX:-/Volumes}}
  if [[ $PWD != $prefix/* ]]; then
    unset _RWS_AUTO_SUPPRESS
    return 0
  fi
  local rest=${{PWD#$prefix/}}
  local volume=$prefix/${{rest%%/*}}
  if [[ -n ${{_RWS_AUTO_SUPPRESS-}} && ( $PWD == $_RWS_AUTO_SUPPRESS || $PWD == $_RWS_AUTO_SUPPRESS/* ) ]]; then
    return 0
  fi
  unset _RWS_AUTO_SUPPRESS
  local -a rws_cmd
  rws_cmd=({rws})
  {config_selection}
  local context
  if ! context=$("${{rws_cmd[@]}}" context --cwd "$PWD" 2>&1); then
    print -u2 "rws auto-shell: $context"
    _RWS_AUTO_SUPPRESS=$volume
    return 0
  fi
  [[ $context == *'"mode":"remote"'* ]] || return 0
  local ws=${{context#*\"workspace\":\"}}
  ws=${{ws%%\"*}}
  local host=${{context#*\"host\":\"}}
  host=${{host%%\"*}}
  typeset -gA _RWS_AUTO_CHOICE
  local choice=${{RWS_AUTO_MODE-}}
  [[ -z $choice ]] && choice=${{_RWS_AUTO_CHOICE[$ws]-}}
  if [[ -z $choice ]]; then
    # No controlling terminal (script, IDE subprocess): never prompt or switch.
    if ! {{ : < /dev/tty; }} 2>/dev/null; then
      return 0
    fi
    local reply=''
    IFS= read -r -k 1 "reply?RWS: switch to $host ($ws)? [Y/n] " < /dev/tty || return 0
    [[ $reply == $'\n' ]] || print -u2 ''
    case $reply in
      ($'\n'|y|Y|o|O) choice=remote ;;
      (*) choice=local ;;
    esac
    _RWS_AUTO_CHOICE[$ws]=$choice
  fi
  [[ $choice == remote ]] || return 0
  "${{rws_cmd[@]}}" shell
  _RWS_AUTO_SUPPRESS=$volume
}}
typeset -ga chpwd_functions
if (( ! ${{chpwd_functions[(I)_rws_auto_shell]}} )); then
  chpwd_functions+=(_rws_auto_shell)
fi
# Also cover a shell that starts inside a mounted workspace.
_rws_auto_shell
"#,
        rws = quoted,
        config_selection = config_selection
    ))
}

/// Return the single .zshrc line that `rws hook install` manages.
pub fn install_line(rws: &Path, config: Option<&Path>) -> Result<String, String> {
    let rws = quote("rws executable", rws)?;
    let config = match config {
        Some(config) => format!(" --config {}", quote("configuration", config)?),
        None => String::new(),
    };
    Ok(format!("eval \"$({rws}{config} hook zsh)\" {MARKER}"))
}

/// Append or replace the managed hook line in the given .zshrc file.
/// Everything else in the file is preserved byte for byte.
pub fn install(zshrc: &Path, rws: &Path, config: Option<&Path>) -> Result<String, String> {
    let line = install_line(rws, config)?;
    update(zshrc, &line, true)?;
    Ok(line)
}

fn update(zshrc: &Path, replacement: &str, append_missing: bool) -> Result<bool, String> {
    // Resolve existing symlinks once and replace the regular target, retaining
    // the user's symlink. Refuse dangling symlinks rather than overwrite them.
    let target = match fs::symlink_metadata(zshrc) {
        Ok(_) => {
            fs::canonicalize(zshrc).map_err(|e| format!("resolve {}: {e}", zshrc.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if !append_missing {
                return Ok(false);
            }
            zshrc.to_path_buf()
        }
        Err(e) => return Err(format!("inspect {}: {e}", zshrc.display())),
    };
    let metadata = match fs::metadata(&target) {
        Ok(m) if m.is_file() => Some(m),
        Ok(_) => return Err("shell hook target must be a regular file".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("inspect shell hook: {e}")),
    };
    let original = if metadata.is_some() {
        fs::read(&target).map_err(|e| format!("read shell hook: {e}"))?
    } else {
        Vec::new()
    };
    let mut updated = Vec::new();
    let mut found = false;
    for bytes in original.split_inclusive(|b| *b == b'\n') {
        // Non-UTF-8 user content is unrelated to our generated UTF-8 line.
        if let Ok(line) = std::str::from_utf8(bytes)
            && let Some((start, end)) = managed_range(line)
        {
            if found {
                return Err("duplicate active RWS hooks; keep exactly one active managed hook in the shell file, then retry repair (file left unchanged)".into());
            }
            found = true;
            updated.extend_from_slice(&bytes[..start]);
            updated.extend_from_slice(replacement.as_bytes());
            updated.extend_from_slice(&bytes[end..]);
            continue;
        }
        updated.extend_from_slice(bytes);
    }
    if !found && append_missing {
        if !updated.is_empty() && !updated.ends_with(b"\n") {
            updated.push(b'\n');
        }
        updated.extend_from_slice(replacement.as_bytes());
        updated.push(b'\n');
    }
    if updated == original {
        return Ok(false);
    }
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if metadata.is_some() {
        exclusive_file(parent, ".rws-hook-backup", &original, None)?;
    }
    let temporary = exclusive_file(
        parent,
        ".rws-hook",
        &updated,
        metadata.as_ref().map(|m| m.permissions()),
    )?;
    let result = (|| {
        // Detect changes during preparation, including a replaced symlink target.
        match (&metadata, fs::symlink_metadata(&target)) {
            (Some(before), Ok(now))
                if now.is_file() && before.dev() == now.dev() && before.ino() == now.ino() =>
            {
                if fs::read(&target).map_err(|e| e.to_string())? != original {
                    return Err("shell hook changed during update; left unchanged".into());
                }
            }
            (None, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err("shell hook target changed during update; left unchanged".into()),
        }
        fs::rename(&temporary, &target).map_err(|e| format!("replace shell hook: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|_| true)
}

fn exclusive_file(
    parent: &Path,
    prefix: &str,
    contents: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    for attempt in 0..100 {
        let path = parent.join(format!(
            "{prefix}-{timestamp}-{}-{attempt}",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                let result = (|| {
                    file.write_all(contents)?;
                    if let Some(permissions) = permissions {
                        file.set_permissions(permissions)?;
                    }
                    file.sync_all()
                })();
                if let Err(error) = result {
                    let _ = fs::remove_file(&path);
                    return Err(format!("write {}: {error}", path.display()));
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("create {}: {e}", path.display())),
        }
    }
    Err("could not reserve unique shell hook file".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn current_hook_accepts_indent_and_comments_but_not_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".zshrc");
        let binary = Path::new("/opt/rws");
        let line = install_line(binary, None).unwrap();
        assert!(!references_current(&path, binary, None).unwrap());
        std::fs::write(&path, format!("# {line}\n  {line} # keep me\r\n")).unwrap();
        assert!(references_current(&path, binary, None).unwrap());
        assert!(!references_current(&path, Path::new("/new/rws"), None).unwrap());
        std::fs::write(&path, format!("{line}\n\t{line}\n")).unwrap();
        assert!(!references_current(&path, binary, None).unwrap());
        std::fs::write(&path, format!("# {line}\n")).unwrap();
        assert!(!references_current(&path, binary, None).unwrap());
    }

    #[test]
    fn duplicate_hook_repair_refuses_without_changing_user_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".zshrc");
        let old = install_line(Path::new("/old/rws"), None).unwrap();
        let original = format!("{old} # first\n# {old}\n{old} # second\n");
        fs::write(&path, &original).unwrap();
        let error = refresh_existing(&path, Path::new("/new/rws"), None).unwrap_err();
        assert!(
            error.contains("duplicate") && error.contains("one active"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn refresh_preserves_bytes_comments_and_disabled_hooks() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".zshrc");
        let old = install_line(Path::new("/old/rws"), None).unwrap();
        let before =
            format!("# disabled: {old}\r\n\r\n  {old} # personal note\r\n\nexport X=1\n\n");
        std::fs::write(&path, &before).unwrap();
        assert!(refresh_existing(&path, Path::new("/new/rws"), None).unwrap());
        let expected = before.replacen(
            &format!("  {old}"),
            &format!("  {}", install_line(Path::new("/new/rws"), None).unwrap()),
            1,
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(!refresh_existing(&path, Path::new("/new/rws"), None).unwrap());
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            metadata.modified().unwrap()
        );
    }

    #[test]
    fn refresh_missing_and_disabled_are_untouched_but_install_enables() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".zshrc");
        assert!(!refresh_existing(&path, Path::new("/new/rws"), None).unwrap());
        assert!(!path.exists());
        let original = format!(
            "# {}\n\n\n",
            install_line(Path::new("/old/rws"), None).unwrap()
        );
        std::fs::write(&path, &original).unwrap();
        assert!(!refresh_existing(&path, Path::new("/new/rws"), None).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        install(&path, Path::new("/new/rws"), None).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!(
                "{original}{}\n",
                install_line(Path::new("/new/rws"), None).unwrap()
            )
        );
    }

    #[test]
    fn refresh_symlink_preserves_target_permissions_and_backup() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".zshrc");
        let target = temp.path().join("real-zshrc");
        let original = install_line(Path::new("/old/rws"), None).unwrap();
        std::fs::write(&target, &original).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        symlink("real-zshrc", &path).unwrap();
        refresh_existing(&path, Path::new("/new/rws"), None).unwrap();
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let backups: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".rws-hook-backup-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), original);
    }

    #[test]
    fn snippet_bakes_config_path_used_when_rws_config_is_unset() {
        let snippet = zsh_snippet(
            Path::new("/opt/rws"),
            Some(Path::new("/home/dev/.rws-local/config.json")),
        )
        .unwrap();
        assert!(snippet.contains("'/home/dev/.rws-local/config.json'"));
        // The environment variable must keep priority over the baked path.
        assert!(snippet.contains("RWS_CONFIG"));
    }

    #[test]
    fn install_line_embeds_quoted_binary_and_config() {
        let line = install_line(
            Path::new("/opt/my tools/rws"),
            Some(Path::new("/cfg/o'brien.json")),
        )
        .unwrap();
        assert!(
            line.contains(
                r#"eval "$('/opt/my tools/rws' --config '/cfg/o'\''brien.json' hook zsh)""#
            ),
            "{line}"
        );
        assert!(line.contains(MARKER));
        let bare = install_line(Path::new("/opt/rws"), None).unwrap();
        assert!(!bare.contains("--config"));
    }

    #[test]
    fn install_appends_once_and_replaces_a_stale_line() {
        let temp = tempfile::tempdir().unwrap();
        let zshrc = temp.path().join(".zshrc");
        std::fs::write(&zshrc, "export EDITOR=vim\n").unwrap();
        install(&zshrc, Path::new("/opt/rws"), None).unwrap();
        install(&zshrc, Path::new("/opt/rws"), None).unwrap();
        let content = std::fs::read_to_string(&zshrc).unwrap();
        assert_eq!(content.matches(MARKER).count(), 1, "{content}");
        assert!(content.starts_with("export EDITOR=vim\n"));
        // A new binary location replaces the previous managed line.
        install(&zshrc, Path::new("/new/rws"), None).unwrap();
        let content = std::fs::read_to_string(&zshrc).unwrap();
        assert_eq!(content.matches(MARKER).count(), 1);
        assert!(content.contains("/new/rws"));
        assert!(!content.contains("/opt/rws"));
    }

    #[test]
    fn install_creates_a_missing_zshrc() {
        let temp = tempfile::tempdir().unwrap();
        let zshrc = temp.path().join(".zshrc");
        install(&zshrc, Path::new("/opt/rws"), None).unwrap();
        assert!(std::fs::read_to_string(&zshrc).unwrap().contains(MARKER));
    }

    #[test]
    fn snippet_registers_chpwd_hook_and_embeds_quoted_binary() {
        let snippet = zsh_snippet(Path::new("/opt/my tools/rws"), None).unwrap();
        assert!(snippet.contains("chpwd_functions"));
        assert!(snippet.contains("_rws_auto_shell"));
        assert!(snippet.contains("'/opt/my tools/rws'"));
        // Detection queries context for the current directory before switching.
        assert!(snippet.contains("context --cwd"));
        // Opt-out and re-entry guard are part of the contract.
        assert!(snippet.contains("RWS_NO_AUTO_SHELL"));
        assert!(snippet.contains("_RWS_AUTO_SUPPRESS"));
        // Per-shell mode choice: prompt with remote default, forced mode,
        // per-workspace memory.
        assert!(snippet.contains("[Y/n]"));
        assert!(snippet.contains("RWS_AUTO_MODE"));
        assert!(snippet.contains("_RWS_AUTO_CHOICE"));
    }

    #[test]
    fn snippet_escapes_single_quotes_in_binary_path() {
        let snippet = zsh_snippet(Path::new("/opt/o'brien/rws"), None).unwrap();
        assert!(snippet.contains(r"'/opt/o'\''brien/rws'"));
    }

    #[test]
    fn snippet_rejects_paths_with_newlines() {
        assert!(zsh_snippet(Path::new("/opt/bad\nname/rws"), None).is_err());
        assert!(zsh_snippet(Path::new("/opt/rws"), Some(Path::new("/cfg\n.json"))).is_err());
    }
}
