# RWS — Remote Workspace System

RWS aims to make remote development workspaces accessible from macOS while keeping files and command execution on the machine that owns each workspace.

**Status: early Rust CLI prototype. Remote execution has been exercised on a Linux SSH host; Finder mounting is awaiting local dependency installation and end-to-end validation.**

## Try it

See the [prototype guide](docs/prototype.md) for build commands, workspace registration, remote execution, mounting prerequisites, and current limitations.

```sh
cargo build --locked
./target/debug/rws --help
```

## First prototype

Validate one complete workflow: connect to an existing SSH host, mount a remote directory on macOS, edit a file locally, and run a command or interactive shell in the corresponding remote directory.

The remote machine remains the source of truth. The prototype will use existing SSH and filesystem tools before considering a custom filesystem.

## Project documents

- [Decisions and prototype scope](docs/decisions.md)
- [Roadmap](ROADMAP.md)
- [Validation results and remaining checks](docs/validation.md)

## Open-source preparation

The repository is being organized for public development from the start. Documentation and examples must use fictitious hostnames and paths, and must not contain credentials or personal SSH configuration.

A license must be selected before the project is presented as licensed open-source software. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and checks.
