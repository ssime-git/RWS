//! Transfer the bundled, auditable Arch NFS setup to the configured SSH host.
//! The administrator password is entered only in the user's remote sudo prompt.
use crate::{native_nfs, process, workspace::Workspace};
use std::{
    io::{IsTerminal, Write},
    process::{Command, Stdio},
    time::Duration,
};

const SCRIPT: &str = include_str!("../scripts/rws-nfs-server.sh");
const REMOTE_STAGE: &str =
    "umask 077; p=$(mktemp /tmp/rws-nfs-stage.XXXXXXXX) && cat > \"$p\" && printf '%s\\n' \"$p\"";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Setup,
    Remove,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::Remove => "remove",
        }
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ssh(host: &str) -> Command {
    let mut command = Command::new("/usr/bin/ssh");
    command.args(["-o", "ConnectTimeout=8", "-o", "BatchMode=yes", "--", host]);
    command
}

fn captured(command: &mut Command, timeout: Duration) -> Result<String, String> {
    let output = process::output(command, timeout)?;
    if !output.status.success() {
        return Err(format!(
            "SSH failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("SSH output is not UTF-8: {e}"))
}

fn client_ip(workspace: &Workspace) -> Result<String, String> {
    let mut command = ssh(&workspace.host);
    command.arg("printf '%s\\n' \"${SSH_CLIENT%% *}\"");
    let address = captured(&mut command, Duration::from_secs(20))?;
    let address = address.trim();
    if !valid_tailscale_ip(address) {
        return Err("SSH did not arrive from a Tailscale IPv4 address".into());
    }
    Ok(address.into())
}

fn valid_tailscale_ip(address: &str) -> bool {
    let Some(parts) = address
        .split('.')
        .map(|part| part.parse::<u8>().ok())
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    parts.len() == 4 && parts[0] == 100
}

fn digest(output: &str) -> Result<&str, String> {
    let hash = output
        .split_whitespace()
        .next()
        .ok_or("missing script SHA-256")?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid script SHA-256".into());
    }
    Ok(hash)
}

fn stage_path(output: &str) -> Result<&str, String> {
    let path = output.trim();
    if !path.starts_with("/tmp/rws-nfs-stage.")
        || path.len() > 100
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'-' | b'_'))
    {
        return Err("remote staging path has an unexpected shape".into());
    }
    Ok(path)
}

pub fn run(workspace: &Workspace, action: Action, dry_run: bool) -> Result<(), String> {
    workspace.validate()?;
    let server_ip = native_nfs::resolve_endpoint(workspace)?;
    if !valid_tailscale_ip(&server_ip) {
        return Err("the SSH endpoint must resolve to a Tailscale IPv4 address".into());
    }
    let mac_ip = client_ip(workspace)?;
    if dry_run {
        println!(
            "Would run audited RWS NFS {} for {} on {}: export {}, client {}, server {}",
            action.name(),
            workspace.name,
            workspace.host,
            workspace.remote_root,
            mac_ip,
            server_ip
        );
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(
            "run NFS server setup from an interactive terminal for the remote sudo prompt".into(),
        );
    }
    let mut local_script = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    local_script
        .write_all(SCRIPT.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut local_hash = Command::new("/usr/bin/shasum");
    local_hash.args(["-a", "256"]).arg(local_script.path());
    let local_hash = digest(&captured(&mut local_hash, Duration::from_secs(10))?)?.to_owned();
    let mut stage = ssh(&workspace.host);
    stage.arg(REMOTE_STAGE);
    stage.stdin(Stdio::from(
        local_script.reopen().map_err(|e| e.to_string())?,
    ));
    let staged = captured(&mut stage, Duration::from_secs(30))?;
    let staged = stage_path(&staged)?;
    let mut remote_hash = ssh(&workspace.host);
    remote_hash.arg(format!("sha256sum -- {}", quote(staged)));
    let remote_hash = captured(&mut remote_hash, Duration::from_secs(20))
        .and_then(|output| digest(&output).map(str::to_owned));
    if remote_hash.as_deref() != Ok(local_hash.as_str()) {
        let mut cleanup = ssh(&workspace.host);
        cleanup.arg(format!("rm -- {}", quote(staged)));
        let _ = captured(&mut cleanup, Duration::from_secs(20));
        return Err("remote NFS setup script failed SHA-256 verification; sudo was not run".into());
    }
    let remote_command = format!(
        "sudo /bin/bash {} {} {} {} {}",
        quote(staged),
        action.name(),
        quote(&workspace.remote_root),
        quote(&mac_ip),
        quote(&server_ip)
    );
    let mut execute = Command::new("/usr/bin/ssh");
    execute.args([
        "-tt",
        "-o",
        "ConnectTimeout=8",
        "--",
        &workspace.host,
        &remote_command,
    ]);
    let result = process::run_interactive(&mut execute, Duration::from_secs(600));
    let mut cleanup = ssh(&workspace.host);
    cleanup.arg(format!("rm -- {}", quote(staged)));
    let cleanup_result = captured(&mut cleanup, Duration::from_secs(20));
    if let Err(error) = cleanup_result {
        return Err(format!(
            "remote setup finished, but private staging file {staged} could not be removed: {error}"
        ));
    }
    let status = result?;
    if !status.success() {
        return Err(format!("remote NFS {} failed with {status}", action.name()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_expected_remote_staging_paths() {
        assert_eq!(
            stage_path("/tmp/rws-nfs-stage.Ab12cd34\n").unwrap(),
            "/tmp/rws-nfs-stage.Ab12cd34"
        );
        assert!(stage_path("/tmp/rws-nfs-stage.ok; touch /tmp/evil").is_err());
        assert!(stage_path("/etc/exports").is_err());
    }

    #[test]
    fn validates_a_complete_sha256_before_trusting_transfer() {
        assert!(digest(&format!("{}  script.sh", "a".repeat(64))).is_ok());
        assert!(digest("bad  script.sh").is_err());
        assert!(digest(&format!("{}  script.sh", "z".repeat(64))).is_err());
    }

    #[test]
    fn requires_tailscale_ipv4_addresses() {
        assert!(valid_tailscale_ip("100.64.59.41"));
        assert!(!valid_tailscale_ip("100.999.2.3"));
        assert!(!valid_tailscale_ip("192.168.1.1"));
        assert!(!valid_tailscale_ip("100.1.2.3; true"));
    }

    #[test]
    fn quotes_remote_path_as_one_literal_argument() {
        assert_eq!(quote("/home/a'b"), "'/home/a'\\''b'");
    }
}
