/// OpenSSH sends a command string to the remote login shell. Quote each literal
/// argument for a POSIX shell; local argument arrays alone do not protect it.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Expand SHELL on the remote host, keeping its executable path a single word.
pub fn remote_shell(directory: &str) -> Result<String, String> {
    if !directory.starts_with('/') || directory.contains('\0') {
        return Err("remote directory must be absolute and contain no NUL".into());
    }
    Ok(format!(
        "cd {} && exec \"${{SHELL:-/bin/sh}}\" -l",
        quote(directory)
    ))
}

pub fn remote_command(directory: &str, argv: &[String]) -> Result<String, String> {
    if !directory.starts_with('/') || directory.contains('\0') {
        return Err("remote directory must be absolute and contain no NUL".into());
    }
    if argv.is_empty()
        || argv[0].is_empty()
        || argv[0].starts_with('-')
        || argv.iter().any(|v| v.contains('\0'))
    {
        return Err("provide a command after --; arguments must contain no NUL".into());
    }
    Ok(format!(
        "cd {} && exec {}",
        quote(directory),
        argv.iter().map(|v| quote(v)).collect::<Vec<_>>().join(" ")
    ))
}
