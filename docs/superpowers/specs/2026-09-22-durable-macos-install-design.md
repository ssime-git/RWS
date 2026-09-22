# Durable macOS installation and remount design

## Goal

Make a configured RWS installation survive deletion or movement of its source
checkout. Login remounting, zsh integration, and Delta rules must use one
canonical user configuration and never refer to `target/debug` or
`.rws-local`.

## Canonical macOS layout

RWS uses the existing macOS default configuration location:

```
~/Library/Application Support/RWS/
  config.json
  bin/rws
  libexec/sshfs/sshfs
```

`config.json` is the only mutable workspace registry. `bin/rws` and the
selected SSHFS executable are copies managed by RWS; the config records the
absolute path to the managed SSHFS copy. Files are installed through temporary
files plus rename and use owner-only permissions where applicable.

## Commands and migration

`rws install` receives the currently selected configuration (normally supplied
with `--config` during the one-time migration). It validates that configuration,
copies the running executable and its selected SSHFS executable into the
canonical layout, rewrites the copied configuration to use the managed SSHFS
path, then atomically installs it as the canonical configuration. It must not
modify or delete the source configuration.

The command is idempotent: repeating it replaces only RWS-managed artifacts and
the canonical configuration. Failure before the final rename leaves the previous
canonical configuration usable.

## Login remounting

`rws autostart install` writes exactly one LaunchAgent:

```
~/Library/LaunchAgents/io.rws.mounts.plist
```

Its only RWS program argument is the managed binary followed by `autostart run`.
It has no workspace name or configuration argument. At login, `autostart run`
loads the canonical configuration and attempts every registered workspace with
its configured mount backend. It succeeds only when all workspaces are mounted
or already verified; otherwise its nonzero exit causes Launchd to retry after 30
seconds. This makes later workspace changes effective without regenerating the
plist.

## Integrations

After a successful install, `hook install` and `delta-rules` use the managed
binary and the canonical default configuration. Existing integrations pointing to
a checkout remain functional until refreshed; the install command reports the
explicit refresh commands instead of silently editing shell or Delta files.

## Failure handling and safety

- A missing source executable, selected SSHFS executable, or invalid source
  configuration fails before changing the canonical configuration.
- Existing configuration is backed up before replacement, with a bounded
  RWS-managed backup name in the canonical directory.
- A LaunchAgent does not prompt for macOS privileges. Initial macFUSE helper
  authorization remains a GUI-terminal task.
- `autostart run` does not treat an unknown or unverified mount as success.

## Tests

Automated tests cover canonical path selection, atomic migration output, SSHFS
path rewriting, preservation of the source config, a generic plist with no
workspace/config path, and retry semantics. CLI tests cover `install` and
`autostart run` with temporary directories and a fake mount command boundary;
they do not claim live macFUSE mounts.
