# Runory Local Vault and End-to-End Encrypted Cloud Sync

> Status: Architecture proposal. This document authorizes no production rollout by itself.
> Current Phase 9 remains the production baseline until each migration gate in this document is implemented and verified.

## 中文摘要

- 当前 Phase 9 继续只同步加密后的 Profile/Group 元数据；新方案未通过门禁前，密码和私钥仍禁止上传。
- Supabase 账号只负责身份认证，账号密码不作为 Vault 解密密钥。
- 每个 Personal/Organization Vault 使用随机 Vault Epoch Key；云端只保存设备公钥、密文和加密后的密钥信封。
- 每台设备拥有独立的 Ed25519 签名密钥与 X25519/HPKE 解密密钥，私钥由系统 Keychain/Keystore 或显式本地主密码保护。
- 新设备优先由现有可信设备通过二维码/短校验码批准；所有设备丢失时使用单独保存的 256-bit 恢复码。
- 每个对象版本使用新的随机 ODEK 与 AES-256-GCM 加密，并由设备签名；服务端只能执行权限、大小、Revision 和配额校验。
- 同步采用逐对象 Outbox、游标、签名变更链和 CAS，不再长期依赖整库覆盖式 Snapshot。
- SSH 密码、密钥口令和导入 Vault 的私钥分别显式选择是否同步；桌面文件路径及其私钥内容绝不自动迁移。
- 团队凭据使用独立 Collection Key 和逐设备信封；成员移除后阻止未来访问并轮换密钥，但不声称能收回其已看过的秘密。
- 实施按 M0–M5 门禁推进：密码学基础、本地 Vault 迁移、元数据 v3、个人凭据、设备恢复、团队 Collection；任一步失败都保留 Local-only 与原始本地数据。

## 1. Decision summary

Runory Cloud Sync SHALL remain optional and local-first. The cloud service SHALL authenticate users, enforce organization membership, store opaque encrypted objects, coordinate revisions, and retain content-free audit metadata. It SHALL NOT possess any key capable of decrypting SSH credentials, private keys, host metadata, or other synchronized content.

The design separates three concerns:

```text
Account authentication     Supabase Auth, MFA, short-lived session
Device trust               Device signing/encryption key pairs
Vault decryption           Random Vault Epoch Keys distributed in encrypted envelopes
```

The account password is not a Vault encryption key. Changing the account password does not require re-encrypting Vault data. A stolen cloud session can read and attempt to mutate ciphertext but cannot decrypt it or produce a valid mutation from an unregistered device.

The recommended user experience is:

- A secure platform keystore unlocks a previously enrolled device.
- A new device is approved by an existing trusted device.
- A separately stored 256-bit recovery code is the last-resort recovery path.
- A user-chosen Vault password is an optional compatibility fallback only when a secure platform keystore is unavailable. It is never silently replaced by an empty password or insecure local storage.

## 2. Relationship to the current implementation

Current Phase 9 already provides:

- optional Supabase Auth;
- Organization membership and RLS;
- opaque AES-256-GCM inventory snapshots produced in Rust;
- an Argon2id sync passphrase;
- optimistic `expected_revision` writes;
- tombstones, import preview, explicit conflict decisions, and content-free audit;
- short-lived access tokens kept out of persistent WebView storage;
- signed access-policy decisions verified in Rust.

Current Phase 9 deliberately excludes:

- SSH passwords;
- private-key passphrases;
- private-key content;
- CredentialVault keys or master secrets;
- `key_source` and local private-key paths;
- terminal output and remote file content.

This proposal is a versioned extension, not an in-place weakening of that boundary. The existing v2 snapshot remains readable during migration. Credential sync begins only after the user explicitly enables it and the v3 device/Vault foundation is ready.

## 3. Goals

The implementation SHALL provide:

1. End-to-end confidentiality against the cloud operator, database compromise, backups, and network intermediaries.
2. Authenticated, tamper-evident changes attributable to a registered device.
3. Per-device enrollment and revocation.
4. Cross-device synchronization of Profile and Group metadata.
5. Explicit opt-in synchronization of SSH passwords, key passphrases, and imported Vault private keys.
6. Safe recovery without giving the cloud a decryption key.
7. Per-object conflict handling without silently overwriting local state.
8. Local-only operation when cloud services are absent or unavailable.
9. Compatibility with Windows, macOS, Linux, iOS, and Android.
10. Preservation of `CredentialService`, SSH Core, host verification, and narrow Tauri IPC boundaries.

## 4. Non-goals

The first implementation SHALL NOT provide:

- server-side plaintext search;
- cloud-side SSH connection or credential testing;
- escrow or support-assisted recovery of plaintext Vault data;
- automatic migration of arbitrary desktop private-key files;
- synchronization of terminal output, session transcripts, remote files, model context, incident evidence, or arbitrary filesystem content;
- treating a synchronized known-host record as sufficient host verification;
- retroactive deletion of secrets already decrypted by a revoked device;
- a generic key-management, filesystem, HTTP, or shell IPC exposed to React or an LLM.

