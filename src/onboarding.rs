//! Guided VM onboarding: read-only checks of the Mac and of a VM, each failure
//! explained with a concrete fix. Nothing here installs or configures anything.
use crate::process;
use serde::Serialize;
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

/// One finding. `code` is a stable machine-readable reason (for UIs that
/// translate); `fix` is written for a person and present for warn/fail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Check {
    pub id: String,
    pub status: Status,
    pub code: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshFailure {
    Unresolved,
    Refused,
    Unreachable,
    KeyDenied,
    TailscalePolicy,
    HostKeyUnknown,
    HostKeyChanged,
    Other,
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn check(
    id: &str,
    status: Status,
    code: &str,
    detail: impl Into<String>,
    fix: Option<String>,
) -> Check {
    Check {
        id: id.into(),
        status,
        code: code.into(),
        detail: detail.into(),
        fix,
    }
}

/// Classify the stderr of a failed `ssh -o BatchMode=yes HOST true`. The
/// order matters: a changed key also says "verification failed", and a
/// Tailscale SSH policy refusal is not an ordinary key problem.
pub fn classify_ssh_failure(stderr: &str) -> SshFailure {
    let text = stderr.to_lowercase();
    let has = |needle: &str| text.contains(needle);
    if has("remote host identification has changed") {
        SshFailure::HostKeyChanged
    } else if has("host key verification failed") {
        SshFailure::HostKeyUnknown
    } else if has("tailnet policy does not permit") {
        SshFailure::TailscalePolicy
    } else if has("could not resolve hostname") {
        SshFailure::Unresolved
    } else if has("connection refused") {
        SshFailure::Refused
    } else if has("timed out")
        || has("no route to host")
        || has("network is unreachable")
        || has("host is down")
    {
        SshFailure::Unreachable
    } else if has("permission denied") {
        SshFailure::KeyDenied
    } else {
        SshFailure::Other
    }
}

/// The `vm.reachable` finding for a failed `ssh HOST true`.
pub fn ssh_failure_check(failure: &SshFailure, host: &str, stderr: &str) -> Check {
    let name = host.rsplit('@').next().unwrap_or(host);
    let (code, detail, fix) = match failure {
        SshFailure::Unresolved => (
            "unresolved",
            format!("Cannot resolve the host name `{name}`."),
            "Check the spelling. For a Tailscale machine use its MagicDNS name (see the machine list from `rws onboard check`) and make sure Tailscale is connected.".to_string(),
        ),
        SshFailure::Refused => (
            "refused",
            "The VM refused the connection on the SSH port.".to_string(),
            "Start the SSH server on the VM (for example `sudo systemctl enable --now sshd`), or check the port in your SSH configuration.".to_string(),
        ),
        SshFailure::Unreachable => (
            "unreachable",
            "The VM did not answer (timeout or no route).".to_string(),
            "Connect Tailscale on this Mac and on the VM (it must show as online in `tailscale status`), then retry.".to_string(),
        ),
        SshFailure::KeyDenied => (
            "key_denied",
            "The VM rejected your SSH key.".to_string(),
            format!("Install your public key on the VM once: `ssh-copy-id {host}` (it asks for the VM password a single time). No key yet? Create one first with `ssh-keygen -t ed25519`."),
        ),
        SshFailure::TailscalePolicy => (
            "tailscale_policy",
            "Tailscale SSH refused this user.".to_string(),
            "Your tailnet policy does not allow this SSH user on that machine. Use a user your Tailscale ACL permits (see the SSH rules in the Tailscale admin console), or an account that has a normal SSH key.".to_string(),
        ),
        SshFailure::HostKeyUnknown => (
            "host_key_unknown",
            "This Mac has never trusted the VM's host key.".to_string(),
            format!("Connect once and confirm the fingerprint: `ssh {host}` (answer `yes` only if you recognise the machine), then retry."),
        ),
        SshFailure::HostKeyChanged => (
            "host_key_changed",
            "The VM's host key changed: it was rebuilt, or something is impersonating it.".to_string(),
            format!("Do not continue blindly. If you rebuilt the VM, remove the old key with `ssh-keygen -R {name}` and connect again; otherwise investigate before trusting it."),
        ),
        SshFailure::Other => (
            "ssh_error",
            stderr
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("SSH failed without a message.")
                .to_string(),
            format!("Run `ssh -v {host} true` to see the full diagnostic."),
        ),
    };
    check("vm.reachable", Status::Fail, code, detail, Some(fix))
}

/// SFTP protocol version 3 INIT packet sent to `ssh -s HOST sftp`.
pub const SFTP_INIT: [u8; 9] = [0, 0, 0, 5, 1, 0, 0, 0, 3];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpHello {
    pub version: u32,
    pub extensions: Vec<(String, String)>,
}

/// Total bytes of the reply once its 4-byte length prefix is readable.
pub fn sftp_reply_len(bytes: &[u8]) -> Option<usize> {
    let prefix: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(4 + u32::from_be_bytes(prefix) as usize)
}

fn sftp_string<'a>(bytes: &'a [u8], at: &mut usize) -> Result<&'a str, String> {
    let truncated = || "truncated SFTP reply".to_string();
    let prefix: [u8; 4] = bytes
        .get(*at..*at + 4)
        .ok_or_else(truncated)?
        .try_into()
        .map_err(|_| truncated())?;
    let len = u32::from_be_bytes(prefix) as usize;
    let start = *at + 4;
    let text = bytes.get(start..start + len).ok_or_else(truncated)?;
    *at = start + len;
    std::str::from_utf8(text).map_err(|_| "SFTP extension is not UTF-8".to_string())
}

