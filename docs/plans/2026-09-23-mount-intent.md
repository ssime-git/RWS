# Automatic mount intent implementation plan

> **For agentic workers:** Use test-driven-development and independent review. The approved contract is in docs/connection.md, Automatic remount contract.

**Goal:** Persist Connect/Disconnect intent and reconcile eligible mounts without overriding deliberate pauses.

**Architecture:** Keep intent in the canonical user configuration, separate from identity receipts. A launchd periodic one-shot runner reloads configuration on each pass and leaves healthy mounts untouched. Manual and background operations share workspace serialization; automatic work must re-read intent after acquiring the operation lock. No global FSKit restart or automatic forced ejection.

**Tech Stack:** Rust, serde, launchd plist, Swift Codable, Cargo and macOS CI.

## Implementation record

The state/reconciliation and compatibility/documentation steps below are
implemented. Specification and independent quality reviews were performed.
The loaded-service diagnostic now checks launchd read-only instead of trusting
the plist. Activation remains at next login (no silent reload or enablement).
An additional deterministic fork-inheritance regression covers explicit advisory
unlocking. See docs/validation.md for evidence and remaining live acceptance;
reboot/network testing is not replaced by automated checks.

## Chunk 1: Persistent state and reconciliation

- [ ] Add failing config tests: legacy defaults connected, explicit paused round-trip, unknown workspace refused, updates preserve unrelated settings.
- [ ] Implement a per-workspace desired state in config with atomic writes. Legacy absent values preserve old login behavior; registration alone does not silently activate a disabled service.
- [ ] Add failing CLI/lifecycle tests: Connect persists intent before transient mount failure; Disconnect persists pause before busy unmount; dry-run makes no writes; a retry cannot override a newer pause.
- [ ] Exercise both lock orderings with deterministic synchronization, the `unmount` alias, preservation of another workspace's state, failure followed by a successful retry, and healthy verified mounts left intact. A rejected concurrent Disconnect explicitly says its pause was not saved and must be retried.
- [ ] Implement state transitions under shared operation serialization and automatic intent re-read. Do not rely on stale configuration captured before locking. Reject lock contention visibly without claiming the operation succeeded.
- [ ] Add failing autostart tests: paused workspace skipped, recurring scheduling after success, recognized old plist migration, custom/disabled plist preservation.
- [ ] Implement periodic launchd passes with throttled failure retries. Background work must not call the explicit Connect intent setter. Unhealthy verified volumes report actionable repair errors; unknown volumes are untouched.
- [ ] Run `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`; explicitly record any sandbox-only failure.

## Chunk 2: App compatibility and documentation

- [ ] Add Swift Codable round-trip test for desired state; implement decoding/encoding preservation and strict shape validation.
- [ ] Document Finder/external eject as loss of actual mount, not an RWS Disconnect: eligible mounts may return. Document legacy migration, fixed periodic retry interval, safe recovery limits, and busy disconnect behavior.
- [ ] Document activation separately from on-disk installation; do not pretend writing a plist reloads launchd. Preserve disabled/custom services. Add explicit bounded activation if safe and covered by tests; otherwise surface required activation without claiming deployment.
- [ ] Review spec compliance then code quality, fix findings, run relevant suites and diff checks. No live fault injection on user data. Real reboot/network acceptance remains separate from unit tests.