## 5. Threat model

### 5.1 In scope

- Read-only or read/write compromise of the Supabase database and its backups.
- An honest-but-curious cloud operator.
- TLS interception that does not also compromise the client trust store.
- Theft of a short-lived access token.
- Malicious replacement, truncation, duplication, or replay of encrypted sync objects.
- A stolen device while Runory and its Vault are locked.
- Cross-organization access attempts.
- A revoked device attempting future synchronization.
- Crash, power loss, or process termination during local migration or sync apply.
- Untrusted encrypted payloads crafted to trigger parser, allocation, or logic failures.

### 5.2 Inherent limitations

- Malware controlling an unlocked Runory process can observe secrets when they are legitimately used.
- An authorized team member can retain data already decrypted before their access was revoked.
- End-to-end encryption does not hide all metadata. The service necessarily sees account, organization, device, object count, ciphertext size, revision, and timing metadata.
- A cloud service can withhold data. Existing devices can detect rollback below their last accepted checkpoint; a completely fresh device cannot prove global freshness without an external transparency service.
- Recovery is impossible after all trusted devices and the recovery code are lost.

## 6. Trust boundaries

```text
Lower trust
  React/WebView
  Supabase Auth and Data API
  Edge Functions
  Postgres and backups
  Network responses
  Synced ciphertext and public metadata

Trusted for secret handling
  Rust Core
  LocalVaultService
  DeviceIdentityService
  CloudCryptoService
  CredentialService
  Platform secure-storage adapter
```

React MAY render status and conflict metadata. React MUST NOT receive Vault keys, device private keys, recovery codes after creation, decrypted credentials, private-key content, or a generic API to read stored secrets.

## 7. Data classification and sync policy

| Class | Examples | Default | Cloud representation |
|---|---|---:|---|
| Infrastructure metadata | host, port, username, profile/group name | Sync when enabled | E2EE object |
| SSH credential | password, private-key passphrase | Explicit opt-in | E2EE secret object |
| Imported private key | `KeySource::Vault` content | Separate explicit opt-in | E2EE secret object, size capped |
| File private-key reference | desktop path | Never sync path/content automatically | unresolved device-local binding |
| Known-host evidence | fingerprint and algorithm | Local-only by default | optional signed claim, never automatic trust |
| Application API secret | LLM API key, MCP token | Local-only in first release | future separately scoped secret collection |
| Runtime content | terminal output, SFTP content, logs | Never | absent |
| Vault/device secret | Vault keys, device private keys, recovery code | Never in plaintext | only encrypted key envelopes where specified |
| Auth token | access/refresh token | Never in sync data | process memory only under current policy |

Credential sync MUST be disabled by default, even if metadata sync is already enabled. Enabling it requires a separate explanation and confirmation. Imported private keys require an additional confirmation because they are long-lived authentication material.

## 8. Cryptographic design

### 8.1 Required primitives

The initial ciphersuite SHALL use maintained, reviewed libraries and published test vectors:

- Payload authenticated encryption: AES-256-GCM with a fresh random 96-bit nonce.
- Password fallback KDF: Argon2id, 128-bit random salt, 256-bit output.
- Device signatures: Ed25519.
- Device key envelopes: RFC 9180 HPKE using X25519 and HKDF-SHA256 with an approved AEAD suite.
- Hash and key derivation: SHA-256/HKDF-SHA256 with explicit domain separation.
- Randomness: operating-system CSPRNG only.

The implementation MUST use a maintained HPKE implementation and RFC test vectors. It MUST NOT hand-author a new ECIES-like construction.

Argon2id parameters SHALL be explicit and versioned in the envelope, not library defaults. The starting compatibility profile is `m=64 MiB, t=3, p=4`, subject to measured mobile and desktop gates. Devices MAY increase work factors, but every envelope records the parameters needed to open it. RFC 9106 identifies this as its recommended memory-constrained profile.

### 8.2 Key hierarchy

```text
Platform secure storage
  └─ protects Device Private Keys
       ├─ Device Signing Key (Ed25519)
       └─ Device Encryption Key (X25519 / HPKE recipient)

Vault Epoch Key (VEK, random 256-bit)
  ├─ HPKE envelope for Device A
  ├─ HPKE envelope for Device B
  └─ recovery envelope for the Personal Vault only

Vault Index Key (VIK, independent random 256-bit)
  └─ derives opaque cloud lookup IDs; distributed inside the Vault keyset envelope

Per-revision Object Data Encryption Key (ODEK, random 256-bit)
  ├─ encrypts exactly one object revision
  └─ is wrapped by the current VEK
```

Keys used for signing, HPKE, object encryption, object lookup, and recovery MUST be independent. No key may be reused for two purposes. A device or recovery envelope contains a versioned Vault keyset: the VIK, the current VEK, any bounded migration-only previous epochs, and a signed checkpoint. VIK stability preserves opaque object lookup IDs across ordinary VEK rotation.

### 8.3 Vault types

Runory defines three cryptographic scopes:

1. `PersonalInventoryVault`: personal Profile and Group metadata.
2. `PersonalSecretVault`: personal SSH credentials and explicitly imported private keys.
3. `OrganizationCollectionVault`: one organization-owned collection with an explicit device membership list.

Separating inventory and secrets allows metadata sync without granting every synchronized device or organization role access to credentials. Team secret sharing SHOULD use multiple named collections rather than one organization-wide master credential key.

### 8.4 Object encryption

Every object revision receives a fresh random ODEK. An ODEK is never reused for a later payload, including an update of the same logical object. The object plaintext contains its real type, logical UUID, schema version, value, and deletion state. The cloud index uses an opaque lookup ID derived from the VIK so the server does not need the local object UUID or type.

Conceptually:

```text
lookup_id = HMAC-SHA256(VIK, object_kind || logical_uuid)
payload_ciphertext = AES-256-GCM(ODEK, payload_nonce, plaintext, payload_aad)
wrapped_odek = AES-256-GCM(VEK, wrap_nonce, ODEK, wrap_aad)
digest = SHA-256(canonical_header || wrapped_odek || payload_ciphertext)
signature = Ed25519.sign(device_signing_key, digest)
```

The transport may encode fields as JSON/Base64, but signatures and AAD MUST be calculated over a specified fixed-order, length-prefixed binary encoding. Signing raw JSON serialization is forbidden.

The authenticated header includes at least:

- format version and ciphersuite ID;
- Vault ID and key epoch;
- opaque lookup ID;
- content revision, key-wrap revision, and parent digest;
- author device ID;
- wrapped-ODEK nonce and payload nonce;
- ciphertext lengths.

The Organization or Personal Vault identity is therefore cryptographically bound. Ciphertext cannot be moved between Vaults, devices, revisions, or object IDs without detection.

### 8.5 Nonce rules

- A new random nonce is generated for every AES-GCM encryption, including retries.
- Retrying an upload may reuse an already-created immutable ciphertext, but re-encryption always uses a new nonce.
- Nonces are never counters shared across devices.
- Tests SHALL force RNG failures and verify the operation fails closed.

### 8.6 Key rotation

Each Vault has a monotonically increasing public key epoch. A new VEK is generated when:

- a device is suspected compromised;
- an organization member or device loses collection access;
- the ciphersuite changes;
- an administrative rotation is requested.

Because each payload revision uses an ODEK, rotation can rewrap the current ODEKs under the new VEK without re-encrypting large payloads. A rewrap produces a signed key-wrap revision while leaving the content revision and payload ciphertext unchanged. Every later content update uses a new ODEK, so knowledge of a pre-revocation ODEK cannot decrypt a future revision. Rewrapping cannot revoke plaintext or ODEKs already obtained by a removed device. Old VEKs remain available only while needed to complete a bounded migration and are then zeroized and removed from active device keysets.

Rotation cannot erase secrets already seen by a revoked device. UI and audit language must state this limitation.

## 9. Local security architecture

### 9.1 PlatformKeyStore abstraction

Rust owns a narrow `PlatformKeyStore` abstraction used only by `DeviceIdentityService` and `LocalVaultService`:

```text
Windows   Credential Manager or DPAPI-protected application secret
macOS     Keychain; hardware-backed/non-exportable key where supported
Linux     Secret Service-compatible keyring
iOS       Keychain with device-only accessibility policy
Android   Android Keystore, hardware-backed where supported
```

Private keys MAY be stored as non-exportable platform keys when the required algorithms are consistently available. Otherwise, the platform keystore protects a random wrapping key, and Rust stores only an authenticated encrypted private-key blob.

There is no `localStorage`, plaintext file, environment-variable, compiled-key, or empty-password fallback. If secure storage is unavailable, Runory offers:

- session-only operation; or
- an explicit local master passphrase using versioned Argon2id parameters.

### 9.2 Local repositories

The existing Service boundaries remain stable. Encryption is added behind repositories rather than moving persistence into React or SSH services.

```text
ProfileService / GroupService / CredentialService
                     ↓
Encrypted Repository and CredentialVault adapters
                     ↓
Atomic local files + PlatformKeyStore-protected Local Vault Key
```

At minimum, credentials and imported private keys remain encrypted. The recommended v3 migration also encrypts infrastructure metadata at rest because hostnames, usernames, and organization layout are sensitive. Repository writes retain temporary-file, flush, `fsync`, validation, and atomic-replace semantics.

### 9.3 Unlock lifecycle

- Application launch does not automatically expose secrets to React.
- A successful platform unlock loads only the minimum key material into Rust memory.
- Lock, sign-out, OS session lock, and configured inactivity timeout zeroize decrypted Vault keys and cached secrets.
- Mobile backgrounding immediately applies the privacy cover. Biometric authentication gates reuse of a device key but never returns Vault material to React.
- SSH credentials are resolved immediately before authentication and zeroized after use.
- Private-key content remains inside Rust and is never returned through a general IPC command.

### 9.4 Local outbox and cursor

Local synchronization state is content-free and atomic:

- last accepted server sequence per Vault;
- last accepted object digest and revision;
- pending encrypted/signed outbox entries;
- device and Vault public IDs;
- tombstone acknowledgement state;
- migration version.

The outbox may persist ciphertext but never plaintext or unwrapped keys. A local mutation and its outbox intent are committed under the same repository write lock so a crash cannot create an untracked local change.

## 10. Account authentication versus Vault access

Supabase Auth proves account identity and supplies a short-lived authorization token. It does not unlock Vault data.

Current session rules remain:

- publishable key only in the app;
- no service-role key in React, Rust, or the application package;
- `persistSession=false` in WebView storage;
- the Supabase session exists only in WebView process memory; Rust receives the current access token through a narrow session-update command and keeps its copy in zeroizing process memory;
- refresh token remains process-memory-only under the current sign-in policy;
- MFA and verified email are required for device enrollment, recovery use, destructive reset, and organization key administration.

Account password changes do not rotate VEKs. Recovery-code rotation changes only the recovery envelope unless compromise is suspected, in which case VEK rotation is also required.

## 11. Device identity and enrollment

### 11.1 First device

1. User completes recent Supabase authentication and MFA.
2. Rust generates independent Ed25519 and X25519 key pairs.
3. Private keys are protected by `PlatformKeyStore` before any cloud registration.
4. Rust generates Personal Inventory and Personal Secret VEKs.
5. The user is shown a randomly generated 256-bit recovery code once and confirms it has been saved.
6. The recovery code derives a Recovery KEK using HKDF over the full-entropy code; a user-chosen recovery passphrase, if supported, uses Argon2id instead.
7. Rust uploads the device public keys, recovery envelopes, and empty Vault heads.
8. Server bootstrap succeeds only when the user has no existing active device and no existing personal Vault.

The server never generates the recovery code or any VEK.

### 11.2 Adding a device with an existing device

1. New device signs in, generates its key pairs, and creates an expiring enrollment request.
2. New and existing devices display/scan a QR code and a short authentication string derived from the enrollment transcript.
3. The existing trusted device shows the new device name, platform, public-key fingerprints, account, and request expiry.
4. After explicit approval, the existing device signs a device certificate and HPKE-wraps each authorized VEK to the new device public key.
5. The cloud verifies the approver is active, the challenge is unused, and the request is unexpired, then atomically activates the new device and stores the envelopes.
6. The new device verifies the approver certificate chain, opens its envelopes, downloads objects, and validates every signature before apply.

The cloud account session alone cannot complete this flow.

Device certificates, approvals, and revocations form a signed device-trust event chain. The first device creates a self-signed genesis event whose digest is included in the recovery keyset. Every later event binds the previous trust-event digest. Existing devices pin the highest accepted trust checkpoint; a fresh device receives that checkpoint through the approving-device transcript or recovery envelope. Database `active/revoked` state is still enforced for availability and RLS, but clients do not treat an unsigned server-side device row as cryptographic authority.

### 11.3 Recovery without another device

1. User completes recent account authentication and MFA.
2. New device downloads only the personal recovery envelopes and public metadata.
3. Recovery code is submitted once from an uncontrolled secret input to a narrow Rust command and is never placed in React State or Zustand.
4. Rust opens the Personal Vault keyset, registers a new device, and rotates the recovery envelope.
5. Runory recommends revoking lost devices and rotating affected VEKs.

Rate limiting occurs both in the UI and cloud API, but cryptographic safety does not depend on server rate limiting because the recovery code has 256 bits of entropy.

### 11.4 Device revocation

- Revocation requires recent MFA and confirmation on an active trusted device, or recovery flow.
- The server immediately blocks the device from pull, push, enrollment, and key-envelope APIs.
- Remaining devices receive a mandatory rotation task for every affected secret Vault.
- Organization collection owners rotate collection VEKs and rewrap ODEKs.
- Revocation is audited without credential content.
- A revoked device may retain previously downloaded data; this fact is displayed explicitly.

## 12. Cloud synchronization protocol

### 12.1 Pull

1. Rust requests signed device-trust and Vault-membership events, then a bounded object page after the locally committed server sequence.
2. The cloud applies JWT, RLS, active-device, membership, and quota checks.
3. Rust rejects unsupported versions, excessive counts/sizes, unknown devices, invalid signatures, broken parent links, revision regressions, and invalid AEAD before parsing plaintext.
4. Objects are decrypted and schema-validated in Rust with strict allocation limits.
5. A merge preview is built in Rust memory.
6. Safe additions and non-conflicting changes may auto-apply. Destructive or ambiguous changes require an explicit decision.
7. Local repositories, accepted heads, tombstones, and cursor update atomically.

The cursor advances only after durable local apply. A failed item prevents acknowledgement of its page.

### 12.2 Push

1. Rust drains a bounded local outbox batch.
2. Each mutation receives a fresh ODEK, including updates to an existing logical object.
3. Rust creates the wrapped ODEK, authenticated header, ciphertext digest, and device signature.
4. The Sync Gateway validates JWT, active device, membership/role, request size, ciphersuite, signature, expected revision, and parent digest.
5. A database transaction appends an immutable change, advances the object head with compare-and-swap, and writes content-free audit metadata.
6. Rust verifies the returned object identity, revision, and digest before removing the outbox entry.