pub fn parse_sftp_hello(bytes: &[u8]) -> Result<SftpHello, String> {
    let total = sftp_reply_len(bytes).ok_or("incomplete SFTP reply")?;
    if bytes.len() < total {
        return Err("incomplete SFTP reply".into());
    }
    let body = &bytes[4..total];
    if body.len() < 5 || body[0] != 2 {
        return Err("the remote command is not an SFTP server".into());
    }
    let version = u32::from_be_bytes(body[1..5].try_into().expect("four bytes"));
    let mut extensions = Vec::new();
    let mut at = 5;
    while at < body.len() {
        let name = sftp_string(body, &mut at)?.to_string();
        let data = sftp_string(body, &mut at)?.to_string();
        extensions.push((name, data));
    }
    Ok(SftpHello {
        version,
        extensions,
    })
}

pub fn has_posix_rename(hello: &SftpHello) -> bool {
    hello
        .extensions
        .iter()
        .any(|(name, _)| name == "posix-rename@openssh.com")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Peer {
    pub name: String,
    pub dns_name: String,
    pub online: bool,
    pub os: String,
    pub ips: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailscaleState {
    pub backend_state: String,
    pub peers: Vec<Peer>,
}

/// Parse `tailscale status --json`. A peer's `HostName` is unreliable (an
/// iPhone reports "localhost"), so machines are identified by the first label
/// of their MagicDNS name; peers without one are not addressable and dropped.
pub fn parse_tailscale_status(json: &str) -> Result<TailscaleState, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("tailscale status is not JSON: {e}"))?;
    let backend_state = value["BackendState"]
        .as_str()
        .unwrap_or("Unknown")
        .to_string();
    let mut peers: Vec<Peer> = value["Peer"]
        .as_object()
        .map(|peers| {
            peers
                .values()
                .filter_map(|peer| {
                    let dns_name = peer["DNSName"].as_str()?.trim_end_matches('.').to_string();
                    if dns_name.is_empty() {
                        return None;
                    }
                    Some(Peer {
                        name: dns_name.split('.').next()?.to_string(),
                        dns_name,
                        online: peer["Online"].as_bool().unwrap_or(false),
                        os: peer["OS"].as_str().unwrap_or("").to_string(),
                        ips: peer["TailscaleIPs"]
                            .as_array()
                            .map(|ips| {
                                ips.iter()
                                    .filter_map(|ip| ip.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    peers.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.name.cmp(&b.name)));
    Ok(TailscaleState {
        backend_state,
        peers,
    })
}

/// The `mac.tailscale` finding for a parsed status.
pub fn tailscale_check(state: &TailscaleState) -> Check {
    match state.backend_state.as_str() {
        "Running" => check(
            "mac.tailscale",
            Status::Ok,
            "ok",
            format!("Tailscale is connected ({} machines).", state.peers.len()),
            None,
        ),
        "Stopped" => check(
            "mac.tailscale",
            Status::Fail,
            "tailscale_stopped",
            "Tailscale is installed but switched off.",
            Some("Turn Tailscale on: open its menu bar app and choose Connect, then retry.".into()),
        ),
        "NeedsLogin" | "NeedsMachineAuth" => check(
            "mac.tailscale",
            Status::Fail,
            "tailscale_login",
            "Tailscale needs you to sign in.",
            Some("Log in to Tailscale from its menu bar app (approve this Mac if asked), then retry.".into()),
        ),
        other => check(
            "mac.tailscale",
            Status::Fail,
            "tailscale_state",
            format!("Tailscale is in state `{other}`."),
            Some("Open the Tailscale app and make sure it shows as connected, then retry.".into()),
        ),
    }
}

/// Whether `host` (`user@name`, a MagicDNS name or a Tailscale IP) is one of
/// the tailnet's machines.
pub fn peer_matches_host(peers: &[Peer], host: &str) -> bool {
    let name = host
        .rsplit('@')
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_lowercase();
    !name.is_empty()
        && peers.iter().any(|peer| {
            peer.name.to_lowercase() == name
                || peer.dns_name.to_lowercase() == name
                || peer.ips.iter().any(|ip| ip.to_lowercase() == name)
        })
}

/// Remote script printing exactly one of `ok`, `missing`, `notdir`, `readonly`.
pub fn directory_probe_script(dir: &str) -> Result<String, String> {
    if !dir.starts_with('/') || dir.contains(['\0', '\n', '\r']) {
        return Err("the remote folder must be an absolute path without control characters".into());
    }
    Ok(format!(
        "d={}; if [ ! -e \"$d\" ]; then echo missing; elif [ ! -d \"$d\" ]; then echo notdir; elif [ ! -w \"$d\" ]; then echo readonly; else echo ok; fi",
        quote(dir)
    ))
}

/// The `vm.directory` finding for the probe script's output.
pub fn directory_check(dir: &str, probe_output: &str) -> Check {
    let (status, code, detail, fix) = match probe_output.trim() {
        "ok" => (
            Status::Ok,
            "ok",
            format!("`{dir}` exists and is writable."),
            None,
        ),
        "missing" => (
            Status::Fail,
            "directory_missing",
            format!("`{dir}` does not exist on the VM."),
            Some(format!("Create it on the VM: `mkdir -p {}`.", quote(dir))),
        ),
        "notdir" => (
            Status::Fail,
            "directory_not_a_folder",
            format!("`{dir}` exists but is not a folder."),
            Some("Choose an existing folder.".to_string()),
        ),
        "readonly" => (
            Status::Fail,
            "directory_not_writable",
            format!("Your user cannot write to `{dir}`."),
            Some(format!(
                "Give your user write permission (for example `sudo chown $USER {}`) or choose a folder you own.",
                quote(dir)
            )),
        ),
        other => (
            Status::Fail,
            "directory_probe_failed",
            format!("Unexpected answer while checking `{dir}`: {other}"),
            Some("Connect with `ssh` and inspect the folder by hand.".to_string()),
        ),
    };
    check("vm.directory", status, code, detail, fix)
}

/// Everything `rws onboard check` found, in the order it was checked.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub ok: bool,
    pub checks: Vec<Check>,
    pub machines: Vec<Peer>,
}

fn tailscale_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RWS_TAILSCALE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("tailscale"))
            .find(|candidate| candidate.is_file())
    });
    on_path.or_else(|| {
        let app = PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale");
        app.is_file().then_some(app)
    })
}

/// `Ok(None)`: no Tailscale command-line tool on this Mac.
fn tailscale_state() -> Result<Option<TailscaleState>, String> {
    let Some(binary) = tailscale_binary() else {
        return Ok(None);
    };
    let output = process::output(
        Command::new(binary).args(["status", "--json"]),
        Duration::from_secs(10),
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(state) = parse_tailscale_status(&stdout) {
        return Ok(Some(state));
    }
    let text = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
    if text.to_lowercase().contains("stopped") {
        return Ok(Some(TailscaleState {
            backend_state: "Stopped".into(),
            peers: Vec::new(),
        }));
    }
    Err(text
        .lines()
        .next()
        .unwrap_or("tailscale failed")
        .trim()
        .to_string())
}

fn mac_ssh_check() -> Check {
    match process::output(Command::new("ssh").arg("-V"), Duration::from_secs(5)) {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stderr);
            check(
                "mac.ssh",
                Status::Ok,
                "ok",
                text.lines().next().unwrap_or("OpenSSH is installed").trim(),
                None,
            )
        }
        _ => check(
            "mac.ssh",
            Status::Fail,
            "ssh_missing",
            "The `ssh` command is not available on this Mac.",
            Some(
                "Install Apple's command line tools with `xcode-select --install`, then retry."
                    .into(),
            ),
        ),
    }
}

fn mac_tailscale_check(state: &Result<Option<TailscaleState>, String>) -> Check {
    match state {
        Ok(Some(state)) => tailscale_check(state),
        Ok(None) => check(
            "mac.tailscale",
            Status::Warn,
            "tailscale_cli_missing",
            "The Tailscale command-line tool was not found, so your machines cannot be listed.",
            Some("Install and open the Tailscale app (https://tailscale.com/download/mac). Ignore this if your VM is reachable another way.".into()),
        ),
        Err(message) => check(
            "mac.tailscale",
            Status::Fail,
            "tailscale_error",
            format!("Tailscale did not answer: {message}"),
            Some("Open the Tailscale app and make sure it is running, then retry.".into()),
        ),
    }
}

fn ssh_command(host: &str) -> Command {
    let mut command = Command::new("ssh");
    command.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "--", host]);
    command
}

