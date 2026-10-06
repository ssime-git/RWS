# NFS bridge automatic recovery, 2026-10-06

## Failure being addressed

The macOS NFS client can retain a verified RWS mount after its per-workspace
`rws nfs-bridge` process exits. SSH may already work again, but the kernel still
sends requests to the vanished localhost server. The 30-second LaunchAgent
currently reports the unhealthy mount forever and never starts another bridge.

## Recovery policy

Only an automatic, desired NFS connection may recover itself. RWS checks that
the mounted filesystem still matches its durable ownership receipt and that
the path is unhealthy. If the matching bridge is alive, RWS leaves it running:
its SFTP transport already reconnects after a network interruption. If the
bridge is absent, RWS records the mount identity and first observation in
private state. A later automatic pass, at least 30 seconds afterward, must
again find the same verified, unhealthy mount with no matching bridge and must
complete a bounded noninteractive SSH probe. Immediately before recovery it
rechecks the identity and bridge absence. Only then it force-unmounts that one
volume, removes its old receipt and mounts it again through the normal
attestation path. Failure leaves the intent connected for the next pass.

Healthy mounts, live bridges, paused intent, SSH outages, foreign mounts,
identity changes, and FSKit mounts never trigger automatic force-unmount.
Manual `--repair` retains its existing behavior. The marker is cleared after
health returns or a successful remount. The NFS mount is `soft` and the dead
bridge cannot acknowledge outstanding writes; recovery cannot restore writes
already failed by the interruption, so this residual risk must be documented.

## Validation

Cover bridge matching with installed executable paths containing spaces,
first and second failure observations, identity changes, healthy/live-bridge
and unavailable-SSH exclusions, and per-workspace isolation. Run the Rust
suite on macOS and Linux CI. In an isolated disposable workspace, kill only
its bridge, confirm the first automatic pass preserves the stale mount and the
second pass remounts after SSH is reachable. Do not install this prototype into
the active configuration until that isolated recovery is verified.