Direct table mutation by `authenticated` is revoked. The gateway/RPC is the only write path.

### 12.3 Retry and idempotency

Every mutation has a random `operation_id`. The server stores a bounded idempotency record keyed by `(device_id, operation_id)`. Repeating an identical request returns the original result. Reusing the ID with different bytes fails closed.

### 12.4 Conflict handling

The server performs no plaintext merge. CAS conflicts return the current encrypted head.

Rust uses a three-way comparison between the last accepted base, local value, and remote value:

- disjoint metadata field edits may merge automatically;
- same-field edits require user selection;
- secret changes never concatenate or field-merge;
- delete-versus-update requires explicit confirmation and defaults to keeping local data;
- group deletion does not silently delete child Profiles;
- conflict decisions bind Vault ID, opaque object ID, local digest, remote digest, and expected local timestamp;
- Apply revalidates the same binding under the repository write lock to prevent TOCTOU.

Secrets are identified in the UI by their owning Profile and credential type, never by displaying the secret.

### 12.5 Tombstones and compaction

Deletion is represented by an encrypted, signed tombstone object. Tombstones are retained until every active device has acknowledged a sequence newer than the tombstone, plus a minimum retention period. Devices inactive beyond the retention ceiling must perform a full resync and cannot rely on an old cursor.

Compaction creates a signed checkpoint and does not delete the latest object head. The service must never infer deletion merely because an object is absent from an incomplete page or snapshot.

## 13. Rollback and replay defense

- Each content or key-wrap revision binds its parent digest.
- Each accepted device stores the highest server sequence and object head digest locally.
- A response below the local checkpoint, a changed ciphertext at the same revision, or a broken parent chain is rejected.
- New devices receive a signed Vault checkpoint from the approving device or from the recovery keyset.
- The cloud cannot forge a newer valid object without an active device key.
- A malicious cloud can still withhold the newest signed state; this availability limitation is documented.

An optional future transparency log may strengthen fresh-device rollback detection, but v3 does not depend on an unimplemented claim.

## 14. Organization and team sharing

Organization membership and cryptographic access are separate states:

```text
Database membership accepted
        ↓
Pending cryptographic grant
        ↓ owner/admin trusted device approves
Collection VEK envelope delivered to each authorized device
        ↓
Shared Vault usable
```

An invitation or RLS role alone never grants decryption.

Recommended first-release collection rules:

- Owner/Admin may manage collection membership.
- Operator may receive a collection key only when explicitly granted.
- Viewer receives inventory access by default but no credential collection key.
- Access Policy may further deny Connect/Operate/Deploy, but policy cannot grant a key the device does not possess.
- Removing a user revokes all their devices from the collection and forces a collection VEK rotation.
- Adding a device to an existing user still requires collection-specific envelopes.

All recipients of a shared credential can copy it while authorized. Runory must not claim DRM-like control over already disclosed secrets.

## 15. Known-host handling

Host verification remains mandatory and independent from Cloud Sync.

- Local known-host state is not uploaded by default.
- A future synchronized fingerprint is a signed claim from another trusted device, not automatic proof of server identity.
- First use on a new device still offers Trust Once, Trust & Remember, or Cancel after showing the fingerprint.
- A changed host key always blocks. Sync cannot auto-accept or suppress the warning.
- Conflicting signed claims are shown as a security conflict and never resolved by latest timestamp.

## 16. Proposed cloud data model

Existing `organizations`, `organization_members`, `organization_invites`, `access_policies`, and content-free `audit_records` remain. V3 adds versioned tables rather than repurposing the v2 snapshot shape:

### `devices`

- `id`, `user_id`, display metadata;
- Ed25519 signing public key;
- X25519 encryption public key;
- status: `pending | active | revoked`;
- certificate, creator device, created/activated/revoked timestamps;
- unique public-key fingerprints.

### `device_trust_events`

- immutable per-user sequence and previous-event digest;
- genesis, approve, revoke, and recovery event kinds;
- subject/author device IDs, public-key fingerprints, expiry where applicable;
- author signature and event digest.

### `vaults`

- `id`, owner user or organization;
- Vault class and current key epoch;
- encrypted object count/quota metadata;
- latest server sequence and checkpoint digest;
- no plaintext name or secret.

### `vault_device_access`

- Vault, device, access state and collection role;
- grant/revoke sequence;
- RLS source for pulls.

### `vault_membership_events`

- immutable per-Vault grant/revoke/epoch sequence;
- previous-event digest, author device, target device and collection scope;
- author signature and event digest;
- cryptographic source for the client-side membership checkpoint.

### `vault_key_envelopes`

- Vault, key epoch, recipient device;
- HPKE ciphersuite, encapsulated key, ciphertext;
- grantor device and signature;
- unique `(vault_id, epoch, recipient_device_id)`.

### `recovery_envelopes`

