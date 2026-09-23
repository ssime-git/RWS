//! Conservative, read-only verification of the loaded GUI LaunchAgent.
use std::{path::Path, process::Command, time::Duration};

pub fn verify(binary: &Path, plist: &Path) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err(
            "on-disk agent exists; loaded launchd state is unavailable on this platform".into(),
        );
    }
    let domain = format!("gui/{}", unsafe { libc::geteuid() });
    let mut disabled = Command::new("/bin/launchctl");
    disabled.args(["print-disabled", &domain]);
    let disabled = rws::process::output(&mut disabled, Duration::from_secs(5))?;
    if !disabled.status.success() {
        return Err(
            "cannot verify whether the LaunchAgent is disabled; no service changes made".into(),
        );
    }
    let mut command = Command::new("/bin/launchctl");
    command.args(["print", &format!("{domain}/io.rws.mounts")]);
    let loaded = rws::process::output(&mut command, Duration::from_secs(5))?;
    if !loaded.status.success() {
        return Err("on-disk agent is current but the loaded service cannot be verified; activate it at login or inspect launchctl; no service changes made".into());
    }
    verify_output(
        &String::from_utf8_lossy(&loaded.stdout),
        &String::from_utf8_lossy(&disabled.stdout),
        binary,
        plist,
    )
}

fn verify_output(
    loaded: &str,
    disabled: &str,
    binary: &Path,
    plist: &Path,
) -> Result<String, String> {
    // launchctl's human-readable output is not a stable API. Fail closed on
    // unfamiliar formatting instead of claiming that a plist proves activation.
    let disabled = disabled.trim();
    let entries = disabled
        .strip_prefix("disabled services = {")
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("cannot recognize launchd disabled-service output; activation not verified")?;
    for entry in entries
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (label, value) = entry
            .split_once(" => ")
            .ok_or("cannot recognize launchd disabled-service entry; activation not verified")?;
        if label == "\"io.rws.mounts\"" {
            match value.trim_end_matches(';') {
                "true" | "disabled" => {
                    return Err(
                        "LaunchAgent is explicitly disabled; preserved without enabling it".into(),
                    );
                }
                "false" | "enabled" => (),
                _ => return Err("unknown launchd disabled state; activation not verified".into()),
            }
        }
    }
    let args = loaded
        .split_once("arguments = {")
        .and_then(|(_, tail)| tail.split_once('}'))
        .map(|(args, _)| {
            args.lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        });
    let binary_text = binary.to_string_lossy();
    let has_line = |expected: &str| loaded.lines().any(|line| line.trim() == expected);
    if !has_line(&format!("program = {}", binary.display()))
        || !has_line(&format!("path = {}", plist.display()))
        || !has_line("run interval = 30 seconds")
        || args.as_deref() != Some(&[binary_text.as_ref(), "autostart", "run"])
    {
        return Err("on-disk agent is current, but its loaded executable/path/30-second schedule is stale or unrecognized; log out and in to load the updated agent, or inspect launchctl; no service changes made".into());
    }
    Ok("current executable and 30-second schedule observed in launchd; this does not prove mount health".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_disabled_spelling_unknown_output_and_wrong_arguments() {
        let binary = Path::new("/app/bin/rws");
        let plist = Path::new("/user/io.rws.mounts.plist");
        let current = format!(
            "program = {}\npath = {}\nrun interval = 30 seconds\narguments = {{\n{}\nautostart\nrun\n}}\n",
            binary.display(),
            plist.display(),
            binary.display()
        );
        assert!(
            verify_output(
                &current,
                "disabled services = {\n\"io.rws.mounts\" => disabled\n}",
                binary,
                plist
            )
            .is_err()
        );
        assert!(verify_output(&current, "unrecognized output", binary, plist).is_err());
        assert!(
            verify_output(
                &current.replace("autostart", "doctor"),
                "disabled services = {\n}",
                binary,
                plist
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_old_schedule_wrong_installation_and_disabled_jobs() {
        let binary = Path::new("/app/bin/rws");
        let plist = Path::new("/user/Library/LaunchAgents/io.rws.mounts.plist");
        let current = format!(
            "\tprogram = {}\n\tpath = {}\n\trun interval = 30 seconds\narguments = {{\n{}\nautostart\nrun\n}}\n",
            binary.display(),
            plist.display(),
            binary.display()
        );
        assert!(verify_output(&current, "disabled services = {\n}", binary, plist).is_ok());
        assert!(
            verify_output(
                &current.replace("run interval = 30 seconds", "runs = 1"),
                "disabled services = {\n}",
                binary,
                plist
            )
            .is_err()
        );
        assert!(
            verify_output(
                &current,
                "disabled services = {\n}",
                Path::new("/other/rws"),
                plist
            )
            .is_err()
        );
        assert!(
            verify_output(
                &current,
                "disabled services = {\n\t\"io.rws.mounts\" => true\n}",
                binary,
                plist
            )
            .unwrap_err()
            .contains("disabled")
        );
    }
}
