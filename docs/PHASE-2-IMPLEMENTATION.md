# Voxora Phase 2 Beta — Implementation Guide

> **Source RFC**: `docs/RFC-0001-voxora-platform.md`
> **Prerequisite**: Phase 1 MVP complete (text chat, auth, communities, channels, messages, Gateway)
> **Target date**: Months 5–8 of the project timeline
> **Goal**: Voice channels, desktop client, pod verification, notification system, file attachments, threads, typing indicators, presence, and advanced RBAC — taking Voxora from "functional MVP" to "daily-driveable beta."

---

## Table of Contents

1. [Phase 1 Recap](#1-phase-1-recap)
2. [Architecture Changes](#2-architecture-changes)
3. [Work Streams](#3-work-streams)
4. [WS-1: Hub API](#ws-1-hub-api)
5. [WS-2: Pod API](#ws-2-pod-api)
6. [WS-3: Web Client](#ws-3-web-client)
7. [WS-4: Desktop Client (Electron)](#ws-4-desktop-client-electron)
8. [WS-5: Pod Admin SPA](#ws-5-pod-admin-spa)
9. [Database Migrations](#9-database-migrations)
10. [New Dependencies](#10-new-dependencies)
11. [Integration Test Plan](#11-integration-test-plan)
12. [Task Dependency Graph](#12-task-dependency-graph)
13. [Out of Scope for Phase 2](#13-out-of-scope-for-phase-2)

---

## 1. Phase 1 Recap

Phase 1 delivered the core platform:

| Component  | What exists                                                                                                                              |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Hub API    | OIDC provider (auth code + PKCE), user registration, SIA issuance, JWKS, pod registry, user profiles, bookmarks                          |
| Pod API    | SIA validation, community/channel/message CRUD, reactions, basic RBAC (admin/mod/member), invites, WebSocket Gateway (core events), bans |
| Web Client | OIDC login, multi-pod connections, community/channel sidebar, real-time messaging (Plate rich text), reactions, settings, Pod Browser    |
| Shared     | `voxora-common` (ULID IDs, Snowflake generator, error types), OpenAPI specs                                                              |

**What Phase 1 does NOT have:** voice/video, file uploads, threads, pins, typing indicators, presence, notification relay, MFA, pod verification, desktop client, advanced RBAC (channel overrides), Gateway resume/replay.

---

## 2. Architecture Changes

### 2.1 Phase 2 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                          CLIENTS                            │
│   ┌──────────┐  ┌──────────────┐  ┌──────────┐             │
│   │   Web    │  │   Desktop    │  │  Mobile  │             │
│   │  (SPA)   │  │  (Electron)  │  │ (future) │             │
│   └────┬─────┘  └──────┬───────┘  └────┬─────┘             │
│        └────────────────┼───────────────┘                   │
│                         │                                   │
│              ┌──────────▼──────────┐                        │
│              │   OIDC + REST +     │                        │
│              │   Hub Gateway WS    │  ← NEW (notification   │
│              │   (preferred pod    │     relay for paid/     │
│              │    direct WS ×10)   │     managed pods)       │
│              └──────────┬──────────┘                        │
└─────────────────────────┼───────────────────────────────────┘
                          │
             ┌────────────▼────────────┐
             │          HUB            │
             │  ┌──────────────────┐   │
             │  │  OIDC Provider   │   │
             │  │  User Profiles   │   │
             │  │  Pod Registry    │   │
             │  │  MFA (TOTP/WA)   │   │  ← NEW
             │  │  Verification    │   │  ← NEW
             │  │  Notification    │   │  ← NEW
             │  │    Relay         │   │
             │  │  TURN Creds      │   │  ← NEW
             │  │  Hub Gateway     │   │  ← NEW
             │  │  JWKS Endpoint   │   │
             │  └──────────────────┘   │
             └──────┬──────────┬───────┘
                    │          │
          ┌─────────▼──┐  ┌───▼─────────┐
          │   Pod A    │  │   Pod B     │
          │ ┌────────┐ │  │ ┌────────┐  │
          │ │REST API│ │  │ │REST API│  │
          │ │Gateway │ │  │ │Gateway │  │
          │ │  SFU   │ │  │ │  SFU   │  │  ← NEW
          │ │Storage │ │  │ │Storage │  │  ← NEW (attachments)
          │ └────────┘ │  │ └────────┘  │
          └────────────┘  └─────────────┘
```

### 2.2 Key Architectural Additions

**Hub Gateway WebSocket**: New client-facing WebSocket endpoint on the Hub for notification relay. Separate from the Pod Gateway — this carries only lightweight notification envelopes (unread counts, mention flags), never message content.

**SFU Sidecar (`voxora-sfu`)**: Each Pod runs a separate Rust binary that uses the `mediasoup` Rust crate for voice/video. The Pod API manages signaling and spawns/supervises the sidecar; the sidecar handles media routing. Communication is via JSON-over-Unix-socket IPC.

**Object Storage**: Pods gain file upload capability via local filesystem or S3-compatible storage for attachments.

**Preferred Pods**: Clients maintain up to 10 direct WebSocket connections to user-selected "preferred" pods for free real-time notifications.

### 2.3 New Gateway Opcodes

| Opcode | Name               | Direction       | Phase |
| ------ | ------------------ | --------------- | ----- |
| 3      | RESUME             | Client → Server | 2     |
| 4      | VOICE_STATE_UPDATE | Client → Server | 2     |
| 5      | VOICE_SERVER       | Server → Client | 2     |
| 7      | RECONNECT          | Server → Client | 2     |
| 9      | PRESENCE_UPDATE    | Client → Server | 2     |
| 10     | SUBSCRIBE          | Client → Server | 2     |
| 11     | UNSUBSCRIBE        | Client → Server | 2     |

### 2.4 New Dispatch Events

| Event Name              | Description                      |
| ----------------------- | -------------------------------- |
| `RESUMED`               | Session resumed successfully     |
| `TYPING_START`          | User started typing              |
| `VOICE_STATE_UPDATE`    | User voice state changed         |
| `VOICE_SERVER_UPDATE`   | Voice server connection details  |
| `PRESENCE_UPDATE`       | User presence changed            |
| `CHANNEL_PINS_UPDATE`   | Channel pins changed             |
| `THREAD_CREATE`         | Thread started                   |
| `THREAD_UPDATE`         | Thread modified (archived, etc.) |
| `THREAD_MEMBERS_UPDATE` | Thread member list changed       |

---

## 3. Work Streams

Phase 2 is organized into four parallel work streams. The Desktop Client (WS-4) is a new stream.

| Stream | App            | Can Start Immediately            | Blocked On                         |
| ------ | -------------- | -------------------------------- | ---------------------------------- |
| WS-1   | Hub API        | Yes                              | —                                  |
| WS-2   | Pod API        | Yes (most tasks independent)     | WS-1 (TURN creds) for voice        |
| WS-3   | Web Client     | Partially                        | WS-2 (voice, threads, etc.) for UI |
| WS-4   | Desktop Client | Yes (shell setup is independent) | WS-3 (shared React app)            |
| WS-5   | Pod Admin SPA  | Yes                              | WS-2 (Pod API endpoints)           |

**Critical paths:**

- Voice: Hub TURN provisioning → Pod SFU setup → Pod voice signaling → Web Client voice UI → Desktop voice
- Notifications: Hub preferred pods API → Hub notification relay → Client notification negotiation
- Desktop: Electron shell → embed web client → system tray + hotkeys → auto-update

---

## WS-1: Hub API

### WS-1.1 Project Structure Changes

New files and directories added to `apps/hub-api/src/`:

```
apps/hub-api/src/
├── ...existing...
├── routes/
│   ├── ...existing...
│   ├── mfa.rs              # MFA enrollment + verification endpoints
│   ├── verification.rs     # Pod verification flow endpoints
│   ├── turn.rs             # TURN credential provisioning
│   ├── preferences.rs      # User preferences (preferred pods)
│   └── notifications.rs    # Notification push endpoint (pod → hub)
├── auth/
│   ├── ...existing...
│   ├── totp.rs             # TOTP generation + validation
│   └── webauthn.rs         # WebAuthn/Passkey registration + auth
├── gateway/
│   ├── mod.rs              # Hub Gateway WebSocket server
│   ├── session.rs          # Per-connection state (user_id, subscriptions)
│   └── relay.rs            # Notification envelope routing
├── verification/
│   ├── mod.rs
│   ├── domain.rs           # DNS TXT / HTTP well-known verification
│   └── checks.rs           # Security + policy checklist
└── migrations/
    ├── ...existing...
    ├── 20260214120000_create_passkeys/
    ├── 20260214120001_create_mfa_backup_codes/
    ├── 20260214120002_add_user_mfa_fields/
    ├── 20260214120003_create_pod_verifications/
    ├── 20260214120004_add_pod_verification_field/
    └── 20260214120005_create_user_preferences/
```

### WS-1.2 Tasks

#### Task H2-1: MFA — TOTP Enrollment & Verification

**Priority: P1**
**Depends on: None (builds on existing auth)**

Add TOTP-based MFA as an optional security layer for user accounts.

**Endpoints:**

1. `POST /api/v1/users/@me/mfa/totp/enable` — Begin TOTP enrollment
   - Requires Bearer access token
   - Generates TOTP secret (RFC 6238, SHA-1, 6 digits, 30-second window)
   - Returns `{ secret, otpauth_uri, qr_code_data_uri, backup_codes: string[8] }`
   - Secret is stored encrypted, `mfa_enabled` stays `false` until confirmed

2. `POST /api/v1/users/@me/mfa/totp/confirm` — Confirm TOTP enrollment
   - Request: `{ code: "123456" }`
   - Validates the code against the pending secret
   - Sets `mfa_enabled = true` on the user record
   - Returns `{ backup_codes }` (the same 8 codes, shown one final time)

3. `POST /api/v1/users/@me/mfa/totp/disable` — Disable TOTP
   - Requires current TOTP code or backup code
   - Sets `mfa_enabled = false`, clears `mfa_secret`

4. `POST /api/v1/users/@me/mfa/verify` — Verify MFA during login
   - Called after successful password auth when `mfa_enabled = true`
   - Request: `{ code: "123456" }` or `{ backup_code: "ABCD-EFGH" }`
   - On success: issues the access_token + refresh_token (held pending during MFA challenge)
   - Backup codes are single-use; mark as consumed after use

**Login flow change:**

When MFA is enabled, the OIDC token endpoint returns a partial response after password validation:

```json
{
  "mfa_required": true,
  "mfa_token": "mfa_01KPQRST...",
  "mfa_methods": ["totp"]
}
```

The client then calls `POST /api/v1/users/@me/mfa/verify` with the `mfa_token` + code. On success, the full token set is returned.

**Implementation notes:**

- Use `totp-rs` crate for TOTP generation and validation
- Allow ±1 time step window (previous + current + next 30s period) for clock drift
- Backup codes: 8 codes, 8 alphanumeric characters each, Argon2id-hashed in DB
- Rate limit MFA verification: 5 attempts per `mfa_token`, then invalidate

#### Task H2-2: MFA — Passkey (WebAuthn) Registration & Auth

**Priority: P2**
**Depends on: None**

Add WebAuthn/Passkey support as a passwordless MFA option.

**Endpoints:**

1. `POST /api/v1/users/@me/passkeys/register/begin` — Begin passkey registration
   - Returns WebAuthn `PublicKeyCredentialCreationOptions`

2. `POST /api/v1/users/@me/passkeys/register/complete` — Complete registration
   - Request: `{ credential: <WebAuthn attestation response> }`
   - Stores credential in `passkeys` table

3. `GET /api/v1/users/@me/passkeys` — List registered passkeys

4. `DELETE /api/v1/users/@me/passkeys/{id}` — Remove a passkey

5. `POST /api/v1/auth/passkey/begin` — Begin passkey authentication
   - Returns `PublicKeyCredentialRequestOptions`

6. `POST /api/v1/auth/passkey/complete` — Complete passkey authentication
   - Validates assertion, updates `sign_count`, issues tokens

**Implementation notes:**

- Use `webauthn-rs` crate
- Passkeys can be used as primary auth (passwordless) or as MFA second factor
- Store `credential_id`, `public_key`, `sign_count`, `transports` per passkey
- Support multiple passkeys per user (e.g., device + security key)

#### Task H2-3: Pod Verification Flow

**Priority: P2**
**Depends on: None**

Implement the verification process for self-hosted pods (RFC §14).

**Endpoints:**

1. `POST /api/v1/pods/{pod_id}/verify` — Submit verification request
   - Requires pod owner's access token
   - Creates `pod_verifications` record with status `pending`
   - Returns `{ verification_id, domain_challenge_token }`

2. `GET /api/v1/pods/{pod_id}/verify` — Check verification status
   - Returns current verification state + checklist progress

3. `POST /api/v1/pods/{pod_id}/verify/domain` — Trigger domain verification check
   - Hub checks DNS TXT record: `_voxora-verify.{pod_domain} TXT "voxora-verify={token}"`
   - OR HTTP well-known: `GET https://{pod_domain}/.well-known/voxora-verify`
   - Updates `domain_proof` field with result

**Verification checks (automated):**

| Check                 | Method                                          | Auto |
| --------------------- | ----------------------------------------------- | ---- |
| Domain ownership      | DNS TXT or HTTP well-known                      | Yes  |
| TLS valid             | Connect and verify certificate chain            | Yes  |
| HTTPS-only            | Check HTTP redirects to HTTPS                   | Yes  |
| Pod software version  | Read from heartbeat `version` field             | Yes  |
| Rate limiting enabled | Check `X-RateLimit-*` headers on a test request | Yes  |

**Policy compliance (manual review — deferred):**
For Phase 2, policy compliance checks (community guidelines, moderator presence, abuse response time) are deferred. Verification in Phase 2 only covers the automated technical checks.

Once all automated checks pass, set `verification = 'verified'` on the pod record.

**Background job:**
Run automated re-checks daily via a scheduled task. If a previously verified pod fails 3 consecutive checks, revert to `unverified` and notify the pod owner via webhook.

#### Task H2-4: Preferred Pods API

**Priority: P1**
**Depends on: None**

Store and serve the user's preferred pod list (up to 10 pods that get direct WebSocket connections for free real-time notifications).

**Endpoints:**

1. `GET /api/v1/users/@me/preferences` — Get user preferences

   ```json
   {
     "preferred_pods": ["pod_01J9NX...", "pod_02AB..."],
     "max_preferred_pods": 10
   }
   ```

2. `PATCH /api/v1/users/@me/preferences` — Update preferences
   - Request: `{ "preferred_pods": ["pod_01...", "pod_02..."] }`
   - Validate: max 10 pods, all pod IDs must exist and be active
   - Validate: user must be a member of each pod (check bookmarks)

3. Extend `GET /api/v1/users/@me/pods` response to include notification capability per pod:

   ```json
   {
     "pods": [
       {
         "pod_id": "pod_01...",
         "name": "Gaming Pod",
         "url": "https://pod.example.com",
         "preferred": true,
         "relay": false
       },
       {
         "pod_id": "pod_02...",
         "name": "Work Team",
         "preferred": false,
         "relay": true
       }
     ]
   }
   ```

   `relay` is `true` when: pod is managed, pod has paid plan, OR user has paid plan.
   For Phase 2, since billing is not implemented yet, `relay` is `true` only for managed pods (if any exist).

**DB Table:**

```sql
CREATE TABLE user_preferences (
    user_id         TEXT PRIMARY KEY REFERENCES users(id),
    preferred_pods  TEXT[] NOT NULL DEFAULT '{}',
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

#### Task H2-5: Hub Notification Relay Gateway

**Priority: P1**
**Depends on: H2-4**

Implement the Hub-side WebSocket Gateway and notification push endpoint for the tiered notification system.

**Hub Gateway WebSocket** (`wss://hub.voxora.app/gateway`):

Connection lifecycle:

1. Client connects to `wss://{HUB_URL}/gateway?v=1&encoding=json`
2. Client sends IDENTIFY:

   ```json
   {
     "op": 2,
     "d": {
       "token": "<hub_access_token>",
       "subscribe_pods": ["pod_02...", "pod_03..."]
     }
   }
   ```

   `subscribe_pods` contains non-preferred pods that have `relay: true`.

3. Hub validates token, resolves user, registers in routing table
4. Hub sends READY:
   ```json
   {
     "op": 0,
     "t": "READY",
     "s": 1,
     "d": {
       "session_id": "hgw_...",
       "heartbeat_interval": 45000
     }
   }
   ```
5. Standard heartbeat loop (same protocol as Pod Gateway)

**Notification push endpoint** (Pod → Hub):

`POST /api/v1/notifications/push`

- Authenticated via Pod client credentials (Basic auth or Bearer from client_credentials grant)
- Gated: only accepted from pods with relay enabled (managed or paid plan)

```json
{
  "events": [
    {
      "user_id": "usr_01H8MZ...",
      "pod_id": "pod_01J9NX...",
      "community_id": "com_01KP...",
      "channel_id": "ch_01KP...",
      "type": "unread",
      "delta": 1,
      "has_mention": false
    }
  ]
}
```

Hub looks up each `user_id` in the routing table. If the user has an active Hub Gateway connection, forward the envelope as a DISPATCH event:

```json
{
  "op": 0,
  "t": "NOTIFICATION",
  "s": 5,
  "d": {
    "pod_id": "pod_01J9NX...",
    "community_id": "com_01KP...",
    "channel_id": "ch_01KP...",
    "type": "unread",
    "delta": 1,
    "has_mention": false
  }
}
```

**Implementation notes:**

- Routing table: `HashMap<UserId, HubGatewayConnection>` behind a `RwLock` or use `dashmap`
- Hub Gateway is much simpler than Pod Gateway — no channel subscriptions, no message fanout, just user → connection mapping and forwarding
- Rate limit: pods can push max 1000 events per minute per pod
- Hub Gateway connections are independent of Pod Gateway connections

#### Task H2-6: TURN Credential Provisioning

**Priority: P1**
**Depends on: None**

Pods request time-limited TURN credentials from the Hub so their users can establish WebRTC connections through NAT.

**Endpoint:**

`POST /api/v1/turn/credentials`

- Authenticated via Pod client credentials
- Returns ICE server configuration with HMAC-based credentials

```json
{
  "ice_servers": [
    {
      "urls": ["stun:stun.voxora.app:3478"]
    },
    {
      "urls": [
        "turn:turn.voxora.app:3478?transport=udp",
        "turn:turn.voxora.app:443?transport=tcp"
      ],
      "username": "1739232000:pod_01J9NXYK...",
      "credential": "<hmac-sha1-credential>",
      "ttl": 43200
    }
  ]
}
```

**Credential generation** (RFC 8489 long-term credentials with shared secret):

- `username` = `{unix_timestamp + ttl}:{pod_id}`
- `credential` = `base64(HMAC-SHA1(username, TURN_SHARED_SECRET))`
- This is the standard coturn REST API credential scheme

**Implementation notes:**

- `TURN_SHARED_SECRET` env var — must match the coturn server's `static-auth-secret`
- STUN server is free to provide (no auth needed); TURN requires credentials
- Pods should cache TURN credentials and refresh when TTL is < 1 hour
- For Phase 2 dev environment, run coturn in Docker alongside the other services

**docker-compose addition:**

```yaml
turn:
  image: coturn/coturn:latest
  command: >
    --no-tls
    --no-dtls
    --listening-port=3478
    --alt-listening-port=3479
    --realm=voxora.app
    --use-auth-secret
    --static-auth-secret=${TURN_SHARED_SECRET}
    --no-cli
  ports:
    - "3478:3478/udp"
    - "3478:3478/tcp"
  environment:
    TURN_SHARED_SECRET: dev-turn-secret-do-not-use-in-production
```

---

## WS-2: Pod API

### WS-2.1 Project Structure Changes

New files and directories added to `apps/pod-api/src/`:

```
apps/pod-api/src/
├── ...existing...
├── routes/
│   ├── ...existing...
│   ├── threads.rs          # Thread CRUD
│   ├── pins.rs             # Pin/unpin messages
│   ├── attachments.rs      # File upload + download
│   ├── embeds.rs           # URL embed metadata fetch
│   └── audit_log.rs        # Audit log query endpoint
├── gateway/
│   ├── ...existing...
│   ├── resume.rs           # Session resume + event replay
│   ├── typing.rs           # Typing indicator logic
│   └── presence.rs         # Presence state machine
├── voice/
│   ├── mod.rs              # Voice channel management
│   ├── signaling.rs        # WebRTC signaling via Gateway
│   ├── sfu_ipc.rs          # IPC client for voxora-sfu sidecar
│   └── state.rs            # Voice session tracking
├── media/
│   ├── mod.rs
│   ├── storage.rs          # Local FS or S3 storage backend
│   ├── thumbnail.rs        # Image thumbnail generation
│   └── proxy.rs            # Link preview proxy (OG/oEmbed fetch)
├── notifications/
│   ├── mod.rs
│   └── push.rs             # Push notification envelopes to Hub
└── migrations/
    ├── ...existing...
    ├── 20260214120000_create_attachments/
    ├── 20260214120001_add_channel_thread_fields/
    ├── 20260214120002_add_channel_voice_fields/
    ├── 20260214120003_create_voice_sessions/
    ├── 20260214120004_create_read_states/
    ├── 20260214120005_create_pod_roles/
    └── 20260214120006_create_pod_bans/
```

### WS-2.2 Tasks

#### Task P2-1: Gateway Resume & Event Replay

**Priority: P0**
**Depends on: None (extends existing Gateway)**

In Phase 1, clients reconnect by re-IDENTIFYing, which causes a full state reload. Phase 2 adds proper session resume with event replay.

**Implementation:**

1. On IDENTIFY, generate a `session_id` and store the session in an in-memory map with a replay buffer (circular buffer, max 1000 events per session)
2. Every DISPATCH event sent to a connection is also appended to that session's replay buffer with a sequence number
3. Session state persists for 5 minutes after disconnect

**RESUME flow (op 3):**

Client sends:

```json
{
  "op": 3,
  "d": {
    "session_id": "gw_01KPQRST...",
    "token": "pat_01KPQRST...",
    "seq": 42
  }
}
```

Server:

1. Look up session by `session_id`
2. Validate `token` matches session's user
3. If session found and seq is within buffer: replay all events after `seq`, send `RESUMED` dispatch, continue normally
4. If session expired or seq too old: send op 7 (RECONNECT), client must re-IDENTIFY

**op 7 RECONNECT:**

```json
{
  "op": 7,
  "d": {
    "reason": "session_expired"
  }
}
```

#### Task P2-2: Typing Indicators

**Priority: P2**
**Depends on: None (extends Gateway)**

**Client → Server:**
Client sends a Gateway command when the user starts typing:

```json
{
  "op": 0,
  "t": "TYPING",
  "d": {
    "channel_id": "ch_01KPQRST..."
  }
}
```

**Server → Client:**
Server broadcasts to all other connections subscribed to that channel:

```json
{
  "op": 0,
  "t": "TYPING_START",
  "d": {
    "channel_id": "ch_01KPQRST...",
    "user_id": "usr_01H8MZ...",
    "username": "alice",
    "timestamp": "2026-06-01T12:00:00Z"
  }
}
```

**Implementation notes:**

- No `TYPING_STOP` event — client-side timeout of 8 seconds handles this
- Rate limit: max 1 typing event per 5 seconds per user per channel
- Only broadcast to users with `VIEW_CHANNEL` permission
- Do NOT persist typing state — purely in-memory, ephemeral

#### Task P2-3: Presence System

**Priority: P2**
**Depends on: None (extends Gateway)**

Track user online/offline/idle/dnd status per pod.

**Presence states:**
| State | Trigger |
| --------- | ------- |
| `online` | Gateway connected + active |
| `idle` | No client activity for 5 minutes (client sends presence update) |
| `dnd` | Manually set by user |
| `offline` | Gateway disconnected |

**Client → Server (op 9):**

```json
{
  "op": 9,
  "d": {
    "status": "idle",
    "activities": []
  }
}
```

**Server → Client (DISPATCH):**

```json
{
  "op": 0,
  "t": "PRESENCE_UPDATE",
  "d": {
    "user_id": "usr_01H8MZ...",
    "status": "online",
    "activities": []
  }
}
```

**Implementation notes:**

- Store presence in memory per Gateway session (not in DB)
- On READY, include initial presence for online members of the user's communities
- Broadcast PRESENCE_UPDATE only to community members (users sharing at least one community)
- For large communities (>1000 online), only send presence for users in the member list sidebar (lazy presence loading)
- Rate limit: max 5 presence updates per 60 seconds per connection
- On disconnect: broadcast `offline` after a 30-second grace period (allows quick reconnect without flicker)

#### Task P2-4: Pins

**Priority: P2**
**Depends on: None**

Allow moderators to pin messages to a channel.

**Endpoints:**

1. `PUT /api/v1/channels/{channel_id}/pins/{message_id}` — Pin message
   - Requires `MANAGE_MESSAGES` permission
   - Max 50 pins per channel
   - Sets `pinned = true` on the message record
   - Broadcasts `CHANNEL_PINS_UPDATE` via Gateway
   - Creates audit log entry

2. `DELETE /api/v1/channels/{channel_id}/pins/{message_id}` — Unpin message
   - Requires `MANAGE_MESSAGES` permission
   - Sets `pinned = false`
   - Broadcasts `CHANNEL_PINS_UPDATE`

3. `GET /api/v1/channels/{channel_id}/pins` — List pinned messages
   - Returns pinned messages ordered by pin time (most recent first)
   - Requires `VIEW_CHANNEL` permission

**Gateway event:**

```json
{
  "op": 0,
  "t": "CHANNEL_PINS_UPDATE",
  "d": {
    "channel_id": "ch_01KPQRST...",
    "last_pin_timestamp": "2026-06-01T12:00:00Z"
  }
}
```

#### Task P2-5: File Attachments

**Priority: P1**
**Depends on: None**

Add file upload and attachment support for messages.

**Endpoints:**

1. `POST /api/v1/channels/{channel_id}/attachments` — Request upload URL
   - Requires `SEND_ATTACHMENTS` permission (bit 2)
   - Request:
     ```json
     {
       "filename": "screenshot.png",
       "content_type": "image/png",
       "size_bytes": 245000
     }
     ```
   - Validate: max file size (25 MB default, configurable per pod), allowed content types
   - Generate `att_` prefixed ULID for attachment ID
   - Generate pre-signed upload URL (S3) or direct upload endpoint (local FS)
   - Return:
     ```json
     {
       "attachment_id": "att_01KPQRST...",
       "upload_url": "http://localhost:4002/api/v1/media/upload/att_01KPQRST...",
       "upload_method": "PUT",
       "expires_at": "2026-06-01T12:10:00Z"
     }
     ```

2. `PUT /api/v1/media/upload/{attachment_id}` — Upload file (local FS mode)
   - Accepts raw binary body
   - Validates content-type and size against the pre-registered attachment
   - Stores file to configured storage path
   - For images: generate thumbnail (max 400×400, WebP)
   - Marks attachment as `uploaded`

3. `GET /api/v1/media/{attachment_id}/{filename}` — Serve file
   - Streams the file with proper Content-Type and Content-Disposition headers
   - Cache-Control: public, max-age=31536000 (immutable content)

**Message attachment flow:**

- Client uploads file(s), receives `attachment_id`(s)
- Client sends message with `attachments: ["att_01KPQRST..."]`
- Server validates all attachment IDs exist and are uploaded
- Server links attachments to message in DB
- `MESSAGE_CREATE` event includes full attachment objects

**Storage backends:**

- **Local FS** (default for self-hosted): files stored under `{DATA_DIR}/attachments/{att_id}/{filename}`
- **S3-compatible** (optional): configurable via `STORAGE_BACKEND=s3`, `S3_BUCKET`, `S3_ENDPOINT`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`

Implement a `StorageBackend` trait with `LocalFs` and `S3` implementations.

**DB Table:**

```sql
CREATE TABLE attachments (
    id              TEXT PRIMARY KEY,
    message_id      BIGINT REFERENCES messages(id) ON DELETE CASCADE,
    filename        TEXT NOT NULL,
    content_type    TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL,
    url             TEXT,
    thumbnail_url   TEXT,
    width           INTEGER,
    height          INTEGER,
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending, uploaded, failed
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_attachments_message ON attachments(message_id);
```

**Implementation notes:**

- Use `image` crate for thumbnail generation (resize, convert to WebP)
- Set `SEND_ATTACHMENTS` permission (bit 2) — add to `@everyone` default permissions
- Content-type allowlist: images (`image/*`), video (`video/*`), audio (`audio/*`), PDF, text, common document formats
- Strip EXIF data from images (privacy)

#### Task P2-6: URL Embeds (Link Previews)

**Priority: P2**
**Depends on: P2-5 (storage for proxy images)**

When a message contains a URL, the pod fetches Open Graph / oEmbed metadata and attaches an embed preview.

**Flow:**

1. On `MESSAGE_CREATE`, scan message content for URLs (simple regex: `https?://[^\s]+`)
2. For each URL (max 5 per message), enqueue an async job to fetch metadata
3. Fetch the URL via server-side proxy (prevents client IP leaks)
4. Parse Open Graph tags (`og:title`, `og:description`, `og:image`, `og:site_name`)
5. If OG tags not found, try oEmbed discovery
6. Store embed data on the message
7. Send `MESSAGE_UPDATE` via Gateway with the embed attached

**Embed format:**

```json
{
  "type": "link",
  "url": "https://example.com/article",
  "title": "Article Title",
  "description": "A brief description...",
  "thumbnail_url": "http://localhost:4002/api/v1/media/proxy/...",
  "site_name": "Example",
  "color": 3447003
}
```

**Implementation notes:**

- Fetch timeout: 5 seconds per URL
- Max response body to scan: 1 MB (only need HTML `<head>`)
- Proxy fetched images through the pod to avoid client IP leaks (store or cache proxied images)
- Cache embed results for 24 hours (key by URL hash)
- Do NOT fetch embeds for URLs pointing to the pod itself (self-reference loop)
- Rate limit: max 10 embed fetches per minute per channel
- Use `reqwest` for HTTP fetching, `scraper` crate for HTML parsing

#### Task P2-7: Threads

**Priority: P2**
**Depends on: P2-1 (Gateway resume, for thread events)**

Allow users to start threaded conversations from any message.

**Endpoints:**

1. `POST /api/v1/channels/{channel_id}/messages/{message_id}/threads` — Create thread
   - Requires `CREATE_THREADS` permission (bit 17)
   - Request: `{ "name": "Discussion about..." }`
   - Creates a new channel with `type = 5` (thread), `parent_id = channel_id`
   - Links thread to the parent message
   - Auto-adds thread creator as thread member
   - Broadcasts `THREAD_CREATE` via Gateway

2. `GET /api/v1/channels/{channel_id}/threads` — List active threads
   - Returns non-archived threads in the channel

3. `PATCH /api/v1/channels/{thread_id}` — Update thread
   - Thread owner or moderator can rename, archive, set auto-archive duration

4. Messages within threads use the same `POST /api/v1/channels/{thread_id}/messages` endpoint — threads are just channels with `type = 5`

**Thread archival:**

- Threads auto-archive after configurable inactivity: 1 hour, 24 hours, 3 days, 7 days (default: 24 hours)
- Run a periodic task (every 5 minutes) to check and archive inactive threads
- Archived threads are read-only until unarchived
- Sending a message to an archived thread auto-unarchives it

**DB changes:**

- Add to `channels` table: `thread_metadata JSONB` — stores `{ parent_message_id, member_count, message_count, archived, auto_archive_seconds, last_activity_at }`
- Thread members tracked in `community_members` or a separate `thread_members` table (simpler: just use channel-level tracking)

**Gateway events:**

- `THREAD_CREATE` — new thread started
- `THREAD_UPDATE` — thread renamed, archived, or unarchived
- `THREAD_MEMBERS_UPDATE` — user joined/left thread

#### Task P2-8: Audit Log

**Priority: P2**
**Depends on: None**

The audit log table already exists from Phase 1 migrations. Phase 2 adds the query endpoint and consistent logging across all moderation actions.

**Endpoint:**

`GET /api/v1/communities/{id}/audit-log`

- Requires `VIEW_AUDIT_LOG` permission (bit 20)
- Query params: `user_id`, `action`, `before`, `limit` (default 50, max 100)
- Returns audit entries with actor info, target info, and changes

**Actions to log (add logging to existing + new endpoints):**

| Action                    | Trigger                                   |
| ------------------------- | ----------------------------------------- |
| `channel.create`          | Channel created                           |
| `channel.update`          | Channel settings changed                  |
| `channel.delete`          | Channel deleted                           |
| `community.update`        | Community settings changed                |
| `member.kick`             | Member kicked                             |
| `member.ban`              | Member banned                             |
| `member.unban`            | Member unbanned                           |
| `member.role_update`      | Member roles changed                      |
| `role.create`             | Role created                              |
| `role.update`             | Role permissions/name changed             |
| `role.delete`             | Role deleted                              |
| `message.delete`          | Message deleted by moderator (not author) |
| `message.pin`             | Message pinned                            |
| `message.unpin`           | Message unpinned                          |
| `invite.create`           | Invite created                            |
| `invite.delete`           | Invite revoked                            |
| `channel_override.update` | Channel permission override changed       |

**Implementation notes:**

- Create an `audit_log::log()` helper function that takes `community_id`, `actor_id`, `action`, `target_type`, `target_id`, `changes` (JSONB diff), `reason` (optional)
- `changes` stores before/after values as JSON: `{ "name": { "old": "Foo", "new": "Bar" } }`
- Audit log entries are immutable — no edit or delete
- Retention: keep indefinitely (pod operator can configure cleanup in future)

#### Task P2-9: Advanced RBAC — Pod-Level Permissions + Channel Overrides

**Priority: P1**
**Depends on: None (extends existing permission system)**

Phase 1 RBAC is entirely community-scoped. Phase 2 adds two layers: **pod-level permissions** (who can do what on the pod itself) and **channel-level overrides** (per-channel permission tweaks within a community).

##### Pod-Level Permissions

Pod-level permissions control what users can do on the pod as a whole, outside the context of any specific community.

**Pod permission definitions:**

| Permission               | Bit | Description                                         |
| ------------------------ | --- | --------------------------------------------------- |
| `POD_CREATE_COMMUNITY`   | 0   | Can create new communities on this pod              |
| `POD_MANAGE_COMMUNITIES` | 1   | Can edit/delete any community                       |
| `POD_BAN_MEMBERS`        | 2   | Can ban users from the entire pod                   |
| `POD_MANAGE_INVITES`     | 3   | Can create pod-level invites                        |
| `POD_VIEW_AUDIT_LOG`     | 4   | Can view pod-wide audit log                         |
| `POD_MANAGE_SETTINGS`    | 5   | Can change pod settings (name, description, limits) |
| `POD_ADMINISTRATOR`      | 15  | All pod permissions (overrides all)                 |

**Pod roles:**

| Role                 | Permissions                                  | Notes                                                   |
| -------------------- | -------------------------------------------- | ------------------------------------------------------- |
| Pod Owner (implicit) | `POD_ADMINISTRATOR`                          | The user who registered the pod; always has full access |
| Pod Admin            | Configurable                                 | Assigned by pod owner                                   |
| Pod Member (default) | `POD_CREATE_COMMUNITY \| POD_MANAGE_INVITES` | Default for all users on the pod                        |

**DB Tables:**

```sql
CREATE TABLE pod_roles (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    position        INTEGER NOT NULL DEFAULT 0,
    permissions     BIGINT NOT NULL DEFAULT 0,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,  -- @everyone pod role
    color           INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE pod_member_roles (
    user_id         TEXT NOT NULL REFERENCES pod_users(id) ON DELETE CASCADE,
    role_id         TEXT NOT NULL REFERENCES pod_roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX idx_pod_member_roles_user ON pod_member_roles(user_id);
```

**Endpoints:**

1. `GET /api/v1/pod/roles` — List pod roles
2. `POST /api/v1/pod/roles` — Create pod role (requires `POD_ADMINISTRATOR`)
3. `PATCH /api/v1/pod/roles/{role_id}` — Update pod role
4. `DELETE /api/v1/pod/roles/{role_id}` — Delete pod role
5. `PUT /api/v1/pod/members/{user_id}/roles/{role_id}` — Assign pod role to user
6. `DELETE /api/v1/pod/members/{user_id}/roles/{role_id}` — Remove pod role from user
7. `PUT /api/v1/pod/bans/{user_id}` — Ban user from pod (requires `POD_BAN_MEMBERS`)
   - User is disconnected from Gateway, all community memberships on this pod are revoked
   - Banned users cannot login with SIA (reject at `POST /api/v1/auth/login`)
8. `DELETE /api/v1/pod/bans/{user_id}` — Unban user from pod
9. `GET /api/v1/pod/bans` — List pod bans

**Permission resolution:**

```
pod_permissions = union of all user's pod role permissions
if user is pod owner: pod_permissions = ALL
if pod_permissions & POD_ADMINISTRATOR: pod_permissions = ALL
```

**Enforcement points (update existing endpoints):**

- `POST /api/v1/communities` — check `POD_CREATE_COMMUNITY`
- `DELETE /api/v1/communities/{id}` — check `POD_MANAGE_COMMUNITIES` (or community owner)
- `POST /api/v1/auth/login` — check user is not pod-banned
- Pod admin endpoints — check `POD_MANAGE_SETTINGS`

**Initial setup:**
On pod registration (or first-run setup), create a default `@everyone` pod role with `POD_CREATE_COMMUNITY | POD_MANAGE_INVITES`. The pod owner gets implicit `POD_ADMINISTRATOR`.

##### Channel Permission Overrides

Phase 1 created the `channel_overrides` table but did not enforce it. Phase 2 wires it up.

**Endpoints:**

1. `PUT /api/v1/channels/{channel_id}/overrides/{target_type}/{target_id}` — Set override
   - `target_type`: `role` or `user`
   - `target_id`: role ID or user ID
   - Request: `{ "allow": 3, "deny": 0 }` (bitfields)
   - Requires `MANAGE_ROLES` permission (for role overrides) or `MANAGE_CHANNELS` (for user overrides)

2. `DELETE /api/v1/channels/{channel_id}/overrides/{target_type}/{target_id}` — Remove override

3. `GET /api/v1/channels/{channel_id}/overrides` — List all overrides for a channel

**Permission resolution update:**

Modify the existing permission computation to include channel overrides:

```
base = union of all user's role permissions
if base & ADMINISTRATOR: return ALL

channel_allow = 0
channel_deny = 0

for each role the user has:
    if override exists for (channel, role):
        channel_allow |= override.allow
        channel_deny  |= override.deny

if override exists for (channel, user):
    channel_allow |= override.allow
    channel_deny  |= override.deny

effective = (base & ~channel_deny) | channel_allow
```

User-level overrides take priority over role-level overrides (applied last).

**Implementation notes:**

- Two separate permission systems: `compute_pod_permissions(user_id)` and `compute_community_permissions(user_id, channel_id)`
- Pod permissions are checked first (e.g., can the user even create a community?), then community permissions for community-scoped actions
- All channel-scoped permission checks (`VIEW_CHANNEL`, `SEND_MESSAGES`, etc.) must now pass through the channel-aware community resolver
- Audit log: log `channel_override.update`, `pod_role.create/update/delete`, `pod_ban` actions

#### Task P2-10: Unread Counts Endpoint

**Priority: P1**
**Depends on: None**

Provide a lightweight endpoint for unread state hydration and long-poll notification fallback.

**Two use cases:**

1. **Initial connection hydration** — called immediately after receiving the Gateway `READY` event on every pod. The `READY` payload includes communities and channels but **not** read state. This endpoint fills the gap so the sidebar can render accurate unread/mention badges from the first frame.
2. **Long-poll fallback** — polled on an interval for non-preferred pods without Hub relay to detect new activity.

**Endpoint:**

`GET /api/v1/unread-counts`

- Requires PAT
- Returns unread message counts and mention counts per channel the user has access to

```json
{
  "channels": [
    {
      "channel_id": "ch_01KP...",
      "community_id": "com_01KP...",
      "unread_count": 12,
      "mention_count": 1,
      "last_message_id": 175928847299117056
    }
  ],
  "last_updated": "2026-06-01T12:00:00Z"
}
```

**Read state tracking:**

New `read_states` table:

```sql
CREATE TABLE read_states (
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_id    BIGINT NOT NULL DEFAULT 0,  -- Snowflake ID of last read message
    mention_count   INTEGER NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, channel_id)
);
```

**Marking as read:**

`PUT /api/v1/channels/{channel_id}/read`

- Request: `{ "message_id": 175928847299117056 }`
- Updates `last_read_id` for the user + channel
- Resets `mention_count` to 0 (or recomputes based on mentions after the new `last_read_id`)

**Incrementing mention count:**
When a message is created that `@mentions` a user, increment `mention_count` in their `read_states` record for that channel.

**Implementation notes:**

- This endpoint is designed to be cheap to serve — single query, no joins, no fan-out
- Response should be cacheable for 10 seconds (`Cache-Control: max-age=10`)
- The client polls this at 30–60 second intervals for non-preferred, non-relay pods

#### Task P2-11: Hub Notification Push (Pod Side)

**Priority: P1**
**Depends on: H2-5 (Hub notification relay)**

After processing a `MESSAGE_CREATE`, determine which community members need a notification via the Hub relay and push notification envelopes.

**Flow:**

1. Message is created in channel X of community Y
2. Resolve which users are members of community Y
3. Exclude users who currently have an active Pod Gateway connection (they already got `MESSAGE_CREATE` in real time)
4. For remaining users, push to Hub:
   ```
   POST {HUB_URL}/api/v1/notifications/push
   Authorization: Bearer <pod_client_credentials_token>
   ```
   With the notification event payload (see H2-5)

**Implementation notes:**

- Only push if the pod has relay enabled (check config / registration status)
- Batch events: accumulate notifications for up to 1 second, then push in one batch (max 100 events per request)
- If Hub is unreachable, drop notifications silently (they're best-effort; the client will catch up via unread counts or when connecting)
- Use the Pod's `client_id`/`client_secret` to obtain a client_credentials access token from the Hub, then use that token for the push endpoint
- Detect mentions: if the message `@mentions` a specific user, set `has_mention: true` in the notification envelope

#### Task P2-12: Voice Channels — SFU Setup

**Priority: P1**
**Depends on: H2-6 (TURN credentials)**

Integrate mediasoup as the SFU for voice (and later video) channels using the [`mediasoup` Rust crate](https://crates.io/crates/mediasoup).

**Architecture — Rust Sidecar:**

```
┌──────────┐         ┌──────────────┐         ┌──────────┐
│ Client A │◄──WS───►│   Pod API    │◄──WS───►│ Client B │
│          │         │  (signaling) │         │          │
│          │         └──────┬───────┘         │          │
│          │                │ IPC (Unix sock) │          │
│          │         ┌──────▼───────┐         │          │
│          │◄─WebRTC─┤  voxora-sfu  ├─WebRTC─►│          │
│          │         │  (Rust bin)  │         │          │
└──────────┘         └──────────────┘         └──────────┘
```

The SFU runs as a **separate Rust binary (`voxora-sfu`)** that uses the `mediasoup` Rust crate. The mediasoup crate internally spawns C++ media worker processes (one per CPU core) — so multi-core scaling is handled automatically. The sidecar binary communicates with the Pod API over a JSON-over-Unix-socket IPC protocol.

**Why a sidecar instead of in-process:**

- **Independent scaling** — run multiple SFU instances per pod, or on media-optimized hardware
- **Fault isolation** — SFU crash doesn't take down the Pod API (and vice versa)
- **Independent restarts** — upgrade the SFU without restarting the Pod API
- **Resource isolation** — media processing gets its own CPU/memory budget, won't compete with API request handling

**Sidecar project structure:**

```
apps/voxora-sfu/
├── Cargo.toml            # depends on: mediasoup, tokio, serde, serde_json
├── src/
│   ├── main.rs           # Entry point — start IPC listener, spawn mediasoup Workers
│   ├── ipc.rs            # JSON-over-Unix-socket IPC server
│   ├── router.rs         # mediasoup Router management (one per voice channel)
│   ├── transport.rs      # WebRtcTransport creation + DTLS connect
│   ├── producer.rs       # Audio/video Producer management
│   └── consumer.rs       # Consumer creation + forwarding
```

The Pod API spawns `voxora-sfu` as a child process on startup (via `tokio::process`) and connects over the Unix socket. If the SFU process crashes, the Pod API restarts it automatically.

**IPC protocol (Pod API ↔ voxora-sfu):**

JSON-delimited messages over a Unix domain socket. Each message is a newline-delimited JSON object.

| Command                   | Direction | Description                                          |
| ------------------------- | --------- | ---------------------------------------------------- |
| `create_router`           | API → SFU | Create a mediasoup Router (one per voice channel)    |
| `create_webrtc_transport` | API → SFU | Create a WebRTC transport for a user                 |
| `connect_transport`       | API → SFU | Complete DTLS handshake                              |
| `produce`                 | API → SFU | Start sending media (audio/video)                    |
| `consume`                 | API → SFU | Start receiving media from another user              |
| `close_transport`         | API → SFU | Clean up a user's transport                          |
| `close_router`            | API → SFU | Clean up a voice channel                             |
| `audio_levels`            | SFU → API | Periodic audio level updates for speaking indicators |
| `error`                   | SFU → API | Error notification (transport failure, etc.)         |

Each request carries a `request_id`; the SFU responds with a matching `request_id` + `data` or `error` payload. The `audio_levels` and `error` messages are unsolicited events pushed from SFU to API.

**Voice channel setup flow:**

1. First user joins voice channel → Pod sends `create_router` to SFU (via IPC)
2. Pod requests TURN credentials from Hub (cache and reuse)
3. For each user joining:
   a. Send `create_webrtc_transport` to SFU (via IPC) → receive transport parameters
   b. Send transport parameters + ICE servers to client (via Gateway op 5)
   c. Client creates local transport, connects
   d. Client starts producing audio → Pod sends `produce` to SFU
   e. Pod sends `consume` for existing producers (other users' audio)
4. Last user leaves → Pod sends `close_router` to SFU

**Codec configuration:**

- Audio: Opus @ 48kHz (mandatory), bitrate 32–128 kbps
- Video: VP9 preferred, VP8 fallback (Phase 3)

**SFU startup configuration (`voxora-sfu`):**

- Listens on a configurable Unix socket path (default: `/tmp/voxora-sfu.sock`)
- Number of mediasoup Workers = number of CPU cores (configurable)
- Log level inherited from Pod API environment
- Graceful shutdown: drains active voice sessions before exiting

#### Task P2-13: Voice Channels — Gateway Signaling

**Priority: P1**
**Depends on: P2-12 (SFU setup)**

Voice state management and signaling through the existing Pod Gateway.

**Client → Server — Voice State Update (op 4):**

Join voice channel:

```json
{
  "op": 4,
  "d": {
    "channel_id": "ch_01KPQRST...",
    "self_mute": false,
    "self_deaf": false
  }
}
```

Leave voice channel (null channel):

```json
{
  "op": 4,
  "d": {
    "channel_id": null
  }
}
```

**Server → Client — Voice Server Info (op 5):**

```json
{
  "op": 5,
  "d": {
    "channel_id": "ch_01KPQRST...",
    "transport_id": "transport_...",
    "ice_parameters": { "usernameFragment": "...", "password": "..." },
    "ice_candidates": [...],
    "dtls_parameters": { "role": "server", "fingerprints": [...] },
    "ice_servers": [
      { "urls": ["stun:stun.voxora.app:3478"] },
      { "urls": ["turn:turn.voxora.app:3478"], "username": "...", "credential": "..." }
    ]
  }
}
```

**Server → All — Voice State Update (DISPATCH):**

```json
{
  "op": 0,
  "t": "VOICE_STATE_UPDATE",
  "d": {
    "channel_id": "ch_01KPQRST...",
    "user_id": "usr_01H8MZ...",
    "session_id": "vs_01KPQRST...",
    "self_mute": false,
    "self_deaf": false,
    "server_mute": false,
    "server_deaf": false
  }
}
```

**SDP Negotiation over Gateway:**

After receiving op 5, the client needs to exchange SDP/DTLS/ICE data:

```json
// Client → Server: Connect transport
{
  "op": 0,
  "t": "VOICE_CONNECT",
  "d": {
    "transport_id": "transport_...",
    "dtls_parameters": { "role": "client", "fingerprints": [...] }
  }
}

// Client → Server: Start producing audio
{
  "op": 0,
  "t": "VOICE_PRODUCE",
  "d": {
    "transport_id": "transport_...",
    "kind": "audio",
    "rtp_parameters": { ... }
  }
}

// Server → Client: Consume another user's audio
{
  "op": 0,
  "t": "VOICE_CONSUME",
  "d": {
    "consumer_id": "consumer_...",
    "producer_id": "producer_...",
    "user_id": "usr_02...",
    "kind": "audio",
    "rtp_parameters": { ... }
  }
}
```

**Voice session tracking:**

```sql
CREATE TABLE voice_sessions (
    id              TEXT PRIMARY KEY,
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    session_id      TEXT NOT NULL,
    self_mute       BOOLEAN NOT NULL DEFAULT FALSE,
    self_deaf       BOOLEAN NOT NULL DEFAULT FALSE,
    server_mute     BOOLEAN NOT NULL DEFAULT FALSE,
    server_deaf     BOOLEAN NOT NULL DEFAULT FALSE,
    connected_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_voice_channel ON voice_sessions(channel_id);
CREATE INDEX idx_voice_user ON voice_sessions(user_id);
```

**Implementation notes:**

- Voice state is cleaned up when a user disconnects from the Gateway
- `user_limit` on voice channels (0 = unlimited) is enforced on join
- Server mute/deafen requires `VOICE_MUTE_OTHERS` / `VOICE_DEAFEN_OTHERS` permission
- Speaking indicator: the SFU sidecar pushes `audio_levels` events via IPC — the Pod API forwards these to clients as `VOICE_SPEAKING` dispatch events

---

## WS-3: Web Client

### WS-3.1 Project Structure Changes

New files and directories added to `apps/web-client/src/`:

```
apps/web-client/src/
├── ...existing...
├── components/
│   ├── ...existing...
│   ├── voice/
│   │   ├── voice-channel.tsx       # Voice channel component (user list + controls)
│   │   ├── voice-controls.tsx      # Mute/deafen/disconnect buttons
│   │   ├── voice-user.tsx          # Individual user in voice (avatar + speaking indicator)
│   │   └── voice-connection.tsx    # Voice connection status bar (bottom of sidebar)
│   ├── threads/
│   │   ├── thread-panel.tsx        # Side panel for active thread
│   │   ├── thread-list.tsx         # List of active threads in channel
│   │   └── thread-starter.tsx      # "Create Thread" action on message hover
│   ├── attachments/
│   │   ├── file-upload.tsx         # Drag-and-drop + paste upload
│   │   ├── attachment-preview.tsx  # Image/file preview in message
│   │   └── upload-progress.tsx     # Upload progress indicator
│   ├── embeds/
│   │   └── link-embed.tsx          # Link preview card in message
│   ├── pins/
│   │   └── pinned-messages.tsx     # Pinned messages panel
│   ├── notifications/
│   │   ├── notification-badge.tsx  # Unread count badge on sidebar
│   │   └── hub-gateway.tsx         # Hub Gateway connection manager
│   ├── mfa/
│   │   ├── totp-setup.tsx          # TOTP enrollment flow
│   │   ├── mfa-challenge.tsx       # MFA code entry during login
│   │   └── passkey-setup.tsx       # Passkey registration
│   └── settings/
│       ├── ...existing...
│       ├── mfa-settings.tsx        # MFA management in settings
│       ├── preferred-pods.tsx      # Preferred pods selector
│       └── permissions-editor.tsx  # Channel override UI for admins
├── stores/
│   ├── ...existing...
│   ├── voice.ts                    # Voice connection state per pod
│   ├── presence.ts                 # Presence state for users
│   ├── typing.ts                   # Typing indicators per channel
│   ├── threads.ts                  # Thread state
│   ├── unread.ts                   # Unified unread count store
│   └── notifications.ts           # Hub notification state
├── lib/
│   ├── ...existing...
│   ├── gateway/
│   │   ├── ...existing...
│   │   └── hub-connection.ts       # Hub Gateway connection class
│   ├── voice/
│   │   ├── webrtc.ts               # WebRTC transport management
│   │   ├── media.ts                # getUserMedia, audio processing
│   │   └── speaking.ts             # Speaking detection (audio level)
│   └── notifications/
│       └── negotiator.ts           # Auto-selects best notification method per pod
```

### WS-3.2 Tasks

#### Task C2-1: Notification System — Preferred Pods + Hub Gateway + Long Poll

**Priority: P1**
**Depends on: H2-4, H2-5**

Implement the client-side notification negotiation system.

**Initial unread hydration:**

- After receiving `READY` from any pod's Gateway, immediately call `GET /api/v1/unread-counts` on that pod
- Populate the unread store so sidebar badges are accurate from first render
- The `READY` event contains communities and channels but **not** read state — this call fills the gap

**Preferred pods connection manager:**

- On login, fetch `GET /api/v1/users/@me/preferences` to get preferred pod list
- Maintain direct WebSocket connections to all preferred pods (up to 10)
- These connections stay open regardless of which pod the user is actively viewing
- Events from preferred pods update the unread store in real time

**Hub Gateway connection:**

- On login, if any non-preferred pods have `relay: true`, connect to Hub Gateway
- Send IDENTIFY with `subscribe_pods` list
- On `NOTIFICATION` dispatch: increment unread/mention counts in the unread store
- Display badge counts on pod/community/channel icons in the sidebar

**Long poll fallback:**

- For non-preferred pods without relay, poll `GET /api/v1/unread-counts` every 30–60 seconds
- Use increasing intervals for pods with no recent activity (30s → 60s → 120s)

**Unread store (`stores/unread.ts`):**

```ts
interface UnreadState {
  // Key: `${podId}:${channelId}`
  channels: Record<
    string,
    { unread: number; mentions: number; lastMessageId: string }
  >;
  markRead: (podId: string, channelId: string, messageId: string) => void;
  increment: (
    podId: string,
    channelId: string,
    delta: number,
    hasMention: boolean,
  ) => void;
}
```

**Notification badges:**

- Channel name in sidebar: bold + unread count badge if `unread > 0`
- Channel name in sidebar: red badge if `mentions > 0`
- Community icon in sidebar: dot indicator if any channel in that community has unreads
- Pod section in sidebar: dot indicator if any community in that pod has unreads

**Preferred pods UI (settings):**

- In Settings, add a "Preferred Pods" section
- Show all connected pods with toggle switches (max 10)
- Save via `PATCH /api/v1/users/@me/preferences`
- Explain: "Preferred pods maintain a real-time connection for instant notifications"

#### Task C2-2: MFA UI

**Priority: P2**
**Depends on: H2-1, H2-2**

**TOTP setup flow (in Settings > Security):**

1. User clicks "Enable Two-Factor Auth"
2. Call `POST /api/v1/users/@me/mfa/totp/enable`
3. Display QR code (from `qr_code_data_uri`) and manual entry key
4. User enters 6-digit code from authenticator app
5. Call `POST /api/v1/users/@me/mfa/totp/confirm`
6. Display backup codes with "copy all" button and warning to save them

**MFA challenge during login:**

- After OIDC token exchange, if response contains `mfa_required: true`:
  - Show MFA code input dialog (6-digit numeric input, auto-submit on 6 chars)
  - "Use backup code" link toggles to 8-char alphanumeric input
  - Submit to `POST /api/v1/users/@me/mfa/verify` with `mfa_token`

**Passkey setup (in Settings > Security):**

1. "Add Passkey" button
2. Call `POST /api/v1/users/@me/passkeys/register/begin`
3. Invoke `navigator.credentials.create()` with the options
4. Call `POST /api/v1/users/@me/passkeys/register/complete`
5. List registered passkeys with name + last used date + delete button

**Passkey login:**

- "Sign in with Passkey" button on login page
- Call `POST /api/v1/auth/passkey/begin`
- Invoke `navigator.credentials.get()`
- Call `POST /api/v1/auth/passkey/complete`

#### Task C2-3: Typing Indicators UI

**Priority: P2**
**Depends on: P2-2 (Pod typing support)**

**Typing store (`stores/typing.ts`):**

```ts
// Key: `${podId}:${channelId}`, value: list of typing users with timestamps
typingUsers: Record<
  string,
  { userId: string; username: string; expiresAt: number }[]
>;
```

**Sending typing events:**

- On keydown in message input (debounced to every 5 seconds), send typing command via Gateway
- Stop sending when: input is empty, message is sent, or user navigates away

**Displaying typing indicators:**

- Below the message input, show typing indicator text:
  - 1 user: "Alice is typing..."
  - 2 users: "Alice and Bob are typing..."
  - 3+ users: "Alice, Bob, and 2 others are typing..."
- Animated dots (`...`) using CSS animation
- Auto-expire after 8 seconds of no `TYPING_START` event from that user

#### Task C2-4: Presence UI

**Priority: P2**
**Depends on: P2-3 (Pod presence support)**

**Presence store (`stores/presence.ts`):**

```ts
// Key: `${podId}:${userId}`, value: presence status
presence: Record<string, "online" | "idle" | "dnd" | "offline">;
```

**Displaying presence:**

- Member list sidebar: colored dot on avatar
  - `online` = green
  - `idle` = yellow (half-moon icon)
  - `dnd` = red (minus icon)
  - `offline` = gray (or hide from online members section)
- Sort member list: online → idle → dnd → offline (within each role group)

**Setting own presence:**

- Status selector in user area (bottom of sidebar): dropdown with online/idle/dnd
- Auto-idle: send `PRESENCE_UPDATE` with `idle` after 5 minutes of no mouse/keyboard activity (use `document.addEventListener('mousemove'/'keydown')` with debounce)
- On focus return: send `PRESENCE_UPDATE` with `online`

#### Task C2-5: File Upload & Attachment UI

**Priority: P1**
**Depends on: P2-5 (Pod attachment support)**

**File upload component:**

- Drag-and-drop zone on the message input area
- Paste handler: `onPaste` event, detect `DataTransfer.files`
- Click-to-upload button (paperclip icon) next to message input
- Max 10 files per message

**Upload flow:**

1. User drops/pastes/selects files
2. Show preview thumbnails above the message input (image previews, file icons for non-images)
3. For each file, call `POST /api/v1/channels/{id}/attachments` to get upload URL
4. Upload file with PUT request, show progress bar per file
5. On all uploads complete, enable the send button
6. User sends message → include `attachment_ids` in the request

**Attachment display in messages:**

- Images: inline preview (click to expand in lightbox)
- Videos: inline video player (HTML5 `<video>`)
- Audio: inline audio player
- Other files: file icon + filename + size + download link

**Upload progress component:**

- Fixed bar above message input during upload
- Per-file progress (percentage)
- Cancel button per file
- Error handling: show toast on upload failure, allow retry

#### Task C2-6: Pins UI

**Priority: P2**
**Depends on: P2-4 (Pod pins support)**

- Pin icon in channel header → opens pinned messages panel (right side overlay or slide-in panel)
- "Pin Message" in message hover actions (for users with `MANAGE_MESSAGES`)
- "Unpin Message" in pinned messages panel
- Show pin count badge on pin icon

#### Task C2-7: Threads UI

**Priority: P2**
**Depends on: P2-7 (Pod thread support)**

- "Create Thread" in message hover actions
- Opens a side panel (right side, replaces member list) with thread view
- Thread panel: thread name header + message list + message input (same components as main chat, reused via ChannelProvider with the thread's channel ID)
- Thread indicator on parent message: "3 replies" link, opens thread panel
- Active threads list: icon in channel header → dropdown showing active threads in the channel

#### Task C2-8: Voice UI

**Priority: P1**
**Depends on: P2-12, P2-13 (Pod voice support)**

**Voice channel display in sidebar:**

- Voice channels (type 1) show in channel list with a speaker icon
- Below each voice channel: list of users currently in the channel (avatar + name + speaking indicator)
- Click voice channel to join

**Voice connection flow:**

1. User clicks voice channel → send op 4 (VOICE_STATE_UPDATE) via Gateway
2. Receive op 5 (VOICE_SERVER) with transport parameters + ICE servers
3. Create WebRTC peer connection with ICE servers
4. Call `navigator.mediaDevices.getUserMedia({ audio: true })` to get microphone
5. Connect transport (DTLS), start producing audio
6. Receive `VOICE_CONSUME` events for other users → create consumers and play audio

**Voice controls bar (bottom of sidebar, visible when in a voice channel and the mute/deafen button always):**

- Current voice channel name
- Mute/Unmute button (microphone icon)
- Deafen/Undeafen button (headphone icon)
- Disconnect button (phone-down icon)
- Connection quality indicator (green/yellow/red)

**Speaking indicators:**

- Green border/glow on avatar when user is speaking
- Based on audio level data from the SFU sidecar (via `VOICE_SPEAKING` events) or local VAD

**Voice store (`stores/voice.ts`):**

```ts
interface VoiceState {
  // Current voice connection (one at a time, across pods)
  currentChannel: { podId: string; channelId: string } | null;
  selfMute: boolean;
  selfDeaf: boolean;
  // Users in voice channels per pod
  voiceStates: Record<string, VoiceUserState[]>; // key: `${podId}:${channelId}`
  speakingUsers: Set<string>;
}
```

**Implementation notes:**

- Only one voice connection at a time (across all pods) — joining a new channel disconnects from the current one
- Use `mediasoup-client` npm package for WebRTC transport management
- Audio processing: use Web Audio API for local speaker detection if needed
- Handle permission errors gracefully (microphone denied → show toast)

#### Task C2-9: Channel Permission Override UI

**Priority: P2**
**Depends on: P2-9 (Pod advanced RBAC)**

- In channel settings (accessible via edit icon on channel in sidebar), add "Permissions" tab
- Show list of role overrides + user overrides
- "Add Override" button → select role or user → toggle individual permissions on/off/inherit
- Three-state toggles: Allow (green check), Deny (red X), Inherit (gray dash)
- Save via `PUT /api/v1/channels/{id}/overrides/{type}/{target_id}`

---

## WS-4: Desktop Client (Electron)

### WS-4.1 Project Structure

```
apps/desktop/
├── package.json
├── tsconfig.json
├── electron-builder.yml          # Electron Builder configuration
├── src/
│   ├── main/
│   │   ├── index.ts              # Electron main process entry
│   │   ├── window.ts             # BrowserWindow management
│   │   ├── tray.ts               # System tray icon + menu
│   │   ├── hotkeys.ts            # Global hotkey registration
│   │   ├── notifications.ts      # electron.Notification wrapper
│   │   ├── updater.ts            # Auto-update logic (electron-updater)
│   │   ├── deeplink.ts           # voxora:// protocol handler
│   │   └── preload.ts            # Preload script (context bridge)
│   └── renderer/
│       └── index.html            # Loads the web client
├── resources/
│   ├── icon.png                  # App icon (1024x1024)
│   ├── icon.ico                  # Windows icon
│   ├── icon.icns                 # macOS icon
│   └── tray-icon.png             # System tray icon (22x22)
└── project.json                  # Nx project configuration
```

### WS-4.2 Tasks

#### Task D-1: Electron Shell Setup

**Priority: P1**
**Depends on: None**

Set up the Electron project to wrap the existing web client.

**Setup steps:**

1. Create `apps/desktop/` directory
2. Initialize package.json with `electron`, `electron-builder`, `electron-updater`
3. Main process (`src/main/index.ts`):
   - Create BrowserWindow (1280×800 default, min 800×600)
   - Load the web client — either:
     - **Dev**: `win.loadURL('http://localhost:4200')` (Vite dev server)
     - **Prod**: `win.loadFile('renderer/index.html')` (bundled web client)
   - Enable node integration: `false`, context isolation: `true`
   - Preload script for IPC bridge

4. Preload script (`src/main/preload.ts`):
   - Expose controlled IPC channels via `contextBridge.exposeInMainWorld()`
   - API surface:
     ```ts
     window.voxora = {
       platform: 'desktop',
       notifications: {
         show: (title: string, body: string, opts?: NotificationOpts) => void,
       },
       app: {
         minimize: () => void,
         maximize: () => void,
         close: () => void,
         isMaximized: () => boolean,
       },
       updater: {
         checkForUpdates: () => Promise<UpdateInfo | null>,
         downloadUpdate: () => Promise<void>,
         installUpdate: () => void,
         onUpdateAvailable: (cb: (info: UpdateInfo) => void) => void,
         onDownloadProgress: (cb: (progress: ProgressInfo) => void) => void,
       },
     }
     ```

5. **Build configuration** (`electron-builder.yml`):

   ```yaml
   appId: app.voxora.desktop
   productName: Voxora
   directories:
     output: dist
     buildResources: resources
   files:
     - "src/main/**/*"
     - "renderer/**/*"
   mac:
     category: public.app-category.social-networking
     target: [dmg, zip]
   win:
     target: [nsis, portable]
   linux:
     target: [AppImage, deb]
     category: Network;Chat
   ```

6. **Web client detection**: The web client should detect it's running in Electron via `window.voxora` existence and adjust behavior:
   - Use `electron.Notification` instead of browser notifications
   - Use custom titlebar (frameless window with CSS titlebar)
   - Enable deep link handling

#### Task D-2: System Tray

**Priority: P1**
**Depends on: D-1**

- Create tray icon with context menu:
  - "Open Voxora" — show/focus window
  - "Status" submenu → Online, Idle, Do Not Disturb
  - Separator
  - "Check for Updates"
  - "Quit Voxora"
- Close button minimizes to tray (don't quit) — configurable in settings
- Tray icon updates: show dot indicator when there are unread mentions
- On tray icon click (macOS): toggle window visibility
- On tray icon double-click (Windows/Linux): show window

#### Task D-3: Global Hotkeys

**Priority: P2**
**Depends on: D-1**

Register global keyboard shortcuts via `globalShortcut`:

| Shortcut               | Action                  |
| ---------------------- | ----------------------- |
| `Ctrl/Cmd + Shift + M` | Toggle mute (voice)     |
| `Ctrl/Cmd + Shift + D` | Toggle deafen (voice)   |
| `Ctrl/Cmd + Shift + V` | Show/hide Voxora window |

- Hotkeys should be configurable in settings
- Unregister on app quit
- Show notification toast when hotkey action fires (e.g., "Microphone Muted")

#### Task D-4: Desktop Notifications

**Priority: P1**
**Depends on: D-1**

- When a message arrives in a non-focused channel/pod, show OS notification via `electron.Notification`
- Notification content: "**Username** in #channel: message preview..."
- Click notification → focus Voxora window, navigate to that channel
- Respect Do Not Disturb status (no notifications when DND)
- Respect muted channels/communities (no notifications)
- Sound: play notification sound (optional, configurable)

#### Task D-5: Auto-Update

**Priority: P2**
**Depends on: D-1**

- Use `electron-updater` with GitHub Releases as the update source
- Check for updates on launch and every 4 hours
- Show in-app notification when update available: "Update available — v1.2.0. Restart to update."
- Download in background
- On "Restart" click: `autoUpdater.quitAndInstall()`
- Settings toggle: "Automatically download updates" (default: on)

#### Task D-6: Deep Links

**Priority: P2**
**Depends on: D-1**

Register `voxora://` protocol handler:

- `voxora://invite/{code}` — open invite acceptance flow
- `voxora://pod/{podId}/community/{communityId}/channel/{channelId}` — navigate to channel

On macOS: register via `app.setAsDefaultProtocolClient('voxora')`
On Windows: register during NSIS installation
On Linux: register via `.desktop` file

---

## WS-5: Pod Admin SPA

### WS-5.1 Overview

A lightweight admin panel built into the Pod binary itself. On first startup (no registration), it serves a setup wizard. After setup, it provides a dashboard for pod operators to manage their pod without touching config files or making raw API calls.

The admin SPA is embedded directly in the Pod binary via `rust-embed` — no separate deployment or build step for operators. It's served at `/admin` on the same port as the Pod API.

### WS-5.2 Project Structure

```
apps/pod-admin/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── index.html
├── src/
│   ├── main.tsx                    # Entry point
│   ├── routes/
│   │   ├── __root.tsx              # Root layout
│   │   ├── setup.tsx               # First-run setup wizard
│   │   ├── _authenticated.tsx      # Auth guard (pod owner only)
│   │   ├── _authenticated/
│   │   │   ├── index.tsx           # Dashboard overview
│   │   │   ├── communities.tsx     # Community list + stats
│   │   │   ├── verification.tsx    # Verification status + management
│   │   │   ├── storage.tsx         # Storage settings + usage
│   │   │   ├── logs.tsx            # Audit log viewer
│   │   │   └── settings.tsx        # Pod settings (name, description, limits)
│   ├── components/
│   │   ├── ui/                     # shadcn/ui primitives (minimal set)
│   │   ├── setup/
│   │   │   ├── welcome-step.tsx    # "Welcome to your new Pod"
│   │   │   ├── hub-connect.tsx     # Hub URL + OAuth login
│   │   │   ├── pod-register.tsx    # Name, description, register with Hub
│   │   │   └── complete.tsx        # "Your pod is live!"
│   │   └── dashboard/
│   │       ├── stats-cards.tsx     # Member count, storage, uptime
│   │       ├── community-table.tsx # Community list
│   │       └── verification-checklist.tsx
│   ├── stores/
│   │   └── admin.ts               # Admin auth state + pod config
│   └── lib/
│       ├── api.ts                  # Pod admin API client
│       └── utils.ts
└── dist/                           # Built output, embedded into Pod binary
```

### WS-5.3 Embedding in Pod Binary

The built SPA (`apps/pod-admin/dist/`) is embedded into the Rust binary using `rust-embed`:

```rust
#[derive(RustEmbed)]
#[folder = "../../apps/pod-admin/dist/"]
struct AdminAssets;

// Serve at /admin/*
async fn admin_handler(path: &str) -> impl IntoResponse {
    match AdminAssets::get(path) {
        Some(content) => { /* serve with correct content-type */ },
        None => { /* serve index.html for SPA routing */ },
    }
}
```

In development, proxy `/admin` to the Vite dev server (`http://localhost:4300`) for hot reload.

### WS-5.4 Tasks

#### Task A-1: Pod Admin API Endpoints

**Priority: P1**
**Depends on: None (extends Pod API)**

Add admin-only endpoints to the Pod API. All require the requesting user to be the pod owner (check `owner_id` from pod registration).

**Endpoints:**

1. `GET /api/v1/admin/status` — Pod status overview
   - Returns:
     ```json
     {
       "pod_id": "pod_01J9NX...",
       "name": "Alice's Gaming Pod",
       "registered": true,
       "verified": false,
       "uptime_seconds": 86400,
       "member_count": 4521,
       "online_count": 312,
       "community_count": 8,
       "storage_used_bytes": 5368709120,
       "storage_limit_bytes": 10737418240,
       "version": "1.2.0"
     }
     ```

2. `GET /api/v1/admin/setup-status` — Check if pod needs first-run setup
   - Returns `{ "needs_setup": true }` if pod has no registration credentials
   - Used by the admin SPA to decide whether to show the setup wizard or dashboard

3. `POST /api/v1/admin/setup` — Complete first-run registration
   - Request:
     ```json
     {
       "hub_url": "https://hub.voxora.app",
       "owner_sia": "<signed-jwt>",
       "pod_name": "Alice's Gaming Pod",
       "pod_description": "A community for gamers",
       "public": true
     }
     ```
   - Pod uses the owner's SIA to authenticate with the Hub
   - Pod calls `POST {hub_url}/api/v1/pods/register` on behalf of the owner
   - Stores returned `pod_id`, `client_id`, `client_secret` in its config/database
   - Starts heartbeat loop
   - Returns `{ "pod_id": "pod_01...", "status": "active" }`

4. `PATCH /api/v1/admin/settings` — Update pod settings
   - Updatable: name, description, icon_url, public, max file size, storage backend
   - Syncs name/description changes to Hub via `PATCH /api/v1/pods/{pod_id}`

5. `GET /api/v1/admin/storage` — Storage usage breakdown
   - Returns per-community storage usage, total used vs limit

6. `POST /api/v1/admin/verification/start` — Initiate pod verification
   - Calls Hub `POST /api/v1/pods/{pod_id}/verify`
   - Returns verification challenge token + instructions

7. `POST /api/v1/admin/verification/check` — Trigger verification check
   - Calls Hub `POST /api/v1/pods/{pod_id}/verify/domain`
   - Returns current verification status

8. `GET /api/v1/admin/verification` — Get verification status
   - Returns checklist with pass/fail status for each check

**Authentication:**

- During first-run setup (no registration yet): endpoints are open but rate-limited (only `/admin/setup-status` and `/admin/setup`)
- After registration: all `/admin/*` endpoints require PAT from the pod owner (validated by checking `user_id == pod.owner_id`)

#### Task A-2: First-Run Setup Wizard

**Priority: P1**
**Depends on: A-1**

The setup wizard runs when the pod starts with no registration credentials.

**Flow:**

1. **Welcome screen**: "Welcome to Voxora Pod Setup" — brief explanation of what a pod is
2. **Hub connection**: User enters Hub URL (default: `https://hub.voxora.app`), clicks "Connect" → redirects to Hub OAuth login
3. **Hub OAuth callback**: Pod receives authorization code, exchanges for tokens, gets user identity
4. **Pod details**: User enters pod name, description, selects visibility (public/private)
5. **Registration**: Pod registers itself with the Hub, receives credentials, stores them
6. **Complete**: "Your pod is live! Share this URL to let people join." Shows the pod URL and a link to the admin dashboard

**Implementation notes:**

- The setup wizard is a multi-step form (wizard pattern)
- OAuth flow: Pod acts as a temporary OAuth client — it redirects the admin's browser to Hub `/oidc/authorize`, receives the callback at `/admin/callback`, and uses the resulting SIA to register
- After setup completes, the wizard route redirects to the dashboard and is no longer accessible
- Pod stores credentials in a local config file or database table (`pod_config`)

#### Task A-3: Admin Dashboard

**Priority: P2**
**Depends on: A-1, A-2**

Post-setup dashboard for ongoing pod management.

**Dashboard pages:**

1. **Overview** (`/admin`):
   - Stats cards: members, online users, communities, storage used, uptime
   - Quick actions: "Create Community", "View Audit Log", "Start Verification"
   - Pod status indicator (connected to Hub, heartbeat healthy)

2. **Communities** (`/admin/communities`):
   - Table: community name, member count, channel count, created date
   - Click to expand: channel list, owner info
   - No CRUD here — community management happens in the main Voxora client

3. **Verification** (`/admin/verification`):
   - Current status badge (unverified / pending / verified)
   - Checklist UI showing each verification requirement with pass/fail
   - DNS TXT record instructions with copy button
   - "Check Now" button to trigger re-verification

4. **Storage** (`/admin/storage`):
   - Usage bar (used / limit)
   - Per-community breakdown
   - Storage backend config (local path or S3 settings)

5. **Settings** (`/admin/settings`):
   - Pod name, description, icon
   - Max upload size
   - Public/private toggle
   - Hub connection status

6. **Logs** (`/admin/logs`):
   - Recent audit log entries (last 100)
   - Filter by action type
   - Links to the community audit log endpoint

**Implementation notes:**

- Keep the admin SPA small — use minimal shadcn components, no heavy dependencies
- Build output should be < 500 KB gzipped to keep the Pod binary lean
- Use `@tanstack/react-router` for routing (same as web client, familiar patterns)
- Tailwind for styling (same as web client)

#### Task A-4: Embed Admin SPA in Pod Binary

**Priority: P1**
**Depends on: A-2 (at minimum the setup wizard must be built)**

- Add `rust-embed` to Pod API dependencies
- Configure the embed to include `apps/pod-admin/dist/`
- Add Axum route handler for `/admin/*` that serves embedded files
- SPA fallback: serve `index.html` for any path under `/admin/` that doesn't match a static file
- In dev mode: proxy `/admin` to Vite dev server via `ADMIN_DEV_URL` env var
- Add build step to Nx: build pod-admin before building pod-api

**Build integration:**

Add to `apps/pod-api/project.json`:

```json
{
  "targets": {
    "build": {
      "dependsOn": ["pod-admin:build"]
    }
  }
}
```

---

## 9. Database Migrations

### Hub Database (new migrations)

```sql
-- 20260214120000_create_passkeys/up.sql
CREATE TABLE passkeys (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id   BYTEA NOT NULL UNIQUE,
    public_key      BYTEA NOT NULL,
    sign_count      BIGINT NOT NULL DEFAULT 0,
    name            TEXT NOT NULL,
    transports      TEXT[],
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ
);
CREATE INDEX idx_passkeys_user ON passkeys(user_id);

-- 20260214120001_create_mfa_backup_codes/up.sql
CREATE TABLE mfa_backup_codes (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash       TEXT NOT NULL,
    used            BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_backup_codes_user ON mfa_backup_codes(user_id);

-- 20260214120002_add_user_mfa_fields/up.sql
ALTER TABLE users ADD COLUMN mfa_enabled BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN mfa_secret TEXT;  -- TOTP secret, encrypted

-- 20260214120003_create_pod_verifications/up.sql
CREATE TABLE pod_verifications (
    id              TEXT PRIMARY KEY,
    pod_id          TEXT NOT NULL REFERENCES pods(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending',
    domain_proof    JSONB,
    security_check  JSONB,
    notes           TEXT,
    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at     TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ
);
CREATE INDEX idx_pod_verifications_pod ON pod_verifications(pod_id);

-- 20260214120004_add_pod_verification_field/up.sql
ALTER TABLE pods ADD COLUMN verification TEXT NOT NULL DEFAULT 'unverified';

-- 20260214120005_create_user_preferences/up.sql
CREATE TABLE user_preferences (
    user_id         TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    preferred_pods  TEXT[] NOT NULL DEFAULT '{}',
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Pod Database (new migrations)

```sql
-- 20260214120000_create_attachments/up.sql
CREATE TABLE attachments (
    id              TEXT PRIMARY KEY,
    message_id      BIGINT REFERENCES messages(id) ON DELETE CASCADE,
    filename        TEXT NOT NULL,
    content_type    TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL,
    url             TEXT,
    thumbnail_url   TEXT,
    width           INTEGER,
    height          INTEGER,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_attachments_message ON attachments(message_id);

-- 20260214120001_add_channel_thread_fields/up.sql
ALTER TABLE channels ADD COLUMN thread_metadata JSONB;
-- thread_metadata: { parent_message_id, member_count, message_count, auto_archive_seconds, last_activity_at }

-- 20260214120002_add_channel_voice_fields/up.sql
ALTER TABLE channels ADD COLUMN user_limit INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN bitrate INTEGER NOT NULL DEFAULT 64000;

-- 20260214120003_create_voice_sessions/up.sql
CREATE TABLE voice_sessions (
    id              TEXT PRIMARY KEY,
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    session_id      TEXT NOT NULL,
    self_mute       BOOLEAN NOT NULL DEFAULT FALSE,
    self_deaf       BOOLEAN NOT NULL DEFAULT FALSE,
    server_mute     BOOLEAN NOT NULL DEFAULT FALSE,
    server_deaf     BOOLEAN NOT NULL DEFAULT FALSE,
    connected_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_voice_channel ON voice_sessions(channel_id);
CREATE INDEX idx_voice_user ON voice_sessions(user_id);

-- 20260214120004_create_read_states/up.sql
CREATE TABLE read_states (
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_id    BIGINT NOT NULL DEFAULT 0,
    mention_count   INTEGER NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, channel_id)
);

-- 20260214120005_create_pod_roles/up.sql
CREATE TABLE pod_roles (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    position        INTEGER NOT NULL DEFAULT 0,
    permissions     BIGINT NOT NULL DEFAULT 0,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    color           INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE pod_member_roles (
    user_id         TEXT NOT NULL REFERENCES pod_users(id) ON DELETE CASCADE,
    role_id         TEXT NOT NULL REFERENCES pod_roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX idx_pod_member_roles_user ON pod_member_roles(user_id);

-- 20260214120006_create_pod_bans/up.sql
CREATE TABLE pod_bans (
    user_id         TEXT PRIMARY KEY REFERENCES pod_users(id),
    banned_by       TEXT NOT NULL REFERENCES pod_users(id),
    reason          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## 10. New Dependencies

### Hub API — New Rust Crates

| Crate                | Purpose                                          |
| -------------------- | ------------------------------------------------ |
| `totp-rs`            | TOTP code generation + validation                |
| `webauthn-rs`        | WebAuthn/Passkey registration + auth             |
| `hmac` + `sha1`      | TURN credential generation (HMAC-SHA1)           |
| `dashmap`            | Concurrent HashMap for Hub Gateway routing table |
| `trust-dns-resolver` | DNS TXT record lookup for pod verification       |

### Pod API — New Rust Crates

| Crate                    | Purpose                                              |
| ------------------------ | ---------------------------------------------------- |
| `image`                  | Image thumbnail generation (resize, WebP conversion) |
| `scraper`                | HTML parsing for URL embed (OG tag extraction)       |
| `tokio::process`         | Spawn + supervise voxora-sfu sidecar process         |
| `tokio::net::UnixStream` | IPC communication with voxora-sfu                    |
| `rust-embed`             | Embed Pod Admin SPA static files in binary           |

### Voxora SFU Sidecar — Rust Crates

| Crate                      | Purpose                                                             |
| -------------------------- | ------------------------------------------------------------------- |
| `mediasoup`                | SFU core library (Rust crate, manages C++ media workers internally) |
| `tokio`                    | Async runtime                                                       |
| `serde` + `serde_json`     | IPC message serialization                                           |
| `tokio::net::UnixListener` | IPC server (Unix socket)                                            |
| `tracing`                  | Structured logging (same as Pod API)                                |

### Web Client — New npm Packages

| Package                   | Purpose                                       |
| ------------------------- | --------------------------------------------- |
| `mediasoup-client`        | WebRTC transport management (client-side SFU) |
| `@simplewebauthn/browser` | WebAuthn browser helpers                      |

### Pod Admin SPA — npm Packages

| Package                  | Purpose                                   |
| ------------------------ | ----------------------------------------- |
| `react` + `react-dom`    | UI framework                              |
| `@tanstack/react-router` | Routing (same as web client)              |
| `tailwindcss`            | Styling (same as web client)              |
| `vite`                   | Build tool                                |
| shadcn/ui (minimal set)  | Button, Input, Card, Table, Badge, Dialog |

### Desktop Client — npm Packages

| Package            | Purpose                               |
| ------------------ | ------------------------------------- |
| `electron`         | Desktop shell                         |
| `electron-builder` | Build + package (dmg, nsis, AppImage) |
| `electron-updater` | Auto-update from GitHub Releases      |

---

## 11. Integration Test Plan

### Test 1: MFA Login Flow

1. Register user, enable TOTP MFA
2. Attempt login → verify `mfa_required` response
3. Submit valid TOTP code → verify full token set returned
4. Submit invalid code → verify rejection + rate limit
5. Use backup code → verify success + code consumed

### Test 2: Voice Channel Round-Trip

1. Two users authenticate and connect to same pod
2. User A joins voice channel
3. Verify `VOICE_STATE_UPDATE` sent to User B
4. User B joins voice channel
5. Verify WebRTC transports created for both users
6. Verify audio flows (produce + consume)
7. User A mutes → verify `VOICE_STATE_UPDATE` received by User B
8. User A disconnects → verify cleanup

### Test 3: File Attachment Flow

1. Request upload URL via `POST /channels/{id}/attachments`
2. Upload file via PUT
3. Send message with `attachment_ids`
4. Verify `MESSAGE_CREATE` event includes attachment objects
5. Fetch attachment via `GET /media/{id}/{filename}` → verify file content
6. For images: verify thumbnail was generated

### Test 4: Thread Flow

1. Send a message in a channel
2. Create thread from the message
3. Send messages in the thread
4. Verify `THREAD_CREATE` and `MESSAGE_CREATE` events
5. Verify thread appears in active threads list
6. Wait for auto-archive → verify thread is archived

### Test 5: Notification Negotiation

1. User has 2 preferred pods and 1 non-preferred pod (no relay)
2. Verify direct WS connections to both preferred pods
3. Verify long-poll to non-preferred pod
4. Message arrives on preferred pod → verify real-time delivery
5. Message arrives on non-preferred pod → verify unread count updates within poll interval

### Test 6: Pod Verification

1. Register a pod
2. Submit verification request
3. Set up DNS TXT record (mock in test)
4. Trigger domain check → verify success
5. Verify `verification` field updates to `verified`

### Test 7: Gateway Resume

1. Connect to Gateway, receive events (capture seq numbers)
2. Disconnect (simulate network drop)
3. Reconnect within 5 minutes with RESUME
4. Verify missed events replayed
5. Disconnect, wait >5 minutes, reconnect → verify RECONNECT (op 7) sent

### Test 8: Pod Admin Setup Wizard

1. Start pod with no registration credentials
2. Verify `/admin` serves the setup wizard
3. Complete Hub OAuth login flow
4. Register pod with Hub (name, description)
5. Verify pod credentials stored and heartbeat starts
6. Verify `/admin` now shows the dashboard (not wizard)
7. Verify pod appears in Hub pod registry

### Test 9: Channel Permission Overrides

1. Create channel with default permissions
2. Add role override: deny `SEND_MESSAGES` for @everyone
3. Verify member cannot send messages
4. Add user override: allow `SEND_MESSAGES` for specific user
5. Verify that user CAN send messages (user override > role override)

---

## 12. Task Dependency Graph

```
WS-1: Hub API
──────────────────────────────────────────────────────────
H2-1 (MFA TOTP) ─────────────────────────── C2-2 (MFA UI)
H2-2 (Passkeys) ─────────────────────────── C2-2
H2-3 (Pod Verification) ──────── (standalone)
H2-4 (Preferred Pods API) ───┐
                              ├── H2-5 (Hub Gateway + Relay) ── C2-1 (Notification UI)
                              │
H2-6 (TURN Credentials) ─────┼── P2-12 (Voice SFU Setup)
                              │

WS-2: Pod API
──────────────────────────────────────────────────────────
P2-1  (Gateway Resume) ──────── (standalone, extends Gateway)
P2-2  (Typing Indicators) ───── C2-3 (Typing UI)
P2-3  (Presence) ─────────────── C2-4 (Presence UI)
P2-4  (Pins) ─────────────────── C2-6 (Pins UI)
P2-5  (Attachments) ──────────── C2-5 (Upload UI)
P2-6  (Embeds) ────── depends on P2-5
P2-7  (Threads) ──────────────── C2-7 (Thread UI)
P2-8  (Audit Log) ──── (standalone)
P2-9  (Advanced RBAC) ────────── C2-9 (Override UI)
P2-10 (Unread Counts) ────────── C2-1 (Notification UI)
P2-11 (Hub Push) ──── depends on H2-5
P2-12 (Voice SFU) ──── depends on H2-6
P2-13 (Voice Signaling) ──── depends on P2-12 ──── C2-8 (Voice UI)

WS-3: Web Client
──────────────────────────────────────────────────────────
C2-1 (Notifications) ──── depends on H2-4, H2-5, P2-10
C2-2 (MFA UI) ──── depends on H2-1, H2-2
C2-3 (Typing UI) ──── depends on P2-2
C2-4 (Presence UI) ──── depends on P2-3
C2-5 (Upload UI) ──── depends on P2-5
C2-6 (Pins UI) ──── depends on P2-4
C2-7 (Threads UI) ──── depends on P2-7
C2-8 (Voice UI) ──── depends on P2-12, P2-13
C2-9 (Override UI) ──── depends on P2-9

WS-4: Desktop Client
──────────────────────────────────────────────────────────
D-1 (Electron Shell) ──── (standalone)
D-2 (System Tray) ──── depends on D-1
D-3 (Global Hotkeys) ──── depends on D-1
D-4 (Desktop Notifications) ──── depends on D-1
D-5 (Auto-Update) ──── depends on D-1
D-6 (Deep Links) ──── depends on D-1

WS-5: Pod Admin SPA
──────────────────────────────────────────────────────────
A-1 (Admin API Endpoints) ──── (standalone, extends Pod API)
A-2 (Setup Wizard) ──── depends on A-1
A-3 (Admin Dashboard) ──── depends on A-1, A-2
A-4 (Embed in Binary) ──── depends on A-2
```

### Recommended Implementation Order

**Sprint 1 (Weeks 1–3): Foundation + Quick Wins**

Independent tasks that unblock everything else:

- P2-1 (Gateway Resume) — improves reliability for all subsequent testing
- P2-4 (Pins) — small, standalone
- P2-2 (Typing Indicators) — small, standalone
- P2-8 (Audit Log endpoint) — wires up existing table
- H2-4 (Preferred Pods API) — simple CRUD
- H2-6 (TURN Credentials) — needed to unblock voice
- D-1 (Electron Shell) — start desktop in parallel
- A-1 (Pod Admin API) — unblocks setup wizard

**Sprint 2 (Weeks 4–6): Notifications + Attachments + RBAC**

- H2-5 (Hub Gateway + Notification Relay)
- P2-10 (Unread Counts)
- P2-11 (Hub Notification Push)
- C2-1 (Notification UI — preferred pods, Hub GW, long poll, badges)
- P2-5 (File Attachments)
- C2-5 (Upload UI)
- P2-9 (Advanced RBAC — channel overrides)
- D-2 (System Tray)
- D-4 (Desktop Notifications)
- A-2 (Setup Wizard)
- A-4 (Embed in Binary)

**Sprint 3 (Weeks 7–10): Voice**

- P2-12 (Voice SFU — mediasoup setup)
- P2-13 (Voice Signaling)
- C2-8 (Voice UI)
- P2-3 (Presence)
- C2-4 (Presence UI)
- P2-6 (Embeds)
- D-3 (Global Hotkeys — voice mute/deafen)
- A-3 (Admin Dashboard — verification, storage, logs)

**Sprint 4 (Weeks 11–14): MFA + Threads + Polish**

- H2-1 (MFA TOTP)
- H2-2 (Passkeys)
- C2-2 (MFA UI)
- P2-7 (Threads)
- C2-7 (Thread UI)
- H2-3 (Pod Verification)
- C2-3 (Typing UI)
- C2-6 (Pins UI)
- C2-9 (Channel Override UI)

**Sprint 5 (Weeks 15–16): Desktop Polish + Integration**

- D-5 (Auto-Update)
- D-6 (Deep Links)
- Integration tests (all 8 test plans)
- Cross-platform desktop testing (macOS, Windows, Linux)
- Performance testing (voice latency, Gateway resume, notification delivery)
- Bug fixes and polish

---

## 13. Out of Scope for Phase 2

These items are explicitly deferred to later phases. Do NOT implement them:

- Video channels / screen sharing — Phase 3
- Mobile clients (iOS/Android) — Phase 3
- Mobile push notifications (APNs/FCM) — Phase 3
- Billing / Stripe integration — Phase 3
- Managed Pod provisioning — Phase 3
- Social login (GitHub/Google/Apple) — Phase 3
- Forum / Stage / Announcement channels — Phase 3
- Bot API — Phase 3
- E2EE for DMs — Phase 4
- Custom emoji / Stickers — Phase 4
- Rich presence / Activities — Phase 4
- Pod-to-Pod DMs — Phase 4
- Hub Admin SPA (managed pod provisioning, billing dashboard, platform analytics) — Phase 3
- Developer portal / Marketplace — Phase 4
- Self-hosted Hub — Phase 4

---

_End of Phase 2 Implementation Guide_
