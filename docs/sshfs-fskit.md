# Experimental FSKit-compatible SSHFS

On the tested macOS 27.0 / macFUSE 5.4.0 combination, official SSHFS 3.7.5 advertises an extended rename capability it does not support. RWS can use a separately built copy with that capability disabled. See [the diagnosis](fskit-debugging.md).

## Build

Requires macOS Command Line Tools, installed macFUSE headers/libraries, Python 3, and GLib/GThread development files. If GLib is absent, install it with `brew install glib`. The script does not install packages or replace `/usr/local/bin/sshfs`.

```sh
./scripts/build-sshfs-fskit.sh
```

The script downloads six files from official SSHFS commit `9e35c39ba83f54a49a9df4bf0a629f26c60cc38c` (3.7.5), verifies individual SHA-256 values, applies guarded capability and filename-conversion patches, and compiles into a unique `.rws-local/sshfs/build.XXXXXX` directory. It retains the modified source and upstream GPL notice beside the executable. This is a reproducible source-and-patch procedure, not a guarantee of identical binaries across compiler/library versions. Build dependencies remain dynamically linked; rebuilding may be necessary after their removal or upgrade.

The rename patch clears `FUSE_DARWIN_CAP_RENAME_EXT` in `sshfs_init`. It does not silently ignore flags supplied to the rename callback and does not emulate rename using copy/delete. The resulting binary identifies as `3.7.5-rws-fskit3`; it is experimental, not an official SSHFS release.

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

## Unicode filenames

FSKit mounts now enable the patched binary's `rws_unicode` option. GLib converts local child paths to NFC for SFTP and directory entries/link targets to NFD for macOS. The configured remote root is preserved verbatim. This addresses accented filenames that previously produced EEXIST/ENOENT and disappeared from Finder. Accents, Japanese and emoji passed live create/read/rename/replace tests. macOS UTF-8-MAC iconv was rejected after a supplementary-plane emoji failed a round trip; this build uses GLib instead.

**Contract:** existing remote child filenames must be valid UTF-8 in NFC form. Existing NFD names, invalid UTF-8, or directories containing canonically equivalent names are not supported by this conversion. No existing server names are renamed automatically. `mount WORKSPACE --fskit --raw-names` disables conversion for intentional raw access, but does not fix Finder's Unicode limitations. The standard system SSHFS is rejected before mounting in conversion mode; rebuild and select the printed `RWS_SSHFS` path.

SSHFS directory/attribute caching and FUSE attribute/entry/negative timeouts are disabled in conversion mode because byte-keyed caches retained stale metadata between NFC/NFD aliases. Bidirectional write/stat/unlink checks pass with this correction. This increases metadata requests; performance and concurrent remote-edit behavior remain unvalidated.

## Finder sidebar

Volumes are named `RWS-WORKSPACE`. A successful mount does not guarantee an entry under Finder's Locations. On the tested Mac, enabling Hard disks and External disks (with user approval) was insufficient; Connected servers was already enabled. Finder's Computer view showed a browsable remote volume throughout.

For the current mount, open Finder > Go > Computer, select the RWS volume, then File > Add to Sidebar. This produced a Locations entry with an eject button. After an unmount/remount the entry disappeared in a new Finder window; repeat this action if necessary. Automatic sidebar persistence remains unresolved. RWS does not change Finder preferences or represent the remote filesystem as a local disk.

## Repeated directory enumeration

The first Unicode build disabled SSHFS's cache wrapper but reused an exhausted SFTP directory cursor on subsequent reads. On the tested FSKit stack, the root handle persists: the first listing succeeded, later listings were empty, while mkdir still created directories on the server. Version `3.7.5-rws-fskit3` opens and closes a private SFTP handle per Unicode-mode enumeration and disables FUSE `nullpath_ok` because reopening requires a path. The original directory handle remains untouched. This preserves cache invalidation without returning EOF for every subsequent root listing. See the [FUSE callback contract](https://libfuse.github.io/doxygen/structfuse__operations.html).

Run `python3 scripts/test-mount-listing.py /Volumes/RWS-test` on an authorized mounted root. It creates a unique disposable directory, repeatedly checks root and child listings during Unicode file creation/deletion, and cleans up after success. On failure it retains its test directory for diagnosis. This is a live macFUSE acceptance check, separate from Cargo tests.

After remounting, a stale sidebar entry may also report that its original item cannot be found. Remove that entry using its context menu, then add the current volume from Computer again. Verify by clicking the sidebar entry and creating a folder, not just by observing the icon.