fn vm_reachable(host: &str) -> Check {
    let mut command = ssh_command(host);
    command.arg("true");
    match process::output(&mut command, Duration::from_secs(30)) {
        Ok(out) if out.status.success() => check(
            "vm.reachable",
            Status::Ok,
            "ok",
            format!("SSH key login to {host} works."),
            None,
        ),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            ssh_failure_check(&classify_ssh_failure(&stderr), host, &stderr)
        }
        Err(message) => ssh_failure_check(&SshFailure::Unreachable, host, &message),
    }
}

/// Speak the SFTP handshake over `ssh -s HOST sftp` and return the server's
/// version reply. The reply length is bounded: a login shell that happens to
/// answer is rejected immediately instead of being waited on.
fn sftp_handshake(host: &str, timeout: Duration) -> Result<SftpHello, String> {
    // `-s` is an ssh option and must come before `--`, or it would reach the
    // remote shell as part of the command.
    let mut command = Command::new("ssh");
    command.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-s",
        "--",
        host,
        "sftp",
    ]);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("start ssh: {e}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 512];
        let result = loop {
            if let Some(total) = sftp_reply_len(&buffer) {
                if total > 64 * 1024 {
                    break Err("the remote command is not an SFTP server".to_string());
                }
                if buffer.len() >= total {
                    break parse_sftp_hello(&buffer);
                }
            }
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    let mut text = String::new();
                    let _ = stderr.read_to_string(&mut text);
                    let reason = text.lines().next().unwrap_or("closed without answering");
                    break Err(format!("no SFTP answer: {reason}"));
                }
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            }
        };
        let _ = sender.send(result);
    });
    stdin
        .write_all(&SFTP_INIT)
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("send SFTP handshake: {e}"))?;
    let result = receiver
        .recv_timeout(timeout)
        .unwrap_or_else(|_| Err("no SFTP answer within the time limit".into()));
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn vm_sftp_checks(host: &str) -> (Check, Option<Check>) {
    match sftp_handshake(host, Duration::from_secs(20)) {
        Ok(hello) => {
            let atomic = if has_posix_rename(&hello) {
                check(
                    "vm.atomic_save",
                    Status::Ok,
                    "ok",
                    "The SFTP server supports atomic replacement.",
                    None,
                )
            } else {
                check(
                    "vm.atomic_save",
                    Status::Warn,
                    "no_posix_rename",
                    "The SFTP server does not advertise atomic replacement; some editors may fail to save.",
                    Some("Use OpenSSH's own `sftp-server` (not a minimal SFTP server) on the VM.".into()),
                )
            };
            (
                check(
                    "vm.sftp",
                    Status::Ok,
                    "ok",
                    format!("The VM answers the SFTP protocol (version {}).", hello.version),
                    None,
                ),
                Some(atomic),
            )
        }
        Err(message) => (
            check(
                "vm.sftp",
                Status::Fail,
                "sftp_unavailable",
                message,
                Some("Enable the SFTP subsystem on the VM: add `Subsystem sftp internal-sftp` (or the path of OpenSSH's sftp-server) to /etc/ssh/sshd_config, then reload sshd.".into()),
            ),
            None,
        ),
    }
}

