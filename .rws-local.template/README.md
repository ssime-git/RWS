# Local configuration template

This directory is versioned. `.rws-local/` is private and ignored by Git.
The sample host and SSHFS path are placeholders, not working defaults.

From the repository root, copy the template only on a new setup:

```sh
if [ -e .rws-local ]; then
  echo 'Existing .rws-local preserved; adapt its configuration manually.'
else
  (umask 077; cp -R .rws-local.template .rws-local)
fi
```

Edit `.rws-local/config.json`: set the SSH destination, remote directory, mount
name/path and absolute path of the patched SSHFS executable. FSKit requires a
direct child of `/Volumes`. Follow [macOS setup](../docs/prototype.md) and the
[SSHFS build guide](../docs/sshfs-fskit.md) for prerequisites; copying this template
does not install or activate them. Configure authentication in your own SSH
configuration, never in this template.

Build the CLI and install it at a stable local path:

```sh
cargo build --locked
mkdir -p .rws-local/bin
cp target/debug/rws .rws-local/bin/rws
```

Then, from the repository root:

```sh
"$PWD/.rws-local/bin/rws" --config "$PWD/.rws-local/config.json" connect documents
"$PWD/.rws-local/bin/rws" --config "$PWD/.rws-local/config.json" status documents
"$PWD/.rws-local/bin/rws" --config "$PWD/.rws-local/config.json" shortcuts documents --directory "$PWD/.rws-local/shortcuts"
```

Use the configured workspace name if you changed `documents`. Shortcuts refuse
to overwrite existing files: choose a new directory when regenerating them.
For Delta, install the global rule with `delta-rules` using the same absolute
binary/config paths, then follow the [checkout procedure](../docs/connection.md#choose-a-checkout-inside-the-mount).

Generate binaries, shortcuts, mount receipts and logs on each machine; do not
copy them into this template. A receipt must verify the current mount, and
shortcuts/rules contain machine-specific absolute paths. Keep diagnostics and
screenshot originals only in the ignored local directory.
