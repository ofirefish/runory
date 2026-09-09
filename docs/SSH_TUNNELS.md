# SSH tunnels — local TCP forwarding v1

User-authorized scope: a peer of Servers and Sessions in the primary navigation.
This is not a shell command launcher, SOCKS proxy, reverse tunnel, jump-host
implementation, public sharing service, or Agent tool.

## Interaction

- Full-width searchable list, SSH-profile and runtime-state filters, selected-rule
  inspector. Server details and the server menu can prefill a new rule.
- Rule: name, saved SSH profile ID, destination host/port, local port. Rust binds
  IPv4 loopback only (`127.0.0.1`); the bind address is not an IPC parameter.
- Save and save/start are separate actions. Start stays on the Tunnels page and
  uses a dedicated background SSH connection, never a terminal tab or PTY.
  If no matching background connection exists, the existing credential/vault and
  fingerprint dialog opens in place. Authentication completes the explicit start.
  Existing terminal tabs are neither required nor used for forwarding.
- A listener being active is not a health claim. An explicit TCP probe opens a
  direct-tcpip channel to the exact saved destination, through the same bound SSH
  transport. It does not claim application health or authentication success.
- Remote DNS/refusal may share SSH's ConnectFailed reason. Never invent a more
  specific diagnosis from untrusted server text.
- Switching pages and closing terminals preserve tunnels. Stopping terminates
  that rule's listener and streams. Rules sharing an unchanged SSH profile reuse
  one background transport; the last stopped rule releases it. SSH loss interrupts
  forwarding without reconnecting. Application restart restores rules as stopped.
- Editing/deleting running rules is rejected; stop first. No silent port changes.
- The inspector route animates recent real byte-count increases: downward for
  sent bytes and upward for received bytes. It uses the existing 2-second metadata
  poll, not per-packet telemetry; activity expires after 2.4 seconds without growth.
  Initial historical counts, counter resets, rule/run changes and stopped states
  do not animate. Reduced-motion preferences retain only static highlighting.

## Engine and safety

React renders metadata and invokes business-specific commands. Rust owns all TCP,
SSH channels, validation, health checks, bounded tasks, and payload-free counters.
TunnelRuleRepository wraps the existing atomic JsonRepository, with schema and
identifier/duplicate validation; corrupt data is preserved and fails closed.
If a profile is deleted, its rules remain editable but cannot start until the
user selects an existing profile; profile existence is rechecked at start/probe.
The background transport reuses native authentication and a crate-internal
direct-tcpip capability. Connection preparation is shared with terminal commands;
no duplicate vault or host verification implementation, shell IPC, broad capability,
dependency, terminal channel or terminal output persistence is introduced.
Start/probe use existing Connect and Operate cloud policies; stop is always allowed.
Pool identity binds the profile ID and all SSH connection fields, not a terminal
tab. Weak pooling leaves ownership with running forwarding tasks. Stop cancels
pending authentication; a 30-second deadline bounds both pool waiting and opening.
No separate persistent authorization or automatic reconnect mechanism is introduced.
Limits: 100 saved rules, 16 listeners, 64 concurrent connections per listener,
10-second channel-open timeout, 16 metadata events per run. No payload logging,
inspection, persistence, or delivery to the frontend/Agent.

The destination is resolved/reached by the SSH server, not by the local machine.
SSH encryption ends at that server; application TLS remains necessary where used.
HTTP launchers, service templates, bulk operations, transfer charts,
automatic reconnect/start and non-loopback sharing are deferred.

## Verification

Frontend: form constraints, in-place background authentication, filters, state versus health,
navigation preservation, error localization and bilingual completeness.
Rust: corrupt storage, rule validation, lifecycle and exact binding; real Docker
OpenSSH tests for payload forwarding, TCP probing, parallel streams, stop/restart,
session disconnect and host-key/auth regression using the existing fixtures.

Initial terminal-bound baseline validated on Windows, 2026-09-04 (before the
background-connection increment):

- `pnpm test`: 185 tests passed across 33 files, including navigation and i18n.
- `pnpm typecheck`, `pnpm lint`, `pnpm build`: passed.
- `cargo test --lib -- --test-threads=4`: 328 passed, 18 opt-in tests ignored.
- `cargo test --lib tunnels::tests -- --include-ignored --test-threads=1`:
  all 6 passed, including both real-network tunnel scenarios.
- Existing `ssh::service::integration_tests`: 13 passed with the unrelated
  three-node production fault lab excluded. Host-key rotation/blocked changes,
  passwords, plain/encrypted keys, wrong credentials, PTY, resize and SFTP passed.
- `cargo clippy --lib`: passed with existing warnings outside the new tunnel code.
  New/modified Rust implementation files pass rustfmt checks.

Background-connection increment (Windows, 2026-09-04):

- Frontend full suite: 194 passed, including 16 tunnel-page tests.
- Typecheck, lint and Vite production build passed.
- Final `cargo clippy --lib` passed with 114 existing warnings outside the changed
  forwarding/authentication files; touched Rust files pass rustfmt checks.
- Rust full unit suite: 331 passed, 20 opt-in scenarios ignored.
- Existing real OpenSSH regression: all 13 passed, including network interruption,
  changed host keys, password/private-key/encrypted-key authentication and SFTP.
  The container-restart scenario was rerun with Docker access after the sandbox
  correctly denied its initial attempt.
- All 11 tunnel Rust tests passed, including four real OpenSSH scenarios.
  The new forwarding-only fixture rejects session channels with `MaxSessions=0`;
  forwarding still works, shares a transport across rules, outlives terminal closure,
  and releases the last owner after stop or failed binding.
- Unknown/changed fingerprint, stored and transient credentials, cancelled starts,
  profile mismatch and no-workspace navigation are covered by the relevant UI/Rust tests.

No macOS/Linux/mobile build or native-window visual acceptance is claimed by this
Windows run. Mobile background suspension does not guarantee a persistent tunnel.
The added Rust IPC requires restarting/rebuilding a running development app.