fn vm_directory(host: &str, dir: &str) -> Check {
    let script = match directory_probe_script(dir) {
        Ok(script) => script,
        Err(message) => {
            return check(
                "vm.directory",
                Status::Fail,
                "directory_invalid",
                message,
                Some("Give an absolute path such as /home/you/project.".into()),
            );
        }
    };
    let mut command = ssh_command(host);
    command.arg(script);
    match process::output(&mut command, Duration::from_secs(30)) {
        Ok(out) if out.status.success() => {
            directory_check(dir, &String::from_utf8_lossy(&out.stdout))
        }
        Ok(out) => directory_check(dir, &String::from_utf8_lossy(&out.stderr)),
        Err(message) => directory_check(dir, &message),
    }
}

/// The name SSH would really connect to (`ssh -G` expands config aliases).
fn resolved_hostname(host: &str) -> Option<String> {
    let out = process::output(
        Command::new("ssh").args(["-G", "--", host]),
        Duration::from_secs(5),
    )
    .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("hostname ").map(|h| h.trim().to_string()))
}

fn vm_tailscale_peer(host: &str, peers: &[Peer]) -> Check {
    let known = peer_matches_host(peers, host)
        || resolved_hostname(host).is_some_and(|name| peer_matches_host(peers, &name));
    if known {
        check(
            "vm.tailscale_peer",
            Status::Ok,
            "ok",
            format!("{host} is one of your Tailscale machines."),
            None,
        )
    } else {
        check(
            "vm.tailscale_peer",
            Status::Warn,
            "outside_tailscale",
            format!("{host} is not one of your Tailscale machines."),
            Some("Fine if the VM is reachable another way; for a private link, add it to your tailnet.".into()),
        )
    }
}

/// Run every check. Read-only: nothing is created, installed or changed.
pub fn run_check(host: Option<&str>, remote: Option<&str>) -> Report {
    let mut checks = vec![mac_ssh_check()];
    let state = tailscale_state();
    checks.push(mac_tailscale_check(&state));
    let peers: Vec<Peer> = match &state {
        Ok(Some(state)) => state.peers.clone(),
        _ => Vec::new(),
    };
    if let Some(host) = host {
        let reachable = vm_reachable(host);
        let reached = reachable.status == Status::Ok;
        checks.push(reachable);
        if matches!(state, Ok(Some(_))) {
            checks.push(vm_tailscale_peer(host, &peers));
        }
        if reached {
            let (sftp, atomic) = vm_sftp_checks(host);
            checks.push(sftp);
            checks.extend(atomic);
            if let Some(dir) = remote {
                checks.push(vm_directory(host, dir));
            }
        }
    }
    Report {
        ok: !checks.iter().any(|c| c.status == Status::Fail),
        checks,
        machines: peers,
    }
}

/// Human rendering of a report, with each fix under its finding.
pub fn format_report(report: &Report, host_given: bool) -> String {
    let mut text = String::from("RWS onboarding check\n");
    for item in &report.checks {
        let mark = match item.status {
            Status::Ok => "ok  ",
            Status::Warn => "warn",
            Status::Fail => "FAIL",
        };
        text.push_str(&format!("  {mark} {:<18} {}\n", item.id, item.detail));
        if let Some(fix) = &item.fix {
            text.push_str(&format!("       fix: {fix}\n"));
        }
    }
    if !host_given && !report.machines.is_empty() {
        text.push_str("\nMachines on your tailnet (use `--host USER@NAME`):\n");
        for peer in &report.machines {
            let state = if peer.online { "online" } else { "offline" };
            text.push_str(&format!("  {:<22} {state}, {}\n", peer.name, peer.os));
        }
    }
    text.push_str(if report.ok {
        "\nReady.\n"
    } else {
        "\nNot ready: fix the FAIL items above, then run this check again.\n"
    });
    text
}

/// What `rws onboard add` is asked to set up.
#[derive(Clone, Debug)]
pub struct AddRequest {
    pub name: String,
    pub host: String,
    pub remote: String,
    pub mount: PathBuf,
    pub install: bool,
}

