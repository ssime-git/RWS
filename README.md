# RWS — Remote Workspace System

RWS aims to make remote development workspaces accessible from macOS while keeping files and command execution on the machine that owns each workspace.

**Status: early Rust CLI prototype. Remote execution and a real FSKit mount/read/write/rename/unmount cycle have passed on a tested Mac and Linux SSH host. FSKit currently requires the experimental SSHFS build described below; broad editor and recovery validation remains pending.**

## Product scope and missing features

The central goal is transparent remote execution when working in a mounted remote
project, including Delta without special agent instructions where technically
feasible. **That goal is not implemented yet.** Mounting files, explicit SSH
launchers and Delta routing rules are partial solutions, not equivalent coverage.
See the [linked feature backlog and acceptance criteria](FEATURES.md) and
[delivery order](ROADMAP.md).

## Try it

See the [prototype guide](docs/prototype.md) for build commands, workspace registration, remote execution, mounting prerequisites, and current limitations.

For saved connection settings, double-click shortcuts, mount status, and the
distinction between remote files and Delta's local commands, see
[connect and execute on the VM](docs/connection.md).

Start a new machine's private configuration from
[`.rws-local.template/`](.rws-local.template/README.md). Adapt the example paths;
keep the resulting `.rws-local/` directory out of Git.

```sh
cargo build --locked
./target/debug/rws --help
```

For the native macOS frontend, development builds and signed automatic release
setup, see [the app and release guide](docs/macos-app.md).

## First prototype

Validate one complete workflow: connect to an existing SSH host, mount a remote directory on macOS, edit a file locally, and run a command or interactive shell in the corresponding remote directory.

The remote machine remains the source of truth. The prototype will use existing SSH and filesystem tools before considering a custom filesystem.

## Project documents

- [Decisions and prototype scope](docs/decisions.md)
- [Roadmap](ROADMAP.md)
- [macOS Finder configuration and troubleshooting](docs/finder-macos.md)
- [Experimental FSKit-compatible SSHFS](docs/sshfs-fskit.md)
- [Validation results and remaining checks](docs/validation.md)
- [Agent skill: set up RWS on a new Mac](.agents/skills/rws-macos-setup/SKILL.md)

## Open-source preparation

The repository is being organized for public development from the start. Documentation and examples must use fictitious hostnames and paths, and must not contain credentials or personal SSH configuration.

A license must be selected before the project is presented as licensed open-source software. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and checks.