- personal Vault only;
- versioned recovery method/KDF parameters, salt where applicable, nonce and ciphertext;
- no recovery secret or verifier that enables online authentication.

### `vault_objects`

- Vault ID and opaque lookup ID;
- current head digest and key epoch;
- distinct content revision, key-wrap revision, and parent digest;
- encrypted ODEK and encrypted payload;
- author device, signature, ciphertext digest;
- server-created/updated timestamps;
- unique `(vault_id, opaque_lookup_id)`.

### `vault_changes`

- immutable server sequence;
- Vault/object identity, revision, digest, author device;
- encrypted revision body or reference to immutable object version;
- append-only for synchronization and auditability.

### `sync_idempotency`

- device and operation ID;
- request digest and committed result;
- bounded expiry and size.

The database SHALL enforce RLS, foreign keys, size/count constraints, non-negative revisions, unique keys, and bounded RPC arguments. `anon` has no business-table access. `authenticated` has no direct mutation grant on encrypted object, key-envelope, device, or membership tables.

## 17. Edge and server responsibilities

The Sync Gateway is a narrow Edge Function/API, not a general proxy. It may:

- validate Supabase JWT and recent-auth/MFA claims where required;
- verify registered-device signatures;
- enforce active device and organization/Vault membership;
- enforce CAS, quotas, object sizes, allowed ciphersuites, and rate limits;
- call fixed database RPCs;
- return opaque encrypted records and public metadata.

It must not:

- receive a recovery code, local master password, VEK, ODEK, SSH password, private key, or decrypted payload;
- log request bodies, authorization headers, ciphertext, key envelopes, or signatures at debug level;
- offer arbitrary SQL, HTTP forwarding, filesystem, or function dispatch;
- bypass membership because a service-role key is available.

If a service-role secret is needed inside the Edge Function, it remains an Edge secret and the function must fully reconstruct caller authorization before every fixed RPC. Prefer forwarding the caller JWT to RLS-constrained RPCs where signature verification can remain trustworthy.

## 18. Audit and privacy

Cloud audit records may contain:

- organization/Vault/device public IDs;
- actor account ID;
- action category;
- object opaque lookup ID or ciphertext digest;
- result and stable error code;
- byte count, revision, and timestamp.

They must not contain:

- host, username, Profile/Group name;
- credential type if avoidable;
- password, passphrase, private-key content;
- decrypted error text;
- access/refresh token, recovery code, VEK/ODEK;
- ciphertext or key-envelope body.

Client logs follow the same rule. Diagnostic export reports whether content was omitted.

## 19. Failure behavior

| Failure | Required behavior |
|---|---|
| Cloud unavailable | Local SSH/SFTP continues; outbox remains pending |
| Auth expired | Sync pauses; local operation continues unless organization governance separately denies it |
| Vault locked | Metadata UI may show safe local display state; secret sync/use waits for unlock |
| Invalid signature/AEAD | Reject item and page; do not advance cursor |
| Unknown ciphersuite/version | Fail closed and retain original bytes for bounded diagnostics without content logging |
| CAS conflict | Pull current head and create Rust merge preview |
| Local disk full during apply | Preserve original repository and cursor; retry later |
| Device revoked | Clear in-memory cloud session/Vault keys; block future sync; local cached data remains explicitly marked |
| Recovery lost | No cloud-assisted plaintext recovery; allow destructive Vault reset only |
| Partial key rotation | Keep previous epoch read-only until every required object/envelope is verified, then retire it |

Cloud failure must never weaken SSH host verification or cause React to fall back to plaintext secret handling.

## 20. Migration from Phase 9 v2

Migration is explicit, resumable, and fail-safe.

### Gate M0: cryptographic foundation

- Freeze canonical encodings and domain-separation labels.
- Add published test vectors for AEAD, Argon2id, HPKE, signatures, and full object envelopes.
- Complete dependency maintenance, license, mobile-compile, and misuse-resistance review.
- Obtain an external cryptographic design review before credential sync is enabled.

M0 dependency decision:

- `hpke 0.14.0` is required because the existing stack does not implement the complete RFC 9180 state machine. It is pure Rust, MIT/Apache-2.0, has MSRV 1.85, publishes RFC KAT coverage, and is compiled only with `alloc + x25519 + chacha`; draft PQ and unused NIST curves remain disabled.
- `ed25519-dalek 3.0.0` already exists in the lock graph and is made direct for typed Ed25519 signing keys with zeroize-on-drop support. It is BSD-3-Clause and MSRV 1.85.
- `getrandom 0.4.3` and `rand_chacha 0.10.0` provide a fallible OS-entropy step followed by a per-operation ChaCha CSPRNG. This avoids HPKE convenience APIs that document a panic if the system RNG fails.
- These libraries are platform-neutral Rust dependencies and must pass the Android/iOS compile gates before M0 is considered complete.

M0 implementation checkpoint (2026-09-01):