/// Where the current and the durable installations live.
#[derive(Clone, Debug)]
pub struct PlanContext {
    pub exe: PathBuf,
    pub config: PathBuf,
    pub canonical_config: PathBuf,
    pub canonical_binary: PathBuf,
    pub launch_agent: PathBuf,
    pub uid: u32,
}

/// One command of the plan. Steps re-invoke existing subcommands instead of
/// duplicating their logic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Step {
    pub id: String,
    pub program: String,
    pub args: Vec<String>,
    /// A failing optional step is reported as a warning and does not stop the plan.
    pub optional: bool,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StepResult {
    pub id: String,
    pub summary: String,
    pub status: String,
    pub output: String,
}

/// Plan the steps that register, mount and optionally install a workspace.
pub fn plan_add(
    context: &PlanContext,
    config: &crate::config::Config,
    request: &AddRequest,
) -> Result<Vec<Step>, String> {
    let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
    let step = |id: &str, program: &str, args: Vec<String>, optional: bool, summary: &str| Step {
        id: id.into(),
        program: program.into(),
        args,
        optional,
        summary: summary.into(),
    };
    let exe = text(&context.exe);
    let with_config = |config: &str, rest: &[&str]| -> Vec<String> {
        ["--config", config]
            .iter()
            .chain(rest)
            .map(|part| part.to_string())
            .collect()
    };
    let source = text(&context.config);
    let mut steps = Vec::new();
    if !config.mount.nfs {
        let untouched =
            config.workspaces.is_empty() && config.mount.sshfs.is_none() && !config.mount.fskit;
        if !untouched {
            return Err("this configuration uses the legacy SSHFS/FSKit backend; disconnect its workspaces and run `rws settings --backend nfs`, or point --config at a new file".into());
        }
        steps.push(step(
            "settings",
            &exe,
            with_config(&source, &["settings", "--backend", "nfs"]),
            false,
            "Use the built-in NFS backend (no macFUSE, no sudo)",
        ));
    }
    match config.workspaces.iter().find(|w| w.name == request.name) {
        Some(w)
            if w.host == request.host
                && w.remote_root == request.remote
                && w.mount_root == request.mount => {}
        Some(_) => {
            return Err(format!(
                "workspace `{}` already exists with different settings; choose another name or remove the old entry",
                request.name
            ));
        }
        None => steps.push(step(
            "workspace_add",
            &exe,
            with_config(
                &source,
                &[
                    "workspace",
                    "add",
                    &request.name,
                    "--ssh",
                    &request.host,
                    "--remote",
                    &request.remote,
                    "--mount",
                    &text(&request.mount),
                ],
            ),
            false,
            "Register the workspace",
        )),
    }
    if request.install {
        let binary = text(&context.canonical_binary);
        let canonical = text(&context.canonical_config);
        steps.push(step(
            "install",
            &exe,
            with_config(&source, &["install"]),
            false,
            "Install RWS durably for this user",
        ));
        steps.push(step(
            "autostart",
            &binary,
            vec!["autostart".into(), "install".into()],
            false,
            "Write the login agent that remounts after a restart",
        ));
        steps.push(step(
            "bootstrap",
            "launchctl",
            vec![
                "bootstrap".into(),
                format!("gui/{}", context.uid),
                text(&context.launch_agent),
            ],
            true,
            "Start the login agent now",
        ));
        steps.push(step(
            "connect",
            &binary,
            with_config(&canonical, &["connect", &request.name]),
            false,
            "Mount the workspace and prove it works",
        ));
    } else {
        steps.push(step(
            "connect",
            &exe,
            with_config(&source, &["connect", &request.name]),
            false,
            "Mount the workspace and prove it works",
        ));
    }
    Ok(steps)
}

/// Run steps in order; a failing required step stops the run.
pub fn execute(steps: &[Step]) -> (Vec<StepResult>, bool) {
    let mut results = Vec::new();
    for step in steps {
        let outcome = process::output(
            Command::new(&step.program).args(&step.args),
            Duration::from_secs(600),
        );
        let (succeeded, output) = match outcome {
            Ok(out) => (
                out.status.success(),
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                )
                .trim()
                .to_string(),
            ),
            Err(message) => (false, message),
        };
        let status = match (succeeded, step.optional) {
            (true, _) => "done",
            (false, true) => "warning",
            (false, false) => "failed",
        };
        results.push(StepResult {
            id: step.id.clone(),
            summary: step.summary.clone(),
            status: status.into(),
            output,
        });
        if status == "failed" {
            return (results, false);
        }
    }
    (results, true)
}

/// Everything `rws onboard add` decided and did.
#[derive(Clone, Debug, Serialize)]
pub struct AddReport {
    pub ok: bool,
    pub dry_run: bool,
    pub checks: Vec<Check>,
    pub steps: Vec<Step>,
    pub results: Vec<StepResult>,
    pub mount: String,
}

