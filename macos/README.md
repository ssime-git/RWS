# RWS for macOS

This SwiftUI executable is the native front end for the RWS Rust CLI. The app
never constructs shell commands: each user value is passed as a separate
process argument to the bundled executable at
`RWS.app/Contents/Resources/bin/rws`.

## Build and test

The package targets macOS 13 or later and pins Sparkle 2.10.0 exactly.

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  CLANG_MODULE_CACHE_PATH=/tmp/rws-clang-cache \
  SWIFTPM_MODULECACHE_OVERRIDE=/tmp/rws-swiftpm-module-cache \
  swift test --package-path macos

DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  CLANG_MODULE_CACHE_PATH=/tmp/rws-clang-cache \
  SWIFTPM_MODULECACHE_OVERRIDE=/tmp/rws-swiftpm-module-cache \
  swift build -c release --package-path macos --product RWSApp
```

The bundle builder must obtain the release directory from
`swift build -c release --show-bin-path --package-path macos`, copy its
`RWSApp` to `Contents/MacOS/RWSApp`, copy the Rust binary to
`Contents/Resources/bin/rws`, and copy the resolved Sparkle framework into
`Contents/Frameworks`.

## Local state and prerequisites

The app uses `~/Library/Application Support/RWS/config.json`. Its import picker
first validates the selected file through the bundled CLI, then copies it only when that destination does
not exist. It never replaces an existing configuration or copies mount
receipts.

Mounting currently needs macFUSE plus the external patched RWS SSHFS build.
Choose its absolute path under **Mac Setup**. These prerequisites are not
installed or modified by the app.

The optional Delta action adds rules that tell Delta agents to forward explicit
commands through RWS. Delta, Finder, editors, Git integrations, and other native
processes still execute locally unless they explicitly use that forwarding.

## Updates

Sparkle starts only when the production bundle has all three values:

- `RWSUpdatesEnabled` is true;
- `SUPublicEDKey` is nonempty and is not a placeholder;
- `SUFeedURL` is HTTPS.

Development bundles therefore do not contact an update feed. Automatic update
checks remain opt-in. Automatic downloads are disabled (`SUAllowsAutomaticUpdates`
must be false in the bundle) so an ordinary Quit can never install a staged
update while a mount is active. Before an explicit update relaunch,
or quitting for an update, the app reruns `rws status --no-probe`; installation
is postponed when an operation or registered mount is active, status is
uncertain, or status output was truncated. The Check for Updates action retries
a postponed install after a safe status refresh. Ordinary Quit remains available
while a volume is mounted and never unmounts it.
