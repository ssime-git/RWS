//! Generate the zsh integration that switches into `rws shell` on entering a mount.
use std::path::Path;

/// Marker comment identifying the .zshrc line managed by `rws hook install`.
pub const MARKER: &str = "# RWS auto-shell hook";

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
    let existing = match std::fs::read_to_string(zshrc) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read {}: {e}", zshrc.display())),
    };
    let mut kept: Vec<&str> = existing.lines().filter(|l| !l.contains(MARKER)).collect();
    while kept.last().is_some_and(|l| l.is_empty()) {
        kept.pop();
    }
    let mut content = kept.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    content.push_str(&line);
    content.push('\n');
    std::fs::write(zshrc, content).map_err(|e| format!("write {}: {e}", zshrc.display()))?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