- Added a private Rust-only crypto module with fallible key generation, X25519/HKDF-SHA-256/ChaCha20-Poly1305 HPKE Vault-keyset envelopes, Ed25519 device signatures, HMAC-SHA-256 opaque lookup IDs, and per-object AES-256-GCM encryption with fresh ODEKs.
- Canonical object headers bind the Vault, opaque object ID, key epoch, content/wrap revisions, parent digest, author device, nonces, and plaintext length. The object digest covers the canonical header, wrapped ODEK, and ciphertext before signing.
- Seven focused tests pass, including RFC 9180 HPKE, RFC 8032 Ed25519, and NIST AES-256-GCM known-answer vectors, HPKE recipient-context binding, fresh-key object encryption, tamper rejection, and cross-Vault replay rejection. The complete Rust library suite also passes.
- No Tauri command, React API, sync transport, credential repository adapter, or upload path exposes this module. Credential sync remains disabled.
- M0 is **in progress**, not complete. Remaining gates are the published Argon2id/full-envelope vectors and expanded malformed-input matrix, configured Android and iOS cross-compiles, and an external cryptographic review. The first Android check stopped in the existing `aws-lc-sys` dependency because the local Android NDK Clang toolchain was not configured; it did not reach Runory source compilation.

### Gate M1: local Vault v2

- Add `PlatformKeyStore`, device identity, and encrypted local metadata adapters.
- User unlocks the existing Stronghold/Portable Vault once.
- Generate new random local keys and re-encrypt records into the new format.
- Write to a temporary location, `fsync`, reopen and verify every record, then atomically switch.
- Preserve the old Vault as a recoverable backup until the new Vault has passed a full restart/unlock test.
- Never delete or overwrite a corrupt source Vault.

M1 implementation checkpoint (2026-09-01):

- Added the Rust-only `PlatformKeyStore` boundary and desktop native adapters through `keyring 3.6.3`: Windows Credential Manager, macOS Keychain, and Linux persistent Secret Service/keyutils. The dependency is MIT/Apache-2.0, MSRV 1.75, and is excluded from Android/iOS builds.
- A new desktop Vault is created with a CSPRNG-generated 256-bit secret stored in the OS credential store; React no longer asks the user to invent a Vault password on supported desktop platforms.
- An existing Stronghold Vault is opened once with its original password. Only after successful decryption is that same unlock secret written to the OS store. A read-only runtime probe distinguishes a compiled backend from a usable login-session backend: known-unavailable storage permits password unlock for the current session, while an unexpected write failure after a successful probe re-locks the Vault and reports a stable error.
- App startup automatically unlocks only an already-existing Vault. A stale OS credential can never create a replacement empty Vault. Manual session lock remains available and does not delete the OS credential.
- This is the M1 desktop unlock migration slice, not completion of M1. It deliberately leaves the existing Stronghold ciphertext in place instead of performing a risky record rewrite. The native Windows write/read integration test is isolated and ignored by default; the current non-interactive development logon session returned Windows error 1312, so an interactive packaged-app restart test remains a release gate. Remaining work includes Android Keystore/iOS Keychain adapters, device identity, encrypted metadata v2, atomic record migration with verified backup, and cross-platform restart tests.

### Gate M2: personal metadata sync v3

- Pull and apply the existing v2 Inventory snapshot first.
- Create per-object v3 encrypted records from the resulting local state.
- Upload v3 under a feature flag and record a signed migration checkpoint.
- Stop writing v2 after the account is committed to v3; do not maintain indefinite dual-write logic.
- Older clients become read-only for cloud sync and remain fully local-capable.

### Gate M3: personal credential sync

- Separate opt-in for passwords/passphrases.
- Separate opt-in for each imported private key or an explicit reviewed batch.
- File-based private keys remain device-local until imported through the existing Rust validation path.
- Verify that React, Zustand, logs, v2 tables, audit, crash reports, and release evidence contain no secrets.

### Gate M4: device enrollment and recovery

- Ship trusted-device approval, QR/short-code verification, recovery code, revocation, and key rotation.
- Complete lost-device, lost-recovery, account-reset, and rollback drills.

### Gate M5: organization collections

- Add pending cryptographic grants after database invitation acceptance.
- Add per-device collection envelopes and member-removal rotation.
- Verify RLS and cryptographic membership cannot diverge into accidental access.

At every gate, disabling Cloud Sync leaves the local repositories and SSH Core functional. Migration failure never deletes local Profiles or credentials.

## 21. Required testing

### 21.1 Cryptographic unit tests

- RFC/library vectors for HPKE, Ed25519, HKDF, Argon2id, and AEAD.
- Wrong key, salt, nonce, AAD, Vault ID, device ID, epoch, revision, parent digest, and signature.
- Truncated, oversized, duplicated, reordered, unknown-version, and non-canonical fields.
- Nonce uniqueness under concurrency and retry.
- Zeroization and no `Debug` formatting for secret wrappers.

### 21.2 Local persistence tests