/// Human rendering of `add`: the failed checks, or the plan, or the results.
pub fn format_add(report: &AddReport) -> String {
    let checks_ok = !report.checks.iter().any(|c| c.status == Status::Fail);
    if !checks_ok {
        return format_report(
            &Report {
                ok: false,
                checks: report.checks.clone(),
                machines: Vec::new(),
            },
            true,
        );
    }
    let warnings = report
        .checks
        .iter()
        .filter(|c| c.status == Status::Warn)
        .count();
    let mut text = format!("Checks passed ({warnings} warning(s)).\n");
    for item in report.checks.iter().filter(|c| c.status == Status::Warn) {
        text.push_str(&format!("  warn {}: {}\n", item.id, item.detail));
    }
    if report.dry_run {
        text.push_str("\nPlan:\n");
        for (index, step) in report.steps.iter().enumerate() {
            text.push_str(&format!("  {}. {}\n", index + 1, step.summary));
        }
        text.push_str("\nDry run: nothing was changed. Run again without --dry-run to apply.\n");
        return text;
    }
    text.push('\n');
    for result in &report.results {
        let mark = match result.status.as_str() {
            "done" => "done",
            "warning" => "warn",
            _ => "FAIL",
        };
        text.push_str(&format!("  [{mark}] {}\n", result.summary));
        if result.status != "done" && !result.output.is_empty() {
            for line in result.output.lines() {
                text.push_str(&format!("         {line}\n"));
            }
        }
    }
    if report.ok {
        text.push_str(&format!(
            "\nDone. Your VM folder is mounted at {} (open it in Finder).\nOptional next steps:\n  - terminal auto-switch on `cd`: rws hook install\n  - Delta agent rules: rws delta-rules\n",
            report.mount
        ));
    } else {
        text.push_str("\nStopped: fix the step above and run the same command again. Completed steps are kept and re-running is safe.\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(check: &Check) -> &str {
        check.fix.as_deref().unwrap_or("")
    }

    #[test]
    fn classifies_real_openssh_failures() {
        use SshFailure::*;
        let cases = [
            (
                "ssh: Could not resolve hostname nope.invalid: nodename nor servname provided, or not known",
                Unresolved,
            ),
            (
                "ssh: connect to host razer-1 port 1: Connection refused",
                Refused,
            ),
            (
                "ssh: connect to host 10.255.255.1 port 22: Operation timed out",
                Unreachable,
            ),
            (
                "ssh: connect to host 10.0.0.9 port 22: No route to host",
                Unreachable,
            ),
            (
                "ssh: connect to host x port 22: Network is unreachable",
                Unreachable,
            ),
            (
                "dev@host: Permission denied (publickey,password).",
                KeyDenied,
            ),
            (
                "tailscale: tailnet policy does not permit you to SSH as user \"x\"\nConnection closed by 100.106.23.6 port 22",
                TailscalePolicy,
            ),
            ("Host key verification failed.", HostKeyUnknown),
            (
                "@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\nHost key verification failed.",
                HostKeyChanged,
            ),
            (
                "kex_exchange_identification: read: Connection reset by peer",
                Other,
            ),
        ];
        for (stderr, expected) in cases {
            assert_eq!(classify_ssh_failure(stderr), expected, "{stderr}");
        }
    }

    #[test]
    fn every_ssh_failure_names_the_host_and_a_concrete_fix() {
        use SshFailure::*;
        for failure in [
            Unresolved,
            Refused,
            Unreachable,
            KeyDenied,
            TailscalePolicy,
            HostKeyUnknown,
            HostKeyChanged,
            Other,
        ] {
            let check = ssh_failure_check(&failure, "dev@vm", "boom\nsecond line");
            assert_eq!(check.id, "vm.reachable");
            assert_eq!(check.status, Status::Fail);
            assert!(!check.code.is_empty() && check.code != "ok");
            assert!(!fix(&check).is_empty(), "{failure:?} has no fix");
        }
        let key = ssh_failure_check(&KeyDenied, "dev@vm", "");
        assert!(fix(&key).contains("ssh-copy-id dev@vm"), "{}", fix(&key));
        let unknown = ssh_failure_check(&HostKeyUnknown, "dev@vm", "");
        assert!(fix(&unknown).contains("ssh dev@vm"));
        let changed = ssh_failure_check(&HostKeyChanged, "dev@vm", "");
        assert!(fix(&changed).contains("ssh-keygen -R vm"));
        assert!(
            fix(&changed).to_lowercase().contains("do not"),
            "a changed host key must never read as a routine fix"
        );
        let other = ssh_failure_check(&Other, "dev@vm", "kex failed\nsecond line");
        assert!(other.detail.contains("kex failed") && !other.detail.contains("second line"));
    }

    fn hello_bytes(version: u32, extensions: &[(&str, &str)]) -> Vec<u8> {
        let mut body = vec![2u8];
        body.extend(version.to_be_bytes());
        for (name, data) in extensions {
            for text in [name, data] {
                body.extend((text.len() as u32).to_be_bytes());
                body.extend(text.as_bytes());
            }
        }
        let mut packet = (body.len() as u32).to_be_bytes().to_vec();
        packet.extend(body);
        packet
    }

    #[test]
    fn parses_an_openssh_sftp_version_reply() {
        let bytes = hello_bytes(
            3,
            &[
                ("posix-rename@openssh.com", "1"),
                ("fsync@openssh.com", "1"),
            ],
        );
        assert_eq!(sftp_reply_len(&bytes[..3]), None);
        assert_eq!(sftp_reply_len(&bytes[..4]), Some(bytes.len()));
        let hello = parse_sftp_hello(&bytes).unwrap();
        assert_eq!(hello.version, 3);
        assert!(has_posix_rename(&hello));
        let plain = parse_sftp_hello(&hello_bytes(3, &[])).unwrap();
        assert!(!has_posix_rename(&plain));
    }

    #[test]
    fn rejects_replies_that_are_not_an_sftp_server() {
        assert!(parse_sftp_hello(b"SSH-2.0-OpenSSH_9.9\r\n").is_err());
        let mut truncated = hello_bytes(3, &[("posix-rename@openssh.com", "1")]);
        truncated.truncate(truncated.len() - 3);
        assert!(parse_sftp_hello(&truncated).is_err());
        let mut wrong_type = hello_bytes(3, &[]);
        wrong_type[4] = 101;
        assert!(parse_sftp_hello(&wrong_type).is_err());
    }

    const TAILSCALE: &str = r#"{
      "BackendState": "Running",
      "Self": {"HostName": "mac", "DNSName": "mac.tail1234.ts.net.", "Online": true, "OS": "macOS"},
      "Peer": {
        "nodekey:a": {"HostName": "funnel-ingress-node", "DNSName": "", "Online": true, "OS": "", "TailscaleIPs": ["fd7a::1"]},
        "nodekey:b": {"HostName": "localhost", "DNSName": "iphone-2.tail1234.ts.net.", "Online": true, "OS": "iOS", "TailscaleIPs": ["100.94.78.8"]},
        "nodekey:c": {"HostName": "razer-1", "DNSName": "razer-1.tail1234.ts.net.", "Online": true, "OS": "linux", "TailscaleIPs": ["100.106.23.6", "fd7a::2"]},
        "nodekey:d": {"HostName": "old-box", "DNSName": "old-box.tail1234.ts.net.", "Online": false, "OS": "linux", "TailscaleIPs": ["100.64.0.9"]}
      }
    }"#;

    #[test]
    fn tailscale_peers_use_the_dns_name_and_list_online_machines_first() {
        let state = parse_tailscale_status(TAILSCALE).unwrap();
        assert_eq!(state.backend_state, "Running");
        let names: Vec<_> = state.peers.iter().map(|p| p.name.as_str()).collect();
        // The nameless funnel node is dropped; "localhost" is replaced by the
        // DNS label; online machines come before offline ones, then by name.
        assert_eq!(names, ["iphone-2", "razer-1", "old-box"]);
        assert!(state.peers[1].online && !state.peers[2].online);
        assert_eq!(state.peers[1].dns_name, "razer-1.tail1234.ts.net");
        assert_eq!(state.peers[1].ips[0], "100.106.23.6");
        assert!(parse_tailscale_status("not json").is_err());
    }

    #[test]
    fn tailscale_check_reports_each_backend_state() {
        let mut state = parse_tailscale_status(TAILSCALE).unwrap();
        let running = tailscale_check(&state);
        assert_eq!(
            (running.id.as_str(), running.status),
            ("mac.tailscale", Status::Ok)
        );
        state.backend_state = "Stopped".into();
        let stopped = tailscale_check(&state);
        assert_eq!(stopped.status, Status::Fail);
        assert!(
            fix(&stopped).to_lowercase().contains("turn on") || fix(&stopped).contains("Connect")
        );
        state.backend_state = "NeedsLogin".into();
        let login = tailscale_check(&state);
        assert_eq!(login.status, Status::Fail);
        assert!(fix(&login).to_lowercase().contains("log in"));
    }

    #[test]
    fn host_matching_accepts_names_dns_names_and_ips() {
        let peers = parse_tailscale_status(TAILSCALE).unwrap().peers;
        for host in [
            "razer@razer-1",
            "razer-1",
            "RAZER-1.tail1234.ts.net",
            "dev@razer-1.tail1234.ts.net.",
            "me@100.106.23.6",
        ] {
            assert!(peer_matches_host(&peers, host), "{host}");
        }
        for host in ["dev@other", "razer", "192.168.1.5", ""] {
            assert!(!peer_matches_host(&peers, host), "{host}");
        }
    }

    #[test]
    fn directory_probe_is_one_quoted_literal_and_rejects_unsafe_paths() {
        let script = directory_probe_script("/home/dev/it's here").unwrap();
        assert!(script.contains("'/home/dev/it'\\''s here'"), "{script}");
        for bad in ["relative/path", "", "/a\0b", "/a\nb"] {
            assert!(directory_probe_script(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn directory_check_explains_each_probe_outcome() {
        let ok = directory_check("/srv/p", "ok\n");
        assert_eq!((ok.id.as_str(), ok.status), ("vm.directory", Status::Ok));
        let missing = directory_check("/srv/p", "missing\n");
        assert_eq!(missing.status, Status::Fail);
        assert!(
            fix(&missing).contains("mkdir -p '/srv/p'"),
            "{}",
            fix(&missing)
        );
        let notdir = directory_check("/srv/p", "notdir");
        assert_eq!(notdir.status, Status::Fail);
        let readonly = directory_check("/srv/p", "readonly");
        assert_eq!(readonly.status, Status::Fail);
        assert!(fix(&readonly).contains("chown") || fix(&readonly).contains("permission"));
        let garbage = directory_check("/srv/p", "weird");
        assert_eq!(garbage.status, Status::Fail);
    }

    fn context() -> PlanContext {
        PlanContext {
            exe: "/opt/rws".into(),
            config: "/work/setup.json".into(),
            canonical_config: "/home/u/Library/Application Support/RWS/config.json".into(),
            canonical_binary: "/home/u/Library/Application Support/RWS/bin/rws".into(),
            launch_agent: "/home/u/Library/LaunchAgents/io.rws.mounts.plist".into(),
            uid: 501,
        }
    }

    fn request(install: bool) -> AddRequest {
        AddRequest {
            name: "demo".into(),
            host: "dev@vm".into(),
            remote: "/srv/p".into(),
            mount: "/home/u/RWS/demo".into(),
            install,
        }
    }

    fn config(json: serde_json::Value) -> crate::config::Config {
        serde_json::from_value(json).unwrap()
    }

    fn ids(steps: &[Step]) -> Vec<&str> {
        steps.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn a_fresh_configuration_selects_the_nfs_backend_adds_and_connects() {
        let fresh = config(serde_json::json!({"version":1,"workspaces":[],"mount":{}}));
        let steps = plan_add(&context(), &fresh, &request(false)).unwrap();
        assert_eq!(ids(&steps), ["settings", "workspace_add", "connect"]);
        assert_eq!(steps[0].program, "/opt/rws");
        assert_eq!(
            steps[0].args,
            [
                "--config",
                "/work/setup.json",
                "settings",
                "--backend",
                "nfs"
            ]
        );
        assert_eq!(
            steps[1].args,
            [
                "--config",
                "/work/setup.json",
                "workspace",
                "add",
                "demo",
                "--ssh",
                "dev@vm",
                "--remote",
                "/srv/p",
                "--mount",
                "/home/u/RWS/demo"
            ]
        );
        assert_eq!(
            steps[2].args,
            ["--config", "/work/setup.json", "connect", "demo"]
        );
        assert!(steps.iter().all(|s| !s.optional));
    }

    #[test]
    fn an_nfs_configuration_keeps_its_backend_and_an_identical_workspace_is_reused() {
        let existing = config(serde_json::json!({
            "version":1,
            "workspaces":[{"name":"demo","host":"dev@vm","remote_root":"/srv/p","mount_root":"/home/u/RWS/demo"}],
            "mount":{"nfs":true}
        }));
        let steps = plan_add(&context(), &existing, &request(false)).unwrap();
        assert_eq!(ids(&steps), ["connect"], "re-running must be safe");
    }

    #[test]
    fn conflicting_names_and_legacy_backends_are_refused_before_any_change() {
        let other = config(serde_json::json!({
            "version":1,
            "workspaces":[{"name":"demo","host":"dev@other","remote_root":"/srv/p","mount_root":"/home/u/RWS/demo"}],
            "mount":{"nfs":true}
        }));
        let error = plan_add(&context(), &other, &request(false)).unwrap_err();
        assert!(
            error.contains("already exists") && error.contains("demo"),
            "{error}"
        );
        let legacy = config(serde_json::json!({
            "version":1,
            "workspaces":[{"name":"old","host":"dev@vm","remote_root":"/srv/o","mount_root":"/Volumes/RWS-old"}],
            "mount":{"fskit":true,"sshfs":"/x/sshfs"}
        }));
        let error = plan_add(&context(), &legacy, &request(false)).unwrap_err();
        assert!(
            error.contains("legacy") && error.contains("settings --backend nfs"),
            "{error}"
        );
    }

    #[test]
    fn install_adds_the_durable_install_the_login_agent_and_connects_from_the_durable_copy() {
        let fresh = config(serde_json::json!({"version":1,"workspaces":[],"mount":{}}));
        let steps = plan_add(&context(), &fresh, &request(true)).unwrap();
        assert_eq!(
            ids(&steps),
            [
                "settings",
                "workspace_add",
                "install",
                "autostart",
                "bootstrap",
                "connect"
            ]
        );
        assert_eq!(steps[2].args, ["--config", "/work/setup.json", "install"]);
        assert_eq!(
            steps[3].program,
            "/home/u/Library/Application Support/RWS/bin/rws"
        );
        assert_eq!(steps[3].args, ["autostart", "install"]);
        assert_eq!(steps[4].program, "launchctl");
        assert_eq!(
            steps[4].args,
            [
                "bootstrap",
                "gui/501",
                "/home/u/Library/LaunchAgents/io.rws.mounts.plist"
            ]
        );
        assert!(
            steps[4].optional,
            "an already-loaded agent must not fail the plan"
        );
        assert_eq!(
            steps[5].program,
            "/home/u/Library/Application Support/RWS/bin/rws"
        );
        assert_eq!(
            steps[5].args,
            [
                "--config",
                "/home/u/Library/Application Support/RWS/config.json",
                "connect",
                "demo"
            ]
        );
    }

    #[test]
    fn execution_stops_at_the_first_failed_required_step_and_tolerates_optional_ones() {
        let step = |id: &str, script: &str, optional: bool| Step {
            id: id.into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            optional,
            summary: format!("run {id}"),
        };
        let (results, ok) = execute(&[
            step("one", "echo first", false),
            step("two", "echo warned >&2; exit 5", true),
            step("three", "echo third", false),
        ]);
        assert!(ok);
        let statuses: Vec<_> = results.iter().map(|r| r.status.as_str()).collect();
        assert_eq!(statuses, ["done", "warning", "done"]);
        assert!(results[0].output.contains("first") && results[1].output.contains("warned"));
        let (results, ok) = execute(&[
            step("one", "exit 3", false),
            step("two", "echo never", false),
        ]);
        assert!(!ok);
        assert_eq!(results.len(), 1, "nothing may run after a required failure");
        assert_eq!(results[0].status, "failed");
    }
}
