# Build, test and run from source

[Documentation](README.md) · [App user guide](install.md) · [Architecture](architecture.md)

There are two independent builds: **RWS itself** and, for the currently tested
mount backend, **the patched SSHFS dependency**. You can compile RWS and run its
ordinary tests without a VM or macFUSE. Real mounting needs the additional setup.

## Prerequisites

| Tool | Requirement |
| --- | --- |
| Mac | Apple silicon for the current app bundle script |
| Apple developer tools | Working Swift compiler/SDK and Command Line Tools; Swift package tools version 5.9 or later |
| Rust | Current stable via [rustup](https://rust-lang.org/tools/install/), with Cargo and the arm64 macOS target |
| Python | Python 3 for packaging and release checks |
| Git + network | Clone and fetch Rust/Swift dependencies |

Use a compatible, selected Xcode toolchain from [Apple](https://developer.apple.com/xcode/resources/).
Check it before building:

```sh
uname -m
xcode-select -p
swift --version
rustc --version
cargo --version
python3 --version
```

If Command Line Tools are missing, `xcode-select --install` starts Apple's setup.
Complete the installer and any license prompts yourself. If installed tools have
a mismatched compiler/SDK, select a matching installation for that invocation,
for example `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` when that
full Xcode installation exists. Do not copy a machine-specific temporary Rust path.

The crate uses Rust edition 2024. No minimum supported Rust version is declared
or tested; using current stable matches CI. The macOS 13 deployment target of the
app is not the compatibility claim for macFUSE/FSKit.

## Clone and build

The current implementation branch is `prototype/cli`:

```sh
git clone --branch prototype/cli https://github.com/ssime-git/RWS.git
cd RWS
rustup target add aarch64-apple-darwin
scripts/build-macos-app.sh development
open dist/development/RWS.app
```

Or double-click `dist/development/RWS.app` in Finder. The script builds the Rust
release CLI, builds SwiftUI, includes the CLI and Sparkle framework, writes build
metadata and applies an ad-hoc signature. It produces:

```text
dist/development/
├── RWS.app
└── RWS-0.1.0-arm64-development.zip    # version comes from Cargo.toml
```

Running the raw Swift executable is not the documented app installation path:
the assembled bundle supplies the embedded CLI and framework. Opening the app is
not a mount test. Follow [first-use setup](install.md#3-configure-the-app-once)
for a real workspace.

## Rebuild without old app copies

Quit RWS, then repeat:

```sh
scripts/build-macos-app.sh development
open dist/development/RWS.app
```

The builder stages the new app before replacing the canonical path. The previous
bundle is kept in `dist/archives/` as ZIP, not as another launchable app. It refuses
to replace an app currently running from that path; a failed build/publication
preserves the previous output. Use the visible revision/date to identify a build.
Do not keep renaming old `.app` copies or repeatedly copy development builds into
Applications; that creates ambiguous launch targets. Source development should
use the canonical `dist/development/RWS.app` path.

## Test without a remote host

From the repository root:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
swift test --package-path macos
python3 -m unittest discover -s tests/release
python3 scripts/check-docs.py
```

Install `rustfmt` and `clippy` via `rustup component add rustfmt clippy` if missing.
Swift tests that modify Finder or use a real mount require explicit environment
opt-in and otherwise skip. Ordinary Rust tests use temporary fixtures and fake
SSH executables; a passing suite does not establish physical mount behavior.

To work on the CLI only:

```sh
cargo build --locked
./target/debug/rws --help
./target/debug/rws agent --help
```

The Rust core also has Linux CI. App and real macFUSE checks require macOS.

## Prepare mounting dependencies

Install macFUSE from its official source, including development headers, and
activate the intended backend. For the experimental FSKit path, install GLib
via [Homebrew](https://brew.sh/) if you use Homebrew, then build the dependency:

```sh
brew install glib
scripts/build-sshfs-fskit.sh
```

This script verifies pinned upstream source hashes, keeps modified source/license
files and prints the exact binary path plus an `export RWS_SSHFS=...` command.
Use that printed path; do not type the literal `build.XXXXXX` example. A non-Homebrew
GLib installation can be selected with `RWS_GLIB_PREFIX`.

The executable dynamically links its dependencies. Moving only the binary to
another Mac is not a complete installation. See [SSHFS details](sshfs-fskit.md).
In the app, choose the binary and save **FSKit** as described in the
[user guide](install.md#3-configure-the-app-once).

## Configure a disposable CLI workspace

Example host/path values must be replaced with your own. First prepare SSH
access and an existing disposable remote directory. After running the export
printed by the SSHFS build, use one configuration consistently:

```sh
cargo build --locked
./target/debug/rws --config .rws-local/setup.json settings \
  --sshfs "$RWS_SSHFS" --backend fskit
./target/debug/rws --config .rws-local/setup.json workspace add demo \
  --ssh devbox --remote /home/dev/rws-test --mount /Volumes/RWS-demo
./target/debug/rws --config .rws-local/setup.json doctor --workspace demo
./target/debug/rws --config .rws-local/setup.json connect demo
./target/debug/rws --config .rws-local/setup.json exec --workspace demo -- uname -s
./target/debug/rws --config .rws-local/setup.json agent --workspace demo -- codex --version
./target/debug/rws --config .rws-local/setup.json disconnect demo
```

Alternatively adapt [`.rws-local.template`](../.rws-local.template/README.md)
without overwriting an existing configuration. Keep `.rws-local/` ignored. The
app can select this exact `setup.json` through **Choisir une configuration…**;
auto-discovery searches `config.json`, not arbitrary filenames.

## Live acceptance is separate

Use a disposable remote directory and the [setup workflow](../.agents/skills/rws-macos-setup/SKILL.md).
Record exactly what you tested: SSH, mount/read/write/rename, Finder navigation,
editor saves, cwd/host of agent processes, normal unmount and cleanup. Do not run
network-loss or write-interruption tests on valuable projects.

The optional Swift lifecycle test **disconnects and reconnects the selected
workspace**. Read its [source](../macos/Tests/RWSAppTests/MountLifecycleLiveTests.swift)
before opting in. Passing it does not replace clicking the favorite in Finder.

## Release builds

Development build ≠ public release. Maintainers need signing/notarization and
Sparkle keys before running the [release procedure](macos-app.md#one-time-production-setup).
Do not create a release tag merely to test compilation.
