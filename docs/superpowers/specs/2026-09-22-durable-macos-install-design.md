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
  releases/<release-id>/sshfs/sshfs
```

`config.json` is the only mutable workspace registry. Its matching
`config.json.mount-state/` directory is the verification state for that registry
and migrates with it. `bin/rws` and the selected SSHFS executable are copies
managed by RWS. SSHFS is placed in an immutable release directory; the config
records that release's absolute SSHFS path.
Directories are mode 0700 and configuration, backups, receipts and logs are
mode 0600; executables are mode 0700.

## Commands and migration

`rws [--config SOURCE] install` is macOS-only and requires an existing,
valid source configuration. It validates the source, resolves the *effective*
SSHFS executable, copies the running executable and SSHFS executable into a new
immutable release directory, validates both staged executables, then writes a
copied configuration that references the final immutable release path. It copies
matching mount receipts into the staged configuration state. It must not modify
or delete the source configuration.

For the installed binary, the canonical configured SSHFS path wins over an
inherited `RWS_SSHFS`; an override that points elsewhere is rejected with a
clear error rather than silently restoring a checkout dependency. The custom
SSHFS executable remains dependent on installed macFUSE and GLib libraries;
their paths are prerequisites, while the RWS-managed copy retains its source
provenance and license alongside the executable.

The command stages and validates the release, atomically replaces `bin/rws`,
then atomically replaces `config.json` and its receipt directory as the final
commit step. The config replacement is the single activation point: it never
references an incomplete release, and every referenced release is retained. A
crash before config replacement leaves the prior config and release active; a
crash after it leaves a fully staged release selected. It keeps the immediately
previous configuration, receipt state, and referenced release as bounded
RWS-owned backups. A missing source config, non-regular executable, or failing
copy leaves the canonical configuration untouched.

## Login remounting

`rws autostart install` writes exactly one LaunchAgent:

```
~/Library/LaunchAgents/io.rws.mounts.plist
```

Its only RWS program argument is the managed binary followed by `autostart run`.
It has no workspace name or configuration argument. At login, `autostart run`
uses `load_existing` for the canonical configuration and attempts every
registered workspace with its configured mount backend. It succeeds only when
all workspaces are mounted or already verified; otherwise it attempts the
remaining workspaces, returns nonzero, and Launchd retries after 30 seconds.
Missing or corrupt canonical state is a nonzero error, never an empty success.
This makes later workspace changes effective without regenerating the plist.

During installation, RWS removes only legacy plist files matching its exact
owned label/path pattern `io.rws.mount.<validated-workspace>.plist`; it first
asks `launchctl bootout` to unload each one and warns if unloading is unavailable.
It never deletes other LaunchAgents. This cleanup occurs only after the generic
agent is written and validated.

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
path rewriting, source-config and matching-receipt preservation, staged failure
rollback, executable mode validation, override rejection, a generic plist with
no workspace/config path, legacy-agent ownership cleanup, and retry semantics.
CLI tests cover missing source configuration, `install`, and aggregated
`autostart run` failures with temporary directories and a fake mount command
boundary; they do not claim live macFUSE mounts.