- Crash injection before/after temporary write, flush, fsync, verify, rename, and cursor commit.
- Corrupt old/new Vault preserves source and fails closed.
- Platform keystore unavailable/locked/invalidated.
- OS user change, device restore, app reinstall, biometric cancellation, and inactivity lock.
- No plaintext secret or sensitive metadata in repository files, temporary files, swap-intended buffers, logs, or crash artifacts within the application's control.

### 21.3 Sync protocol tests

- Two-device create/update/delete/reorder and offline convergence.
- Concurrent same-field and disjoint-field edits.
- Secret conflict and delete/update conflict.
- Idempotent retry and operation-ID mismatch.
- CAS race, page replay, rollback, withheld page, cursor corruption, and checkpoint mismatch.
- Device approval expiry/reuse/substitution and short-code mismatch.
- Device revocation during pull, push, and key rotation.
- Recovery with correct/incorrect code and all-devices-lost drill.

### 21.4 Supabase security tests

- `anon` denial and cross-user/cross-organization isolation.
- Direct DML denial on all v3 sensitive tables.
- RPC/Edge signature, active-device, role, recent-auth, size, count, and rate-limit enforcement.
- Viewer cannot obtain secret collection envelopes by changing request parameters.
- Removed member cannot fetch new epochs or objects.
- Security-definer functions have fixed empty `search_path` and revoked `PUBLIC` execution.
- Backups and audit exports contain only ciphertext/public metadata.

### 21.5 Product regression tests

- Password, private-key, encrypted-key, wrong credential, PTY, resize, disconnect, and reconnect against real Docker OpenSSH.
- Unknown host still requires explicit trust; changed host remains blocked.
- Terminal output never enters React State/Zustand or sync payloads.
- Local-only mode works with no Supabase configuration and with total cloud outage.
- Desktop and Android/iOS Core compile gates pass.
- i18n completeness covers every enrollment, recovery, conflict, and destructive-reset message.

## 22. Security invariants

The implementation is not complete unless all of these remain true:

1. Cloud compromise alone cannot decrypt synchronized content.
2. Account password or access token alone cannot decrypt a Vault.
3. React never receives persistent credential plaintext or key material.
4. The cloud never receives a recovery secret, VEK, ODEK, or local master password.
5. Every accepted mutation is AEAD-authenticated, device-signed, revision-bound, and attributable to an active device at write time.
6. Device or member revocation blocks future access and triggers key rotation where secrets are shared.
7. Known-host synchronization cannot bypass mandatory host verification.
8. Sync conflicts default to preserving local data and require explicit approval for destructive resolution.
9. Local-only mode remains fully usable.
10. No generic Tool, Shell, Filesystem, Vault, or Sync IPC is introduced.
11. Secrets never enter logs, audit, telemetry, model context, crash reports, or release evidence.
12. Migration is atomic, verified, resumable, and preserves the original on corruption.

## 23. Implementation shape

Suggested Rust modules, preserving existing services:

```text
src-tauri/src/cloud/
  crypto.rs              object AEAD, envelopes, canonical digests
  device.rs              device identity and enrollment domain service
  keyring.rs             Vault epochs, recovery, rotation
  protocol.rs            bounded pull/push DTOs
  sync_engine.rs         outbox, cursor, merge orchestration
  repository.rs          encrypted sync state and atomic persistence
  transport.rs           fixed cloud endpoints only

src-tauri/src/credentials/
  platform.rs            PlatformKeyStore trait/adapters
  migration.rs           existing Vault to local v2 migration
```

Tauri commands remain business-specific, for example:

```text
cloud_sync_status
cloud_sync_now
cloud_device_enrollment_begin
cloud_device_enrollment_approve
cloud_device_revoke
cloud_recovery_create
cloud_recovery_restore
cloud_conflict_preview
cloud_conflict_apply
```

There is no command to decrypt an arbitrary object, read a Vault key, sign arbitrary bytes, call an arbitrary URL, or export a stored credential.

## 24. Rollout and operational gates

- Feature flags are server- and client-version bound.
- Metadata v3 ships before credential v3.
- Credential sync requires a separate production flag and external security review.
- Staging uses synthetic credentials only.
- Production migration starts with a small opt-in cohort and provides immediate pause without local data loss.
- Metrics contain counts, latency, stable error codes, client/schema versions, and ciphertext byte sizes only.
- Key-recovery and device-revocation runbooks are exercised before general availability.
- Database migration, Edge deployment, Auth configuration, signing-key changes, and production enablement remain separate reviewed release actions.

## 25. Reference basis

- [RFC 9106 — Argon2 Memory-Hard Function](https://www.rfc-editor.org/rfc/rfc9106.html)
- [RFC 9180 — Hybrid Public Key Encryption](https://www.rfc-editor.org/rfc/rfc9180.html)
- [OWASP Cryptographic Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html)
- [OWASP Key Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Key_Management_Cheat_Sheet.html)
- [NIST SP 800-57 Part 1 Rev. 5 — Key Management](https://csrc.nist.gov/pubs/sp/800/57/pt1/r5/final)

These references guide primitive and lifecycle choices. Runory's concrete protocol, canonical encoding, limits, and migration behavior remain versioned application contracts and require independent review before production credential synchronization.
