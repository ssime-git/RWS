# Experimental FSKit-compatible SSHFS

On the tested macOS 27.0 / macFUSE 5.4.0 combination, official SSHFS 3.7.5 advertises an extended rename capability it does not support. RWS can use a separately built copy with that capability disabled. See [the diagnosis](fskit-debugging.md).

## Build

Requires macOS Command Line Tools, installed macFUSE headers/libraries, Python 3, and GLib/GThread development files. If GLib is absent, install it with `brew install glib`. The script does not install packages or replace `/usr/local/bin/sshfs`.

```sh
./scripts/build-sshfs-fskit.sh
```

The script downloads six files from official SSHFS commit `9e35c39ba83f54a49a9df4bf0a629f26c60cc38c` (3.7.5), verifies individual SHA-256 values, applies one guarded change to capability negotiation, and compiles into a unique `.rws-local/sshfs/build.XXXXXX` directory. It retains the modified source and upstream GPL notice beside the executable. This is a reproducible source-and-patch procedure, not a guarantee of identical binaries across compiler/library versions. Build dependencies remain dynamically linked; rebuilding may be necessary after their removal or upgrade.

The only patch clears `FUSE_DARWIN_CAP_RENAME_EXT` in `sshfs_init`. It does not silently ignore flags supplied to the rename callback and does not emulate rename using copy/delete. The resulting binary identifies as `3.7.5-rws-fskit1`; it is experimental, not an official SSHFS release.

The script prints an exact `export RWS_SSHFS=...` command. Run it in the shell used for RWS, then:

```sh
./target/debug/rws --config .rws-local/setup.json doctor --workspace setup-test
./target/debug/rws --config .rws-local/setup.json mount setup-test --fskit
./target/debug/rws --config .rws-local/setup.json unmount setup-test
```

Use the registered workspace/configuration for your machine. `RWS_SSHFS` selects one executable path, not shell arguments, and also affects `doctor`. Unset it to use the standard `sshfs` on PATH. For GLib installed elsewhere, set `RWS_GLIB_PREFIX` to its installation prefix before building.

## Mount lifecycle

RWS runs SSHFS with `-f` in a separate process group, redirects diagnostics to an exclusively created mode-0600 log beside the configuration (`mount-logs/`), and waits up to 30 seconds for a filesystem device boundary while checking the child is alive. After readiness, RWS returns and SSHFS continues. Normal `rws unmount` delegates to the OS; SSHFS then exits. Logs are retained for diagnosis and may contain private host/path details.

Mounting uses SSH `BatchMode=yes` with closed stdin. Prepare authentication (for example, an unlocked SSH agent) and known-host trust through a normal SSH connection beforehand. RWS does not prompt for credentials in the background. Early exit and timeout return an error pointing to the log and stop/reap the startup process group, including SSH descendants. Process cleanup uses a still-reserved group-leader PID, not a stored PID file. Readiness metadata and forced process termination still depend on OS responsiveness.

Validated: live RWS mount returns, read/write/fsync, rename, replacing an existing file, no-replace refusing an existing target, independent remote content checks, and normal unmount with SSHFS exit. This does not yet establish all editor save patterns, concurrent modifications, or recovery from network loss. Test on disposable data first.
