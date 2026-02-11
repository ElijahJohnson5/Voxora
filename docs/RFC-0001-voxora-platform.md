# RFC-0001: Voxora Platform Architecture

| Field       | Value                                    |
| ----------- | ---------------------------------------- |
| **Title**   | Voxora: Federated Communication Platform |
| **Status**  | Draft                                    |
| **Authors** | Voxora Core Team                         |
| **Created** | 2026-02-10                               |
| **Updated** | 2026-02-10                               |

---

## Table of Contents

1. [Abstract](#1-abstract)
2. [Motivation](#2-motivation)
3. [Terminology](#3-terminology)
4. [System Overview](#4-system-overview)
5. [Identity & Authentication](#5-identity--authentication)
6. [Hub ↔ Pod Federation Protocol](#6-hub--pod-federation-protocol)
7. [Pod Authentication & Authorization](#7-pod-authentication--authorization)
8. [Real-Time Messaging](#8-real-time-messaging)
9. [Voice & Video](#9-voice--video)
10. [Data Model](#10-data-model)
11. [Hub API Specification](#11-hub-api-specification)
12. [Pod API Specification](#12-pod-api-specification)
13. [WebSocket Gateway Protocol](#13-websocket-gateway-protocol)
14. [Pod Verification & Trust](#14-pod-verification--trust)
15. [Managed Pods & Billing](#15-managed-pods--billing)
16. [Client Architecture](#16-client-architecture)
17. [Security](#17-security)
18. [Privacy & Compliance](#18-privacy--compliance)
19. [Operational Requirements](#19-operational-requirements)
20. [Migration & Versioning](#20-migration--versioning)
21. [Development Phases](#21-development-phases)
22. [Open Questions](#22-open-questions)
23. [References](#23-references)

---

## 1. Abstract

Voxora is a federated real-time communication platform comprising a central
identity authority (the **Hub**), independently operated servers (the **Pods**),
and cross-platform client applications. Users authenticate once through the Hub
and can freely join any Pod without creating separate accounts. Pods store
messages, run voice/video infrastructure, and enforce local moderation policy.
The Hub provides OIDC-based identity, a pod registry, verification services,
and billing for managed hosting.

This RFC specifies the complete protocol, data model, API surface, security
model, and operational requirements for building the Voxora platform.

---

## 2. Motivation

Existing platforms (Discord, Slack, Teams) are centralized: a single operator
controls all data, moderation policy, and availability. This creates:

- **Single points of failure** — an outage takes down all communities.
- **Policy monoculture** — one moderation policy for all communities.
- **Data custody risk** — users cannot control where their data lives.
- **Vendor lock-in** — no portability of communities or history.

Fully decentralized protocols (Matrix, XMPP) solve some of these problems but
introduce identity fragmentation: a user on server A has no verified
relationship with the same person on server B, making impersonation trivial
across servers.

Voxora takes a **federated-with-central-identity** approach:

- Identity is global and cryptographically verified through the Hub.
- Data sovereignty is preserved: Pod operators control their own storage and
  policy.
- User experience is seamless: one login, many communities.
- Trust is explicit: verified Pods carry a badge, unverified Pods carry a
  warning.

---

## 3. Terminology

| Term          | Definition                                                                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Hub**       | The central Voxora service. Provides OIDC, user profiles, pod registry, verification, billing. There is exactly one Hub.                         |
| **Pod**       | An independently operated server that hosts communities, stores messages, and runs real-time services. Anyone can run a Pod.                     |
| **Community** | A named group space on a Pod, analogous to a Discord server or guild. A single Pod can host many Communities.                                    |
| **Channel**   | A communication context within a Community. Types: text, voice, announcement, forum, stage.                                                      |
| **SIA**       | Signed Identity Assertion. A compact, signed JWT issued by the Hub that attests to a user's identity. Used by clients to prove identity to Pods. |
| **PAT**       | Pod Access Token. A session token issued by a Pod after validating a SIA. Scoped to that Pod's APIs.                                             |
| **SFU**       | Selective Forwarding Unit. A WebRTC server that receives media streams and selectively forwards them to participants.                            |
| **TURN**      | Traversal Using Relays around NAT. A relay server for WebRTC when direct P2P fails.                                                              |
| **Gateway**   | The WebSocket endpoint on a Pod that delivers real-time events to clients.                                                                       |

---

## 4. System Overview

### 4.1 Architecture Diagram (Logical)

```
┌─────────────────────────────────────────────────────────┐
│                        CLIENTS                          │
│   ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│   │   Web    │  │ Desktop  │  │  Mobile  │             │
│   │  (SPA)   │  │ (Tauri)  │  │(iOS/And) │             │
│   └────┬─────┘  └────┬─────┘  └────┬─────┘             │
│        │              │              │                   │
│        └──────────────┼──────────────┘                   │
│                       │                                  │
│              ┌────────▼────────┐                         │
│              │  OIDC Login +   │                         │
│              │  SIA Issuance   │                         │
│              └────────┬────────┘                         │
└───────────────────────┼─────────────────────────────────┘
                        │
           ┌────────────▼────────────┐
           │          HUB            │
           │  ┌──────────────────┐   │
           │  │  OIDC Provider   │   │
           │  │  User Profiles   │   │
           │  │  Pod Registry    │   │
           │  │  Verification    │   │
           │  │  Billing         │   │
           │  │  JWKS Endpoint   │   │
           │  └──────────────────┘   │
           └──────┬──────────┬───────┘
                  │          │
        ┌─────────▼──┐  ┌───▼─────────┐
        │   Pod A    │  │   Pod B     │
        │ ┌────────┐ │  │ ┌────────┐  │
        │ │REST API│ │  │ │REST API│  │
        │ │Gateway │ │  │ │Gateway │  │
        │ │  SFU   │ │  │ │  SFU   │  │
        │ │Storage │ │  │ │Storage │  │
        │ └────────┘ │  │ └────────┘  │
        └────────────┘  └─────────────┘
```

### 4.2 Trust Model

```
                    ┌─────────┐
                    │   Hub   │  ← Root of Trust
                    │  (JWKS) │
                    └────┬────┘
                         │ signs SIAs
              ┌──────────┼──────────┐
              ▼          ▼          ▼
          ┌──────┐  ┌──────┐  ┌──────┐
          │Pod A │  │Pod B │  │Pod C │
          └──────┘  └──────┘  └──────┘
              │ No direct trust between Pods │
```

- The Hub is the **sole trust root**. It holds the signing keys.
- Pods **never trust each other** directly. There is no Pod-to-Pod
  communication.
- Clients authenticate at the Hub and carry proof (SIA) to Pods.
- Pods verify SIAs using the Hub's published JWKS. No shared secrets.

### 4.3 Connection Lifecycle

```
Client                     Hub                      Pod
  │                         │                        │
  │──── OIDC Login ────────►│                        │
  │◄─── access_token + SIA ─│                        │
  │                         │                        │
  │──── Present SIA ────────┼───────────────────────►│
  │                         │◄── (optional) introspect│
  │                         │──── OK ───────────────►│
  │◄─── PAT + WS ticket ───┼────────────────────────│
  │                         │                        │
  │──── WS Connect (ticket) ┼───────────────────────►│
  │◄─── READY event ────────┼────────────────────────│
  │                         │                        │
  │◄───► real-time events ──┼───────────────────────►│
```

---

## 5. Identity & Authentication

### 5.1 Hub as OIDC Provider

The Hub implements a full OpenID Connect Provider conforming to:

- **OAuth 2.1** (draft-ietf-oauth-v2-1)
- **OpenID Connect Core 1.0**
- **OpenID Connect Discovery 1.0**

#### 5.1.1 Endpoints

| Endpoint      | Path                                    | Purpose                                |
| ------------- | --------------------------------------- | -------------------------------------- |
| Authorization | `GET /oidc/authorize`                   | Start auth code flow                   |
| Token         | `POST /oidc/token`                      | Exchange code for tokens               |
| UserInfo      | `GET /oidc/userinfo`                    | Retrieve user claims                   |
| JWKS          | `GET /oidc/.well-known/jwks.json`       | Public signing keys                    |
| Discovery     | `GET /.well-known/openid-configuration` | Provider metadata                      |
| Revocation    | `POST /oidc/revoke`                     | Revoke tokens                          |
| Introspection | `POST /oidc/introspect`                 | Token introspection (for Pods)         |
| End Session   | `POST /oidc/logout`                     | Logout                                 |
| Device Auth   | `POST /oidc/device`                     | Device flow for TV/CLI                 |
| Registration  | `POST /oidc/register`                   | Dynamic client registration (for Pods) |

#### 5.1.2 Supported Flows

| Flow                      | Use Case                     |
| ------------------------- | ---------------------------- |
| Authorization Code + PKCE | Web, Desktop, Mobile clients |
| Device Authorization      | CLI tools, smart TV          |
| Client Credentials        | Pod ↔ Hub server-to-server   |
| Refresh Token             | Silent token renewal         |

PKCE is **required** for all public clients. `plain` challenge method is
**not** supported; only `S256`.

#### 5.1.3 Token Types

| Token         | Format           | Lifetime          | Audience           |
| ------------- | ---------------- | ----------------- | ------------------ |
| Access Token  | Opaque reference | 15 minutes        | Hub APIs           |
| Refresh Token | Opaque reference | 30 days (sliding) | Hub token endpoint |
| ID Token      | JWT (signed)     | 15 minutes        | Client             |
| SIA           | JWT (signed)     | 5 minutes         | Pods               |

#### 5.1.4 Scopes

| Scope            | Claims / Access                          |
| ---------------- | ---------------------------------------- |
| `openid`         | `sub`, `iss`, `aud`, `exp`, `iat`        |
| `profile`        | `username`, `display_name`, `avatar_url` |
| `email`          | `email`, `email_verified`                |
| `pods`           | Ability to request SIAs for Pod auth     |
| `pods.admin`     | Pod registration and management          |
| `billing`        | Billing and subscription APIs            |
| `offline_access` | Refresh token issuance                   |

### 5.2 User Registration

#### 5.2.1 Registration Methods

- **Email + Password**: Argon2id hashing, min 10 chars, breach-check via
  k-anonymity (Have I Been Pwned API).
- **Passkey (WebAuthn)**: Passwordless registration. Recommended path.
- **OAuth Social Login**: GitHub, Google, Apple as upstream IdPs. Hub links
  external identity to a Voxora account.

#### 5.2.2 Username Rules

- Globally unique.
- 2–32 characters.
- Allowed: `[a-zA-Z0-9_.-]`
- Case-insensitive for uniqueness (stored lowercase, displayed as entered).
- Reserved list for platform terms (`admin`, `system`, `voxora`, etc.).
- Username changes allowed once per 30 days. Previous usernames are held for
  90 days to prevent hijacking.

#### 5.2.3 Display Name

- 1–64 characters.
- Unicode allowed (with normalization NFC).
- No uniqueness constraint.
- Per-community display name overrides are stored on the Pod.

### 5.3 Multi-Factor Authentication

- **TOTP** (RFC 6238): 6-digit codes, 30-second window.
- **WebAuthn / Passkeys**: FIDO2 security keys and platform authenticators.
- **Recovery Codes**: 8 single-use codes generated at MFA enrollment.

MFA is optional but encouraged. Hub Admins can enforce MFA for Hub admin
accounts.

### 5.4 Signed Identity Assertion (SIA)

The SIA is the critical bridge between Hub identity and Pod authorization.

#### 5.4.1 SIA Format

```
Header:
{
  "alg": "EdDSA",
  "kid": "<key-id>",
  "typ": "voxora-sia+jwt"
}

Payload:
{
  "iss": "https://hub.voxora.app",
  "sub": "usr_01H8MZXK9Q5BNRG7YDZS4A2C3E",
  "aud": "pod_01J9NXYK3R6CMSH8ZEWTB5D4F7G",
  "iat": 1739145600,
  "exp": 1739145900,
  "jti": "sia_01KPQRST2U3VWXYZ4A5B6C7D8E",
  "username": "alice",
  "display_name": "Alice",
  "avatar_url": "https://hub.voxora.app/avatars/usr_01H8MZ.webp",
  "email": "alice@example.com",
  "email_verified": true,
  "flags": ["staff", "early_adopter"],
  "hub_version": 1
}
```

#### 5.4.2 SIA Properties

- **Algorithm**: EdDSA (Ed25519). Chosen for small signatures (64 bytes),
  fast verification, and no padding oracles.
- **Audience**: Scoped to a specific Pod ID. A SIA for Pod A cannot be
  replayed to Pod B.
- **Lifetime**: 5 minutes. Short-lived to limit replay window.
- **JTI**: Unique identifier. Pods SHOULD maintain a JTI cache for the
  lifetime window to detect replays.
- **Non-reusable**: Each Pod connection requires a fresh SIA. Clients request
  a new SIA from the Hub before each Pod connection.

#### 5.4.3 SIA Request

```http
POST /oidc/sia HTTP/1.1
Authorization: Bearer <hub_access_token>
Content-Type: application/json

{
  "pod_id": "pod_01J9NXYK3R6CMSH8ZEWTB5D4F7G"
}
```

Response:

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "sia": "<signed-jwt>",
  "expires_at": "2026-02-10T12:05:00Z"
}
```

### 5.5 Key Management

#### 5.5.1 Key Types

| Key                  | Algorithm | Purpose                           |
| -------------------- | --------- | --------------------------------- |
| SIA Signing Key      | Ed25519   | Sign SIAs                         |
| ID Token Signing Key | Ed25519   | Sign OIDC ID tokens               |
| Encryption Key       | X25519    | Encrypt sensitive claims (future) |

#### 5.5.2 Key Rotation

- Keys are rotated every **90 days**.
- The JWKS endpoint publishes both the current and previous key.
- The previous key is retained for **7 days** after rotation for in-flight
  token validation.
- Pods SHOULD cache the JWKS with a max-age of **1 hour** and re-fetch on
  `kid` miss.

#### 5.5.3 JWKS Endpoint

```http
GET /oidc/.well-known/jwks.json HTTP/1.1

{
  "keys": [
    {
      "kty": "OKP",
      "crv": "Ed25519",
      "kid": "hub-sia-2026-02",
      "use": "sig",
      "x": "<base64url-encoded-public-key>"
    }
  ]
}
```

### 5.6 Session Management

- Hub sessions are cookie-based (HttpOnly, Secure, SameSite=Lax).
- Session lifetime: 30 days with sliding expiry.
- Concurrent session limit: 10 per user. Oldest evicted.
- Users can view and revoke sessions from their account page.
- Session revocation propagates to refresh tokens immediately.

---

## 6. Hub ↔ Pod Federation Protocol

### 6.1 Pod Registration

Before a Pod can accept users, it MUST register with the Hub.

#### 6.1.1 Registration Flow

```
Pod Admin                    Hub
    │                         │
    │── POST /pods/register ─►│  (with admin's Hub access token)
    │◄─ pod_id + client_creds │
    │                         │
    │── POST /oidc/register ─►│  (OIDC dynamic client registration)
    │◄─ client_id + secret ───│
    │                         │
    │── PUT /pods/{id}/meta ─►│  (name, description, icon, URL)
    │◄─ 200 OK ───────────────│
    │                         │
    │── Start heartbeat ──────│
```

#### 6.1.2 Pod Registration Payload

```http
POST /api/v1/pods/register HTTP/1.1
Authorization: Bearer <admin_hub_access_token>
Content-Type: application/json

{
  "name": "Alice's Gaming Pod",
  "description": "A community for gamers",
  "url": "https://pod.alice.example.com",
  "icon_url": "https://pod.alice.example.com/icon.png",
  "region": "us-east-1",
  "contact_email": "admin@alice.example.com",
  "public": true,
  "capabilities": ["text", "voice", "video"],
  "max_members": 10000,
  "version": "1.0.0"
}
```

Response:

```http
HTTP/1.1 201 Created
Content-Type: application/json

{
  "pod_id": "pod_01J9NXYK3R6CMSH8ZEWTB5D4F7G",
  "client_id": "pod_client_01KPQRST...",
  "client_secret": "vxs_...",
  "registered_at": "2026-02-10T12:00:00Z",
  "status": "active",
  "verification": "unverified"
}
```

#### 6.1.3 Pod Metadata

Pods publish metadata that the Hub stores and exposes to clients:

```json
{
  "pod_id": "pod_01J9NXYK3R6CMSH8ZEWTB5D4F7G",
  "name": "Alice's Gaming Pod",
  "description": "A community for gamers",
  "icon_url": "https://...",
  "url": "https://pod.alice.example.com",
  "region": "us-east-1",
  "member_count": 4521,
  "online_count": 312,
  "community_count": 8,
  "capabilities": ["text", "voice", "video"],
  "verification": "verified",
  "managed": false,
  "version": "1.2.0",
  "uptime_30d": 0.998,
  "last_heartbeat": "2026-02-10T11:59:30Z"
}
```

### 6.2 Heartbeat Protocol

Pods send periodic heartbeats to the Hub to maintain presence in the registry.

#### 6.2.1 Heartbeat Request

```http
POST /api/v1/pods/{pod_id}/heartbeat HTTP/1.1
Authorization: Bearer <pod_client_credentials_token>
Content-Type: application/json

{
  "timestamp": "2026-02-10T12:00:00Z",
  "member_count": 4521,
  "online_count": 312,
  "community_count": 8,
  "version": "1.2.0",
  "load": {
    "cpu_percent": 42.5,
    "memory_percent": 61.2,
    "active_voice_sessions": 15,
    "messages_per_minute": 340
  },
  "health": "healthy"
}
```

#### 6.2.2 Heartbeat Intervals

| Pod Status | Interval   | Missed Threshold                         |
| ---------- | ---------- | ---------------------------------------- |
| Active     | 60 seconds | 3 missed → degraded                      |
| Degraded   | 30 seconds | 5 more missed → offline                  |
| Offline    | N/A        | Auto-removed from discovery after 7 days |

### 6.3 Pod Discovery

#### 6.3.1 Listing Pods

```http
GET /api/v1/pods?sort=popular&region=us-east&verified=true&page=1 HTTP/1.1
Authorization: Bearer <hub_access_token>
```

Response:

```json
{
  "pods": [ ... ],
  "pagination": {
    "page": 1,
    "per_page": 25,
    "total": 1482
  }
}
```

#### 6.3.2 Sort Options

| Sort       | Description                       |
| ---------- | --------------------------------- |
| `popular`  | By member count descending        |
| `trending` | By growth rate over 7 days        |
| `newest`   | By registration date              |
| `nearest`  | By geographic proximity to client |

#### 6.3.3 Search

```http
GET /api/v1/pods/search?q=gaming&tags=fps,competitive HTTP/1.1
```

Full-text search over Pod name, description, tags, and community names.

### 6.4 Hub → Pod Notifications

The Hub can push events to Pods via webhook:

| Event                      | Trigger                     |
| -------------------------- | --------------------------- |
| `user.banned`              | Hub-level ban (spam, abuse) |
| `user.updated`             | Username or avatar change   |
| `key.rotated`              | JWKS key rotation notice    |
| `pod.verification_changed` | Verification status update  |

Pods register a webhook URL during registration. Events are signed with
HMAC-SHA256 using the Pod's client secret.

```http
POST <pod_webhook_url> HTTP/1.1
Content-Type: application/json
X-Voxora-Signature: sha256=<hmac>
X-Voxora-Event: user.updated
X-Voxora-Delivery: del_01KPQRST...

{
  "event": "user.updated",
  "timestamp": "2026-02-10T12:00:00Z",
  "data": {
    "user_id": "usr_01H8MZXK...",
    "changes": ["username", "avatar_url"]
  }
}
```

---

## 7. Pod Authentication & Authorization

### 7.1 User → Pod Auth Flow

```
1. Client requests SIA from Hub (scoped to target Pod ID)
2. Client sends SIA to Pod: POST /api/v1/auth/login
3. Pod fetches Hub JWKS (cached) and validates SIA signature
4. Pod checks SIA expiration (reject if expired)
5. Pod checks SIA audience (must match own Pod ID)
6. Pod checks JTI against replay cache (reject if seen)
7. Pod OPTIONALLY calls Hub /oidc/introspect for revocation check
8. Pod creates or updates local user record (linked to Hub sub)
9. Pod issues PAT + WebSocket ticket
10. Client uses PAT for REST, ticket for Gateway connection
```

#### 7.1.1 Login Request

```http
POST /api/v1/auth/login HTTP/1.1
Content-Type: application/json

{
  "sia": "<signed-jwt-from-hub>"
}
```

Response:

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "access_token": "pat_01KPQRST...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "refresh_token": "prt_01KPQRST...",
  "ws_ticket": "wst_01KPQRST...",
  "ws_url": "wss://pod.alice.example.com/gateway",
  "user": {
    "id": "usr_01H8MZXK...",
    "username": "alice",
    "display_name": "Alice",
    "avatar_url": "https://hub.voxora.app/avatars/...",
    "roles": ["member"],
    "joined_at": "2026-01-15T10:00:00Z"
  }
}
```

#### 7.1.2 PAT Properties

| Property | Value                                             |
| -------- | ------------------------------------------------- |
| Format   | Opaque token (stored server-side)                 |
| Lifetime | 1 hour                                            |
| Refresh  | Via Pod refresh token (24-hour lifetime)          |
| Scope    | Full Pod API access (further restricted by roles) |

#### 7.1.3 WebSocket Ticket

- Single-use, 30-second lifetime.
- Exchanged during WebSocket handshake.
- Prevents token exposure in WebSocket URL.

### 7.2 Role-Based Access Control (RBAC)

#### 7.2.1 Permission Model

Permissions are computed as a bitfield. Each role has a permission bitfield,
and a user's effective permissions are the union of all their role permissions,
minus any explicit denies.

```
effective = (union of all role allows) & ~(union of all role denies)
```

Channel-level overrides can grant or deny permissions per-role per-channel.

#### 7.2.2 Permission Definitions

| Permission            | Bit | Description                            |
| --------------------- | --- | -------------------------------------- |
| `VIEW_CHANNEL`        | 0   | Can see the channel                    |
| `SEND_MESSAGES`       | 1   | Can send messages                      |
| `SEND_ATTACHMENTS`    | 2   | Can upload files                       |
| `MANAGE_MESSAGES`     | 3   | Can delete/pin others' messages        |
| `MANAGE_CHANNELS`     | 4   | Can create/edit/delete channels        |
| `MANAGE_COMMUNITY`    | 5   | Can edit community settings            |
| `MANAGE_ROLES`        | 6   | Can create/edit roles below own        |
| `KICK_MEMBERS`        | 7   | Can kick members                       |
| `BAN_MEMBERS`         | 8   | Can ban members                        |
| `INVITE_MEMBERS`      | 9   | Can create invites                     |
| `VOICE_CONNECT`       | 10  | Can join voice channels                |
| `VOICE_SPEAK`         | 11  | Can transmit audio                     |
| `VOICE_VIDEO`         | 12  | Can transmit video                     |
| `VOICE_MUTE_OTHERS`   | 13  | Can server-mute others                 |
| `VOICE_DEAFEN_OTHERS` | 14  | Can server-deafen others               |
| `VOICE_MOVE_OTHERS`   | 15  | Can move others between voice channels |
| `USE_REACTIONS`       | 16  | Can add reactions                      |
| `CREATE_THREADS`      | 17  | Can create threads                     |
| `EMBED_LINKS`         | 18  | Can post rich embeds                   |
| `MENTION_EVERYONE`    | 19  | Can @everyone / @here                  |
| `VIEW_AUDIT_LOG`      | 20  | Can view audit log                     |
| `ADMINISTRATOR`       | 31  | All permissions (overrides all)        |

#### 7.2.3 Role Hierarchy

- Roles have a `position` (integer, 0 = lowest).
- A user can only manage roles below their highest role position.
- The `@everyone` role (position 0) applies to all members by default.
- The Community owner has implicit `ADMINISTRATOR` regardless of roles.

---

## 8. Real-Time Messaging

### 8.1 Message Model

```json
{
  "id": "msg_01KPQRST2U3VWXYZ...",
  "channel_id": "ch_01KPQRST...",
  "author": {
    "id": "usr_01H8MZXK...",
    "username": "alice",
    "display_name": "Alice",
    "avatar_url": "https://..."
  },
  "content": "Hello, world!",
  "type": "default",
  "timestamp": "2026-02-10T12:00:00.000Z",
  "edited_at": null,
  "attachments": [],
  "embeds": [],
  "reactions": [],
  "mentions": [],
  "mention_roles": [],
  "mention_everyone": false,
  "pinned": false,
  "thread_id": null,
  "reply_to": null,
  "nonce": "client-generated-uuid",
  "flags": 0
}
```

### 8.2 Message Types

| Type                    | Value | Description               |
| ----------------------- | ----- | ------------------------- |
| `default`               | 0     | Normal user message       |
| `reply`                 | 1     | Reply to another message  |
| `thread_starter`        | 2     | First message of a thread |
| `system_join`           | 10    | User joined the community |
| `system_leave`          | 11    | User left the community   |
| `system_pin`            | 12    | Message was pinned        |
| `system_channel_update` | 13    | Channel settings changed  |

### 8.3 Message Ordering

- Messages are ordered by **Snowflake ID** (timestamp-embedded).
- Snowflake structure (64-bit):

```
 63                         22  21     12  11       0
┌──────────────────────────────┬─────────┬──────────┐
│     Timestamp (ms since      │  Pod    │ Sequence │
│     epoch, 42 bits)          │  ID     │ (12 bits)│
│                              │(10 bits)│          │
└──────────────────────────────┴─────────┴──────────┘
```

- Epoch: `2025-01-01T00:00:00Z` (Voxora epoch)
- This gives ~139 years of IDs, 1024 Pod shards, 4096 IDs per millisecond
  per shard.

### 8.4 Message Delivery

#### 8.4.1 Send Path

```
Client ──POST /channels/{id}/messages──► Pod REST API
                                              │
                                              ▼
                                       Validation
                                       (perms, rate limit, content)
                                              │
                                              ▼
                                       Persist to DB
                                              │
                                              ▼
                                       Fanout to Gateway
                                              │
                                         ┌────┴────┐
                                         ▼         ▼
                                    WS Client  WS Client
                                       A          B
```

#### 8.4.2 Fanout Strategy

- Each channel has a set of subscribed Gateway connections.
- On message create, the Gateway process fans out the `MESSAGE_CREATE` event
  to all subscribed connections.
- For large channels (>10,000 online members), use pub/sub (Redis or NATS)
  across multiple Gateway instances.

### 8.5 Message History

```http
GET /api/v1/channels/{channel_id}/messages?before=msg_01KPQRST...&limit=50 HTTP/1.1
Authorization: Bearer <PAT>
```

- Cursor-based pagination using message IDs.
- Default limit: 50. Max: 100.
- Supports `before`, `after`, and `around` cursors.

### 8.6 Attachments

- Uploaded via `POST /api/v1/channels/{channel_id}/attachments`.
- Returns a pre-signed upload URL (S3-compatible or local storage).
- Max file size: 25 MB (default, configurable per Pod).
- Supported: images, video, audio, documents.
- Images are thumbnailed server-side.
- Virus scanning recommended for managed Pods.

### 8.7 Embeds

- URL embeds: Pod fetches Open Graph / oEmbed metadata.
- Rich embeds: Bots can post structured embeds with title, description,
  fields, color, thumbnail, footer.

### 8.8 Reactions

- Unicode emoji or custom emoji (uploaded to Community).
- One reaction per emoji per user per message.
- Max 20 unique emoji per message.

### 8.9 Threads

- Any message can become a thread parent.
- Threads have their own channel ID (type `thread`).
- Threads auto-archive after configurable inactivity (1h, 24h, 3d, 7d).
- Thread members are tracked separately.

### 8.10 Pins

- Max 50 pins per channel.
- Requires `MANAGE_MESSAGES` permission.

---

## 9. Voice & Video

### 9.1 Architecture

Each Pod runs a **Selective Forwarding Unit (SFU)** for voice and video.

```
┌──────────┐     WebRTC      ┌───────┐     WebRTC     ┌──────────┐
│ Client A │ ◄──────────────► │  SFU  │ ◄─────────────► │ Client B │
│          │    (audio/video) │       │   (audio/video) │          │
└──────────┘                  │       │                  └──────────┘
                              │       │
┌──────────┐     WebRTC      │       │
│ Client C │ ◄──────────────► │       │
└──────────┘                  └───────┘
```

Why SFU over MCU or P2P:

- **vs P2P**: Scales beyond 2-3 participants. Clients upload once.
- **vs MCU**: Lower server CPU (no transcoding). Lower latency.

### 9.2 Signaling

Signaling is performed over the existing WebSocket Gateway connection.

#### 9.2.1 Voice State Update (Client → Pod)

```json
{
  "op": 4,
  "d": {
    "channel_id": "ch_01KPQRST...",
    "self_mute": false,
    "self_deaf": false,
    "self_video": false
  }
}
```

#### 9.2.2 Voice Server Info (Pod → Client)

```json
{
  "op": 5,
  "d": {
    "channel_id": "ch_01KPQRST...",
    "endpoint": "wss://pod.alice.example.com/voice",
    "token": "voice_01KPQRST...",
    "ice_servers": [
      {
        "urls": ["stun:stun.voxora.app:3478"]
      },
      {
        "urls": ["turn:turn.voxora.app:3478"],
        "username": "...",
        "credential": "..."
      }
    ]
  }
}
```

#### 9.2.3 WebRTC Session Setup

1. Client connects to voice WebSocket endpoint.
2. Client sends SDP offer.
3. SFU responds with SDP answer.
4. ICE candidates exchanged.
5. DTLS handshake completes.
6. SRTP media flows.

### 9.3 Codec Negotiation

| Media        | Primary Codec | Fallback     |
| ------------ | ------------- | ------------ |
| Audio        | Opus @ 48kHz  | Opus @ 16kHz |
| Video        | VP9           | VP8          |
| Screen Share | AV1           | VP9          |

- Audio: Always Opus. Bitrate negotiated (32-128 kbps).
- Video: Simulcast with 3 layers (high/medium/low quality).
- SFU selects which layer to forward based on recipient's bandwidth and
  viewport.

### 9.4 Voice Channels

- Voice channels have a configurable user limit (0 = unlimited).
- Voice activity detection (VAD) for speaking indicators.
- Server mute/deafen by moderators.
- Per-user volume adjustment (client-side gain).

### 9.5 Video Rooms

- Toggle video in any voice channel (permission-gated).
- Screenshare: one screenshare slot per channel (or configurable).
- Grid and spotlight layouts (client-side rendering).

### 9.6 TURN Infrastructure

- TURN servers are **provided by the Hub** as a shared service for all Pods.
- Pods request TURN credentials from the Hub using client credentials.
- Credentials are time-limited (12-hour TTL).
- TURN is required for ~15-20% of connections due to symmetric NATs.

```http
POST /api/v1/turn/credentials HTTP/1.1
Authorization: Bearer <pod_client_credentials_token>

Response:
{
  "ice_servers": [
    {
      "urls": ["turn:turn-us.voxora.app:443?transport=tcp"],
      "username": "1739232000:pod_01J9NXYK...",
      "credential": "<hmac-based-credential>",
      "ttl": 43200
    }
  ]
}
```

---

## 10. Data Model

### 10.1 Hub Database (PostgreSQL)

#### 10.1.1 Users

```sql
CREATE TABLE users (
    id              TEXT PRIMARY KEY,           -- usr_<ulid>
    username        TEXT NOT NULL UNIQUE,
    username_lower  TEXT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL,
    email           TEXT UNIQUE,
    email_verified  BOOLEAN NOT NULL DEFAULT FALSE,
    password_hash   TEXT,                       -- NULL if passkey-only
    avatar_url      TEXT,
    banner_url      TEXT,
    bio             TEXT,
    flags           BIGINT NOT NULL DEFAULT 0,  -- staff, early_adopter, etc.
    mfa_enabled     BOOLEAN NOT NULL DEFAULT FALSE,
    mfa_secret      TEXT,                       -- TOTP secret (encrypted)
    status          TEXT NOT NULL DEFAULT 'active',  -- active, suspended, deleted
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

#### 10.1.2 Sessions

```sql
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,           -- ses_<ulid>
    user_id         TEXT NOT NULL REFERENCES users(id),
    refresh_token   TEXT NOT NULL UNIQUE,
    ip_address      INET,
    user_agent      TEXT,
    last_active_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sessions_user ON sessions(user_id);
CREATE INDEX idx_sessions_refresh ON sessions(refresh_token);
```

#### 10.1.3 Passkeys

```sql
CREATE TABLE passkeys (
    id              TEXT PRIMARY KEY,           -- pk_<ulid>
    user_id         TEXT NOT NULL REFERENCES users(id),
    credential_id   BYTEA NOT NULL UNIQUE,
    public_key      BYTEA NOT NULL,
    sign_count      BIGINT NOT NULL DEFAULT 0,
    name            TEXT NOT NULL,              -- user-friendly label
    transports      TEXT[],
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ
);
```

#### 10.1.4 OAuth Connections

```sql
CREATE TABLE oauth_connections (
    id              TEXT PRIMARY KEY,           -- oc_<ulid>
    user_id         TEXT NOT NULL REFERENCES users(id),
    provider        TEXT NOT NULL,              -- github, google, apple
    provider_id     TEXT NOT NULL,
    provider_email  TEXT,
    access_token    TEXT,                       -- encrypted
    refresh_token   TEXT,                       -- encrypted
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider, provider_id)
);
```

#### 10.1.5 Pods (Registry)

```sql
CREATE TABLE pods (
    id              TEXT PRIMARY KEY,           -- pod_<ulid>
    owner_id        TEXT NOT NULL REFERENCES users(id),
    name            TEXT NOT NULL,
    description     TEXT,
    icon_url        TEXT,
    url             TEXT NOT NULL,
    webhook_url     TEXT,
    webhook_secret  TEXT,                       -- encrypted
    region          TEXT,
    client_id       TEXT NOT NULL UNIQUE,
    client_secret   TEXT NOT NULL,              -- encrypted
    public          BOOLEAN NOT NULL DEFAULT TRUE,
    capabilities    TEXT[] NOT NULL DEFAULT '{"text"}',
    max_members     INTEGER NOT NULL DEFAULT 10000,
    version         TEXT,
    verification    TEXT NOT NULL DEFAULT 'unverified',
    managed         BOOLEAN NOT NULL DEFAULT FALSE,
    status          TEXT NOT NULL DEFAULT 'active',
    member_count    INTEGER NOT NULL DEFAULT 0,
    online_count    INTEGER NOT NULL DEFAULT 0,
    community_count INTEGER NOT NULL DEFAULT 0,
    last_heartbeat  TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pods_owner ON pods(owner_id);
CREATE INDEX idx_pods_status ON pods(status);
CREATE INDEX idx_pods_verification ON pods(verification);
```

#### 10.1.6 Pod Verification

```sql
CREATE TABLE pod_verifications (
    id              TEXT PRIMARY KEY,           -- pv_<ulid>
    pod_id          TEXT NOT NULL REFERENCES pods(id),
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending, in_review, approved, rejected
    reviewer_id     TEXT REFERENCES users(id),
    domain_proof    JSONB,
    security_check  JSONB,
    policy_check    JSONB,
    notes           TEXT,
    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at     TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ                -- verification renewal
);
```

#### 10.1.7 Billing

```sql
CREATE TABLE billing_accounts (
    id              TEXT PRIMARY KEY,           -- ba_<ulid>
    user_id         TEXT NOT NULL REFERENCES users(id),
    stripe_customer TEXT UNIQUE,
    plan            TEXT NOT NULL DEFAULT 'free',
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE subscriptions (
    id              TEXT PRIMARY KEY,           -- sub_<ulid>
    billing_id      TEXT NOT NULL REFERENCES billing_accounts(id),
    pod_id          TEXT REFERENCES pods(id),
    plan            TEXT NOT NULL,
    stripe_sub_id   TEXT UNIQUE,
    status          TEXT NOT NULL DEFAULT 'active',
    current_period_start TIMESTAMPTZ,
    current_period_end   TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 10.2 Pod Database (PostgreSQL)

#### 10.2.1 Pod Users (Local)

```sql
CREATE TABLE pod_users (
    id              TEXT PRIMARY KEY,           -- hub user ID (usr_<ulid>)
    username        TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    avatar_url      TEXT,
    hub_flags       BIGINT NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'active',
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

#### 10.2.2 Communities

```sql
CREATE TABLE communities (
    id              TEXT PRIMARY KEY,           -- com_<ulid>
    name            TEXT NOT NULL,
    description     TEXT,
    icon_url        TEXT,
    banner_url      TEXT,
    owner_id        TEXT NOT NULL REFERENCES pod_users(id),
    default_channel TEXT,
    features        TEXT[] NOT NULL DEFAULT '{}',
    member_count    INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

#### 10.2.3 Community Members

```sql
CREATE TABLE community_members (
    community_id    TEXT NOT NULL REFERENCES communities(id),
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    nickname        TEXT,                       -- community-specific display name
    roles           TEXT[] NOT NULL DEFAULT '{}',
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (community_id, user_id)
);

CREATE INDEX idx_members_user ON community_members(user_id);
```

#### 10.2.4 Roles

```sql
CREATE TABLE roles (
    id              TEXT PRIMARY KEY,           -- role_<ulid>
    community_id    TEXT NOT NULL REFERENCES communities(id),
    name            TEXT NOT NULL,
    color           INTEGER,                    -- RGB as integer
    position        INTEGER NOT NULL DEFAULT 0,
    permissions     BIGINT NOT NULL DEFAULT 0,
    mentionable     BOOLEAN NOT NULL DEFAULT FALSE,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE, -- @everyone role
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_roles_community ON roles(community_id);
```

#### 10.2.5 Channels

```sql
CREATE TABLE channels (
    id              TEXT PRIMARY KEY,           -- ch_<ulid>
    community_id    TEXT NOT NULL REFERENCES communities(id),
    parent_id       TEXT REFERENCES channels(id),  -- category or thread parent
    name            TEXT NOT NULL,
    topic           TEXT,
    type            SMALLINT NOT NULL DEFAULT 0,
        -- 0: text, 1: voice, 2: announcement, 3: forum, 4: stage, 5: thread
    position        INTEGER NOT NULL DEFAULT 0,
    slowmode_seconds INTEGER NOT NULL DEFAULT 0,
    nsfw            BOOLEAN NOT NULL DEFAULT FALSE,
    user_limit      INTEGER NOT NULL DEFAULT 0,    -- voice: 0 = unlimited
    bitrate         INTEGER NOT NULL DEFAULT 64000, -- voice: bits per second
    archived        BOOLEAN NOT NULL DEFAULT FALSE,
    archive_after   INTEGER,                   -- seconds of inactivity for threads
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_channels_community ON channels(community_id);
CREATE INDEX idx_channels_parent ON channels(parent_id);
```

#### 10.2.6 Channel Permission Overrides

```sql
CREATE TABLE channel_overrides (
    channel_id      TEXT NOT NULL REFERENCES channels(id),
    target_type     SMALLINT NOT NULL,          -- 0: role, 1: user
    target_id       TEXT NOT NULL,              -- role ID or user ID
    allow           BIGINT NOT NULL DEFAULT 0,
    deny            BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, target_type, target_id)
);
```

#### 10.2.7 Messages

```sql
CREATE TABLE messages (
    id              BIGINT PRIMARY KEY,         -- Snowflake ID
    channel_id      TEXT NOT NULL REFERENCES channels(id),
    author_id       TEXT NOT NULL REFERENCES pod_users(id),
    content         TEXT,
    type            SMALLINT NOT NULL DEFAULT 0,
    flags           INTEGER NOT NULL DEFAULT 0,
    reply_to        BIGINT,                    -- Snowflake ID of parent
    edited_at       TIMESTAMPTZ,
    pinned          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Partition by channel for large Pods
CREATE INDEX idx_messages_channel ON messages(channel_id, id DESC);
CREATE INDEX idx_messages_author ON messages(author_id);
CREATE INDEX idx_messages_pinned ON messages(channel_id) WHERE pinned = TRUE;
```

#### 10.2.8 Attachments

```sql
CREATE TABLE attachments (
    id              TEXT PRIMARY KEY,           -- att_<ulid>
    message_id      BIGINT NOT NULL REFERENCES messages(id),
    filename        TEXT NOT NULL,
    content_type    TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL,
    url             TEXT NOT NULL,
    proxy_url       TEXT,
    width           INTEGER,
    height          INTEGER,
    thumbnail_url   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_attachments_message ON attachments(message_id);
```

#### 10.2.9 Reactions

```sql
CREATE TABLE reactions (
    message_id      BIGINT NOT NULL REFERENCES messages(id),
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    emoji           TEXT NOT NULL,              -- Unicode or custom:emoji_id
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, user_id, emoji)
);
```

#### 10.2.10 Invites

```sql
CREATE TABLE invites (
    code            TEXT PRIMARY KEY,           -- 8-char alphanumeric
    community_id    TEXT NOT NULL REFERENCES communities(id),
    channel_id      TEXT REFERENCES channels(id),
    inviter_id      TEXT NOT NULL REFERENCES pod_users(id),
    max_uses        INTEGER,                    -- NULL = unlimited
    use_count       INTEGER NOT NULL DEFAULT 0,
    max_age_seconds INTEGER,                    -- NULL = never expires
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ
);
```

#### 10.2.11 Audit Log

```sql
CREATE TABLE audit_log (
    id              TEXT PRIMARY KEY,           -- aud_<ulid>
    community_id    TEXT NOT NULL REFERENCES communities(id),
    actor_id        TEXT NOT NULL REFERENCES pod_users(id),
    action          TEXT NOT NULL,              -- e.g. 'message.delete', 'member.kick'
    target_type     TEXT,
    target_id       TEXT,
    changes         JSONB,
    reason          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_community ON audit_log(community_id, created_at DESC);
```

#### 10.2.12 Voice Sessions

```sql
CREATE TABLE voice_sessions (
    id              TEXT PRIMARY KEY,           -- vs_<ulid>
    channel_id      TEXT NOT NULL REFERENCES channels(id),
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    session_id      TEXT NOT NULL,              -- WebRTC session
    self_mute       BOOLEAN NOT NULL DEFAULT FALSE,
    self_deaf       BOOLEAN NOT NULL DEFAULT FALSE,
    server_mute     BOOLEAN NOT NULL DEFAULT FALSE,
    server_deaf     BOOLEAN NOT NULL DEFAULT FALSE,
    self_video      BOOLEAN NOT NULL DEFAULT FALSE,
    self_stream     BOOLEAN NOT NULL DEFAULT FALSE,
    connected_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_voice_channel ON voice_sessions(channel_id);
CREATE INDEX idx_voice_user ON voice_sessions(user_id);
```

---

## 11. Hub API Specification

All Hub APIs are versioned under `/api/v1/`.

### 11.1 Authentication Endpoints

See Section 5.1.1 for OIDC endpoints.

### 11.2 User Endpoints

#### `POST /api/v1/users` — Register

```json
Request:
{
  "username": "alice",
  "email": "alice@example.com",
  "password": "correct-horse-battery-staple",
  "display_name": "Alice"
}

Response (201):
{
  "id": "usr_01H8MZXK...",
  "username": "alice",
  "email": "alice@example.com",
  "email_verified": false,
  "display_name": "Alice",
  "created_at": "2026-02-10T12:00:00Z"
}
```

#### `GET /api/v1/users/@me` — Current User

#### `PATCH /api/v1/users/@me` — Update Profile

#### `GET /api/v1/users/{user_id}` — Public Profile

```json
Response:
{
  "id": "usr_01H8MZXK...",
  "username": "alice",
  "display_name": "Alice",
  "avatar_url": "https://...",
  "banner_url": "https://...",
  "bio": "Hello!",
  "flags": ["early_adopter"],
  "created_at": "2026-02-10T12:00:00Z"
}
```

#### `GET /api/v1/users/@me/pods` — User's Pods

Returns list of Pods the user is a member of (stored on Hub as bookmarks).

### 11.3 Pod Registry Endpoints

#### `POST /api/v1/pods/register` — Register Pod

See Section 6.1.2.

#### `GET /api/v1/pods` — List Pods

#### `GET /api/v1/pods/{pod_id}` — Pod Details

#### `PATCH /api/v1/pods/{pod_id}` — Update Pod Metadata

#### `DELETE /api/v1/pods/{pod_id}` — Deregister Pod

#### `POST /api/v1/pods/{pod_id}/heartbeat` — Heartbeat

See Section 6.2.1.

#### `GET /api/v1/pods/search` — Search Pods

### 11.4 Verification Endpoints

#### `POST /api/v1/pods/{pod_id}/verify` — Submit Verification Request

#### `GET /api/v1/pods/{pod_id}/verify` — Check Verification Status

#### `POST /api/v1/pods/{pod_id}/verify/domain` — Submit Domain Proof

### 11.5 TURN Endpoints

#### `POST /api/v1/turn/credentials` — Request TURN Credentials

See Section 9.6.

### 11.6 Billing Endpoints

#### `POST /api/v1/billing/setup` — Initialize Billing

#### `GET /api/v1/billing/subscriptions` — List Subscriptions

#### `POST /api/v1/billing/subscriptions` — Create Subscription

#### `DELETE /api/v1/billing/subscriptions/{id}` — Cancel Subscription

#### `GET /api/v1/billing/invoices` — List Invoices

---

## 12. Pod API Specification

All Pod APIs are versioned under `/api/v1/`.

### 12.1 Auth

#### `POST /api/v1/auth/login` — Login with SIA

See Section 7.1.1.

#### `POST /api/v1/auth/refresh` — Refresh PAT

```json
Request:
{
  "refresh_token": "prt_01KPQRST..."
}

Response:
{
  "access_token": "pat_01KPQRST...",
  "expires_in": 3600,
  "refresh_token": "prt_01KPQRST..."
}
```

### 12.2 Communities

#### `GET /api/v1/communities` — List Communities on this Pod

#### `POST /api/v1/communities` — Create Community

```json
Request:
{
  "name": "Gamers United",
  "description": "A community for gamers",
  "icon": "<base64 or upload id>"
}

Response (201):
{
  "id": "com_01KPQRST...",
  "name": "Gamers United",
  "owner_id": "usr_01H8MZXK...",
  "channels": [
    {
      "id": "ch_01KPQRST...",
      "name": "general",
      "type": 0
    }
  ],
  "roles": [
    {
      "id": "role_01KPQRST...",
      "name": "@everyone",
      "position": 0,
      "permissions": 1049617
    }
  ],
  "member_count": 1,
  "created_at": "2026-02-10T12:00:00Z"
}
```

#### `GET /api/v1/communities/{id}` — Get Community

#### `PATCH /api/v1/communities/{id}` — Update Community

#### `DELETE /api/v1/communities/{id}` — Delete Community

### 12.3 Channels

#### `GET /api/v1/communities/{id}/channels` — List Channels

#### `POST /api/v1/communities/{id}/channels` — Create Channel

#### `PATCH /api/v1/channels/{id}` — Update Channel

#### `DELETE /api/v1/channels/{id}` — Delete Channel

### 12.4 Messages

#### `GET /api/v1/channels/{id}/messages` — Get Messages

Query params: `before`, `after`, `around`, `limit`.

#### `POST /api/v1/channels/{id}/messages` — Send Message

```json
Request:
{
  "content": "Hello, world!",
  "nonce": "550e8400-e29b-41d4-a716-446655440000",
  "reply_to": null,
  "attachments": []
}

Response (201):
{
  "id": 175928847299117056,
  "channel_id": "ch_01KPQRST...",
  "author": { ... },
  "content": "Hello, world!",
  "timestamp": "2026-02-10T12:00:00.000Z",
  ...
}
```

#### `PATCH /api/v1/channels/{channel_id}/messages/{id}` — Edit Message

#### `DELETE /api/v1/channels/{channel_id}/messages/{id}` — Delete Message

#### `POST /api/v1/channels/{channel_id}/messages/{id}/reactions/{emoji}` — Add Reaction

#### `DELETE /api/v1/channels/{channel_id}/messages/{id}/reactions/{emoji}` — Remove Reaction

#### `POST /api/v1/channels/{channel_id}/pins/{message_id}` — Pin Message

#### `DELETE /api/v1/channels/{channel_id}/pins/{message_id}` — Unpin Message

### 12.5 Threads

#### `POST /api/v1/channels/{channel_id}/messages/{id}/threads` — Create Thread

#### `GET /api/v1/channels/{channel_id}/threads` — List Active Threads

### 12.6 Members

#### `GET /api/v1/communities/{id}/members` — List Members

#### `GET /api/v1/communities/{id}/members/{user_id}` — Get Member

#### `PATCH /api/v1/communities/{id}/members/{user_id}` — Update Member (roles, nickname)

#### `DELETE /api/v1/communities/{id}/members/{user_id}` — Kick Member

#### `PUT /api/v1/communities/{id}/bans/{user_id}` — Ban Member

#### `DELETE /api/v1/communities/{id}/bans/{user_id}` — Unban Member

### 12.7 Roles

#### `GET /api/v1/communities/{id}/roles` — List Roles

#### `POST /api/v1/communities/{id}/roles` — Create Role

#### `PATCH /api/v1/communities/{id}/roles/{role_id}` — Update Role

#### `DELETE /api/v1/communities/{id}/roles/{role_id}` — Delete Role

### 12.8 Invites

#### `POST /api/v1/communities/{id}/invites` — Create Invite

#### `GET /api/v1/invites/{code}` — Get Invite Info

#### `POST /api/v1/invites/{code}/accept` — Accept Invite (join community)

#### `DELETE /api/v1/communities/{id}/invites/{code}` — Revoke Invite

### 12.9 Media

#### `POST /api/v1/channels/{id}/attachments` — Request Upload URL

```json
Request:
{
  "filename": "screenshot.png",
  "content_type": "image/png",
  "size_bytes": 245000
}

Response:
{
  "attachment_id": "att_01KPQRST...",
  "upload_url": "https://storage.pod.example.com/upload/...",
  "upload_method": "PUT",
  "expires_at": "2026-02-10T12:10:00Z"
}
```

### 12.10 Audit Log

#### `GET /api/v1/communities/{id}/audit-log` — Get Audit Log

Query params: `user_id`, `action`, `before`, `limit`.

---

## 13. WebSocket Gateway Protocol

### 13.1 Connection

```
wss://pod.example.com/gateway?v=1&encoding=json
```

Supported encodings: `json`, `msgpack`.

### 13.2 Opcodes

| Opcode | Name               | Direction       | Description                            |
| ------ | ------------------ | --------------- | -------------------------------------- |
| 0      | DISPATCH           | Server → Client | Event dispatch (has event name + data) |
| 1      | HEARTBEAT          | Client → Server | Keep-alive ping                        |
| 2      | IDENTIFY           | Client → Server | Auth with WS ticket                    |
| 3      | RESUME             | Client → Server | Resume dropped connection              |
| 4      | VOICE_STATE_UPDATE | Client → Server | Join/leave/update voice                |
| 5      | VOICE_SERVER       | Server → Client | Voice connection details               |
| 6      | HEARTBEAT_ACK      | Server → Client | Heartbeat response                     |
| 7      | RECONNECT          | Server → Client | Server requests client reconnect       |
| 8      | REQUEST_MEMBERS    | Client → Server | Request member list chunk              |
| 9      | PRESENCE_UPDATE    | Client → Server | Update own presence                    |
| 10     | SUBSCRIBE          | Client → Server | Subscribe to channels/events           |
| 11     | UNSUBSCRIBE        | Client → Server | Unsubscribe from channels/events       |

### 13.3 Connection Lifecycle

#### 13.3.1 Identify

```json
{
  "op": 2,
  "d": {
    "ticket": "wst_01KPQRST...",
    "capabilities": 1023,
    "presence": {
      "status": "online",
      "activities": []
    },
    "subscribe": {
      "communities": ["com_01KPQRST..."],
      "channels": ["ch_01KPQRST..."]
    }
  }
}
```

#### 13.3.2 Ready Event

```json
{
  "op": 0,
  "t": "READY",
  "s": 1,
  "d": {
    "session_id": "gw_01KPQRST...",
    "resume_url": "wss://pod.example.com/gateway/resume",
    "user": { ... },
    "communities": [
      {
        "id": "com_01KPQRST...",
        "name": "Gamers United",
        "channels": [ ... ],
        "roles": [ ... ],
        "member_count": 1500
      }
    ],
    "heartbeat_interval": 41250
  }
}
```

#### 13.3.3 Heartbeat

Client MUST send heartbeat at the interval specified in READY.

```json
{ "op": 1, "d": { "seq": 42 } }
```

Server responds:

```json
{ "op": 6, "d": { "ack": 42 } }
```

If the client misses 3 heartbeat ACKs, it MUST reconnect.

#### 13.3.4 Resume

On disconnect, client reconnects and sends:

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

Server replays missed events (up to a buffer limit). If the session has
expired or the buffer is exceeded, server sends op 7 (RECONNECT) and client
must re-IDENTIFY.

### 13.4 Dispatch Events

| Event Name                | Description                      |
| ------------------------- | -------------------------------- |
| `READY`                   | Initial state after IDENTIFY     |
| `RESUMED`                 | Session resumed successfully     |
| `MESSAGE_CREATE`          | New message                      |
| `MESSAGE_UPDATE`          | Message edited                   |
| `MESSAGE_DELETE`          | Message deleted                  |
| `MESSAGE_REACTION_ADD`    | Reaction added                   |
| `MESSAGE_REACTION_REMOVE` | Reaction removed                 |
| `CHANNEL_CREATE`          | Channel created                  |
| `CHANNEL_UPDATE`          | Channel modified                 |
| `CHANNEL_DELETE`          | Channel deleted                  |
| `COMMUNITY_UPDATE`        | Community settings changed       |
| `MEMBER_JOIN`             | New member                       |
| `MEMBER_LEAVE`            | Member left                      |
| `MEMBER_UPDATE`           | Member roles/nickname changed    |
| `ROLE_CREATE`             | Role created                     |
| `ROLE_UPDATE`             | Role modified                    |
| `ROLE_DELETE`             | Role deleted                     |
| `PRESENCE_UPDATE`         | User presence changed            |
| `TYPING_START`            | User started typing              |
| `VOICE_STATE_UPDATE`      | User voice state changed         |
| `VOICE_SERVER_UPDATE`     | Voice server details             |
| `INVITE_CREATE`           | Invite created                   |
| `INVITE_DELETE`           | Invite deleted                   |
| `THREAD_CREATE`           | Thread started                   |
| `THREAD_UPDATE`           | Thread modified (archived, etc.) |
| `THREAD_MEMBERS_UPDATE`   | Thread member list changed       |

### 13.5 Rate Limits

| Scope            | Limit              |
| ---------------- | ------------------ |
| Gateway commands | 120 per 60 seconds |
| IDENTIFY         | 1 per 5 seconds    |
| Presence updates | 5 per 60 seconds   |

### 13.6 Channel Subscriptions

Clients use op 10/11 to manage which channels they receive events for. This
reduces bandwidth for users in large communities.

```json
{
  "op": 10,
  "d": {
    "channels": {
      "ch_01KPQRST...": {
        "messages": true,
        "typing": true,
        "presence": false
      }
    }
  }
}
```

By default, clients receive events for all channels they have `VIEW_CHANNEL`
permission on. Subscriptions allow narrowing this.

---

## 14. Pod Verification & Trust

### 14.1 Verification Levels

| Level        | Badge            | Requirements            |
| ------------ | ---------------- | ----------------------- |
| `unverified` | None             | Registered with Hub     |
| `verified`   | ✓ Blue checkmark | Passed all checks below |
| `managed`    | ✓ Gold badge     | Hosted by Voxora        |

### 14.2 Verification Requirements

#### 14.2.1 Domain Proof

Pod operator must prove control of the Pod's domain via one of:

- **DNS TXT record**: `_voxora-verify.pod.example.com TXT "voxora-verify=<token>"`
- **HTTP well-known**: `https://pod.example.com/.well-known/voxora-verify` returns the token.

#### 14.2.2 Security Checklist

- [ ] TLS with valid certificate (A or A+ on SSL Labs)
- [ ] HTTPS-only (no plaintext HTTP)
- [ ] Up-to-date Pod software (within 2 major versions)
- [ ] No known CVEs in dependencies (automated scan)
- [ ] Rate limiting enabled
- [ ] Webhook signature validation enabled

#### 14.2.3 Policy Compliance

- [ ] Has published community guidelines
- [ ] Has at least one moderator besides the owner
- [ ] Responds to abuse reports within 48 hours
- [ ] Does not host illegal content
- [ ] Cooperates with Hub-level bans

#### 14.2.4 Ongoing Monitoring

- Automated daily checks for TLS validity and uptime.
- Monthly software version check.
- Quarterly policy compliance review.
- Verification revoked after 3 failed checks without remediation.

### 14.3 Trust Signals in Clients

- **Unverified**: Yellow warning icon. "This Pod is not verified. Exercise
  caution with personal information."
- **Verified**: Blue checkmark. "This Pod has been verified by Voxora."
- **Managed**: Gold badge. "This Pod is hosted and managed by Voxora."

### 14.4 Hub-Level Bans

- The Hub can issue **global user bans** for severe abuse (spam botnets,
  CSAM, etc.).
- Banned users cannot obtain SIAs.
- Hub pushes `user.banned` webhook to all Pods.
- Pods SHOULD disconnect banned users immediately.
- Pod operators can appeal Hub bans through a review process.

---

## 15. Managed Pods & Billing

### 15.1 Managed Pod Tiers

| Tier       | Members   | Storage | Voice Slots | Price      |
| ---------- | --------- | ------- | ----------- | ---------- |
| Starter    | 500       | 10 GB   | 25          | $10/mo     |
| Community  | 5,000     | 50 GB   | 100         | $30/mo     |
| Enterprise | 50,000    | 500 GB  | 500         | $100/mo    |
| Custom     | Unlimited | Custom  | Custom      | Contact us |

### 15.2 Managed Pod Features

- Automated provisioning (Pod created within 60 seconds).
- Automated backups (daily, 30-day retention).
- Auto-scaling for voice/video.
- Managed TLS certificates.
- Automatic software updates.
- 99.9% SLA.
- DDoS protection included.
- Custom domain support.

### 15.3 Billing Flow

1. User creates billing account (links Stripe).
2. User selects managed Pod tier.
3. Hub provisions Pod infrastructure.
4. Monthly billing via Stripe.
5. Usage overages billed at end of period.
6. 7-day grace period for failed payments.
7. Pod suspended after grace period; data retained 30 days.

### 15.4 Self-Hosted Pod Costs

Self-hosted Pods are **free** to register with the Hub. The only cost is the
operator's own infrastructure. Verification is also free.

---

## 16. Client Architecture

### 16.1 Shared Core

All clients share a core SDK that handles:

- OIDC authentication flow.
- SIA acquisition and caching.
- Pod connection management (REST + WebSocket).
- WebRTC voice/video.
- Local state management and caching.
- Message rendering (Markdown, embeds, attachments).
- Notification management.

### 16.2 Web Client

| Aspect    | Choice                                    |
| --------- | ----------------------------------------- |
| Framework | React (SPA)                               |
| State     | React Context + Zustand + IndexedDB cache |
| WebSocket | Native WebSocket API                      |
| WebRTC    | Native browser APIs                       |
| Build     | Vite                                      |
| Deploy    | Static hosting + CDN                      |

### 16.3 Desktop Client

| Aspect        | Choice                                   |
| ------------- | ---------------------------------------- |
| Shell         | Electron                                 |
| Frontend      | Same React app                           |
| Extras        | System tray, global hotkeys, auto-update |
| Voice         | WebRTC via Electron (Chromium)           |
| Notifications | OS-native notifications                  |

### 16.4 Mobile Client

| Aspect     | Choice                                     |
| ---------- | ------------------------------------------ |
| Framework  | React Native or Kotlin Multiplatform (TBD) |
| Push       | APNs (iOS), FCM (Android)                  |
| Background | Background audio for voice calls           |
| Storage    | SQLite for local cache                     |

### 16.5 Push Notifications

Since Pods push real-time events via WebSocket, push notifications for mobile
require a relay:

1. Client registers push token with the Hub.
2. Hub provides push token to Pods that the user is a member of.
3. Pod sends notification payload to Hub push relay.
4. Hub push relay dispatches to APNs/FCM.

This keeps APNs/FCM credentials centralized at the Hub, and Pods don't need
to manage push infrastructure.

```
Pod ──notify──► Hub Push Relay ──► APNs / FCM ──► Mobile Client
```

---

## 17. Security

### 17.1 Transport Security

- All Hub and Pod APIs MUST use TLS 1.3 (or TLS 1.2 with AEAD ciphers).
- HSTS headers required.
- Certificate transparency monitoring for Hub domain.

### 17.2 API Security

- **Rate limiting**: Per-IP and per-user token bucket.
  - Hub: 100 req/min per IP unauthenticated, 300 req/min per user.
  - Pod: Configurable, defaults same as Hub.
- **CORS**: Hub allows only registered client origins. Pods allow their own
  domain + client origins.
- **CSRF**: Cookie-based endpoints use double-submit cookie pattern.
- **Input validation**: All inputs validated and sanitized. Max request body
  16 MB.

### 17.3 Message Content Security

- XSS prevention: Message content is rendered as text or sanitized Markdown.
  No raw HTML.
- Attachment scanning: Managed Pods scan uploads for malware.
- Link previews: Proxy-fetched to avoid IP leaks to clients.

### 17.4 End-to-End Encryption (E2EE) — Future

For DMs and private channels, optional E2EE using:

- **Protocol**: Double Ratchet (Signal Protocol).
- **Key Exchange**: X3DH (Extended Triple Diffie-Hellman).
- **Key Storage**: Client-side only. Hub stores pre-key bundles.
- **Scope**: DMs only in MVP. Private channels in future.

E2EE is **not** in MVP scope. When implemented:

- Pod cannot read message content.
- Search, link previews, and moderation are client-side only for E2EE
  channels.
- Users manage their own device keys and cross-signing.

### 17.5 Abuse Reporting

- Users can report messages, users, or communities.
- Reports go to **Pod moderators** first.
- Escalation to **Hub** for cross-Pod abuse or illegal content.
- Hub can force-remove content from managed Pods.
- Hub can issue global bans (see Section 14.4).

### 17.6 Bot Security

- Bots authenticate via Hub OAuth (client credentials).
- Bots have a `BOT` flag on their user record.
- Bot messages are visually distinguished in clients.
- Bots respect the same permission model as users.
- Rate limits for bots: 30 req/min per endpoint by default.

---

## 18. Privacy & Compliance

### 18.1 GDPR

- **Data minimization**: Hub stores only identity data. Pods store only
  community data.
- **Right to access**: Users can export all Hub data. Pod data export is
  per-Pod.
- **Right to deletion**: Account deletion removes Hub data. Hub sends
  `user.deleted` webhook to all Pods, which MUST purge user data within 30
  days.
- **Data portability**: Export formats are JSON and CSV.
- **Consent**: Explicit consent for data processing at registration.
- **DPA**: Data Processing Agreement available for managed Pod customers.

### 18.2 Data Residency

- Hub: Region configurable at deployment (EU, US, etc.).
- Managed Pods: Region selectable at creation.
- Self-hosted Pods: Operator's responsibility.

### 18.3 Data Retention

- Hub: Account data retained until deletion. Logs retained 90 days.
- Managed Pods: Messages retained indefinitely by default. Configurable
  retention policy (30d, 90d, 1y, forever).
- Self-hosted Pods: Operator's policy.

### 18.4 Logging & Audit

- Hub: All admin actions logged. Authentication events logged.
- Pod: Audit log per community (see Section 10.2.11).
- PII in logs is masked or encrypted.

---

## 19. Operational Requirements

### 19.1 Hub Infrastructure

| Component      | Technology        | Scaling                               |
| -------------- | ----------------- | ------------------------------------- |
| API Server     | Rust (Axum)       | Horizontal (stateless)                |
| Database       | PostgreSQL 16     | Primary-replica, pgBouncer            |
| Cache          | Redis 7 (Cluster) | For sessions, rate limits, JWKS cache |
| Object Storage | S3-compatible     | For avatars, media                    |
| Queue          | NATS JetStream    | For async jobs (email, webhooks)      |
| Search         | Meilisearch       | For Pod discovery                     |

### 19.2 Pod Infrastructure (Reference)

| Component      | Technology          | Notes                   |
| -------------- | ------------------- | ----------------------- |
| API Server     | Rust (Axum)         | Single binary           |
| Database       | PostgreSQL 16       | Embedded or external    |
| Cache          | Redis or in-process | Optional for small pods |
| Gateway        | Tokio WebSocket     | Built into API server   |
| SFU            | mediasoup           | Voice/Video             |
| Object Storage | Local FS or S3      | Attachments             |

### 19.3 Performance Targets

| Metric                                     | Target      |
| ------------------------------------------ | ----------- |
| Hub API p99 latency                        | < 100ms     |
| Pod message send p99                       | < 150ms     |
| Gateway event delivery p99                 | < 50ms      |
| Voice join time                            | < 2 seconds |
| SIA validation                             | < 5ms       |
| Concurrent WS connections per Pod instance | 50,000      |

### 19.4 Availability

| Service      | Target                     |
| ------------ | -------------------------- |
| Hub          | 99.9% (8.7h downtime/year) |
| Managed Pods | 99.9%                      |
| TURN         | 99.95%                     |

### 19.5 Monitoring

- Prometheus metrics on all services.
- Grafana dashboards.
- Alerting via PagerDuty/Opsgenie.
- Distributed tracing (OpenTelemetry).
- Error tracking (Sentry).

---

## 20. Migration & Versioning

### 20.1 API Versioning

- URL-based: `/api/v1/`, `/api/v2/`.
- Breaking changes require a new version.
- Old versions supported for 12 months after deprecation.

### 20.2 Gateway Versioning

- Query parameter: `?v=1`.
- Same deprecation policy as REST API.

### 20.3 Database Migrations

- Hub and Pod both use versioned migrations.
- Migrations are forward-only (no down migrations in production).
- Blue-green deployments for zero-downtime upgrades.

### 20.4 Pod Software Updates

- Pod binary is distributed as a single static binary (Linux, macOS, Windows).
- Also available as Docker image.
- Auto-update mechanism (opt-in): Pod checks Hub for latest version.
- Managed Pods are auto-updated on a rolling basis.

---

## 21. Development Phases

### Phase 1: MVP (Months 1-4)

**Goal**: Basic functioning system with text chat.

- [ ] Hub: OIDC provider (authorization code + PKCE)
- [ ] Hub: User registration (email + password)
- [ ] Hub: SIA issuance and JWKS
- [ ] Hub: Pod registry (register, heartbeat, list)
- [ ] Hub: User profiles
- [ ] Pod: SIA validation and local user creation
- [ ] Pod: Community CRUD
- [ ] Pod: Channel CRUD (text only)
- [ ] Pod: Messages (send, edit, delete, history)
- [ ] Pod: Reactions
- [ ] Pod: WebSocket Gateway (core events)
- [ ] Pod: Basic RBAC (admin, moderator, member)
- [ ] Pod: Invites
- [ ] Web Client: Login flow
- [ ] Web Client: Community/channel navigation
- [ ] Web Client: Message sending and receiving
- [ ] Web Client: Basic settings

### Phase 2: Beta (Months 5-8)

**Goal**: Voice, desktop client, and verification.

- [ ] Hub: MFA (TOTP + Passkeys)
- [ ] Hub: Pod verification flow
- [ ] Hub: Push notification relay
- [ ] Hub: TURN credential provisioning
- [ ] Pod: Voice channels (WebRTC SFU)
- [ ] Pod: Threads
- [ ] Pod: Pins
- [ ] Pod: File attachments
- [ ] Pod: Embeds (URL previews)
- [ ] Pod: Audit log
- [ ] Pod: Advanced RBAC (custom roles, channel overrides)
- [ ] Pod: Typing indicators
- [ ] Pod: Presence
- [ ] Desktop Client: Electron shell
- [ ] Desktop Client: System tray, hotkeys
- [ ] Desktop Client: Auto-update
- [ ] Web Client: Voice UI

### Phase 3: GA (Months 9-14)

**Goal**: Video, mobile, managed pods, billing.

- [ ] Hub: Billing (Stripe integration)
- [ ] Hub: Managed Pod provisioning
- [ ] Hub: Social login (GitHub, Google, Apple)
- [ ] Hub: Global search improvements
- [ ] Pod: Video channels
- [ ] Pod: Screen sharing
- [ ] Pod: Forum channels
- [ ] Pod: Stage channels
- [ ] Pod: Bot API
- [ ] Mobile Client: iOS
- [ ] Mobile Client: Android
- [ ] Mobile Client: Push notifications
- [ ] Mobile Client: Background voice

### Phase 4: Scale (Months 15+)

- [ ] E2EE for DMs
- [ ] Pod federation (cross-pod DMs via Hub relay)
- [ ] Custom emoji
- [ ] Stickers
- [ ] User status / activities
- [ ] Rich presence
- [ ] Developer portal
- [ ] Marketplace for bots/integrations
- [ ] Advanced analytics for Pod admins
- [ ] Self-hosted Hub (for enterprise on-prem)

---

## 22. Open Questions

1. **Pod-to-Pod DMs**: Should users on different Pods be able to DM each
   other? If so, messages would need to route through the Hub or a relay.
   This adds significant complexity. Deferred to Phase 4.

2. **Federation depth**: Should Pods be able to "follow" channels on other
   Pods (à la ActivityPub)? This would allow cross-Pod content sharing but
   significantly complicates the trust model.

3. **E2EE scope**: Should E2EE be extended beyond DMs to private channels?
   Key management for groups is substantially harder.

4. **Mobile framework**: React Native vs. Kotlin Multiplatform vs. Flutter.
   Trade-offs between native feel, development speed, and WebRTC support.

5. **SFU implementation**: Build custom SFU in Rust (via webrtc-rs/str0m) or
   use Pion/mediasoup/LiveKit as a sidecar? Custom gives more control;
   existing solutions are more battle-tested.

6. **Message search**: Full-text search within a Pod. Options: PostgreSQL
   FTS, Meilisearch sidecar, Tantivy (Rust-native). Performance and
   deployment complexity trade-offs.

7. **Custom domains for communities**: Should communities within a Pod be
   addressable via custom domains (e.g., `chat.mygame.com` → specific
   community on a Pod)?

8. **Hub high availability**: Single Hub is a SPOF. Options: active-passive
   failover, multi-region active-active with CRDTs for session state.
   Complexity vs. reliability trade-off.

9. **Offline support**: Should clients support offline message drafts and
   queue sends? Adds complexity to the client SDK but improves UX on
   unreliable connections.

10. **Moderation AI**: Should the Hub or Pods integrate AI-based content
    moderation (toxicity detection, spam filtering)? If so, should it be
    opt-in per Pod?

---

## 23. References

- [OAuth 2.1 Draft](https://datatracker.ietf.org/doc/html/draft-ietf-oauth-v2-1-11)
- [OpenID Connect Core 1.0](https://openid.net/specs/openid-connect-core-1_0.html)
- [RFC 7519 — JSON Web Token](https://datatracker.ietf.org/doc/html/rfc7519)
- [RFC 8037 — CFRG Elliptic Curve Diffie-Hellman (X25519, Ed25519)](https://datatracker.ietf.org/doc/html/rfc8037)
- [RFC 6238 — TOTP](https://datatracker.ietf.org/doc/html/rfc6238)
- [WebRTC 1.0](https://www.w3.org/TR/webrtc/)
- [RFC 8656 — TURN](https://datatracker.ietf.org/doc/html/rfc8656)
- [Signal Protocol — Double Ratchet](https://signal.org/docs/specifications/doubleratchet/)
- [Snowflake IDs — Twitter](https://blog.twitter.com/engineering/en_us/a/2010/announcing-snowflake)
- [WebAuthn Level 2](https://www.w3.org/TR/webauthn-2/)
- [Discord API Documentation](https://discord.com/developers/docs) (prior art reference)

---

_End of RFC-0001_
