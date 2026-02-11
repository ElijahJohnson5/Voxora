# Voxora Phase 1 MVP — Implementation Guide

> **Source RFC**: `docs/RFC-0001-voxora-platform.md`
> **Target date**: Months 1–4 of the project timeline
> **Goal**: Basic functioning system with text chat — a user can register on the Hub, log into any Pod with their Hub identity, join communities, and send/receive messages in real time.

---

## Table of Contents

1. [Current State](#1-current-state)
2. [Architecture Overview](#2-architecture-overview)
3. [Shared Conventions](#3-shared-conventions)
4. [Work Streams](#4-work-streams)
5. [WS-1: Hub API](#ws-1-hub-api)
6. [WS-2: Pod API](#ws-2-pod-api)
7. [WS-3: Web Client](#ws-3-web-client)
8. [Shared Library: `voxora-common`](#shared-library-voxora-common)
9. [Database Migrations](#9-database-migrations)
10. [Integration Test Plan](#10-integration-test-plan)
11. [Development Environment](#11-development-environment)
12. [Dependency Reference](#12-dependency-reference)
13. [Task Dependency Graph](#13-task-dependency-graph)
14. [Out of Scope for Phase 1](#14-out-of-scope-for-phase-1)

---

## 1. Current State

The Nx monorepo is initialized with three application shells:

| App        | Location           | Language                | Status                   |
| ---------- | ------------------ | ----------------------- | ------------------------ |
| Hub API    | `apps/hub-api/`    | Rust (Axum)             | Health endpoint only     |
| Pod API    | `apps/pod-api/`    | Rust (Axum)             | Health endpoint only     |
| Web Client | `apps/web-client/` | TypeScript/React (Vite) | Nx scaffold, no app code |

Rust workspace (`Cargo.toml` at root) contains both Rust apps. Nx manages all three projects via `project.json` files. The web client uses Vite, Vitest, and ESLint.

**What exists:**

- Cargo workspace with `hub-api` and `pod-api` members
- Each Rust app has a `/health` endpoint and tracing setup
- React/Vite web client with default Nx scaffold
- Nx targets: `build`, `serve`, `test` for Rust apps; `build`, `serve`, `dev`, `test`, `lint` for web client

**What needs to be built:** Everything below.

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────┐
│               Web Client (SPA)              │
│         React + Vite + Zustand              │
│  Port 4200 (dev)                            │
└──────────┬──────────────────┬───────────────┘
           │ OIDC + REST      │ REST + WebSocket
           ▼                  ▼
┌──────────────────┐  ┌──────────────────────┐
│     Hub API      │  │      Pod API         │
│  Rust / Axum     │  │   Rust / Axum        │
│  Port 4001       │  │   Port 4002          │
│                  │  │                      │
│  PostgreSQL      │  │  PostgreSQL          │
│  Redis           │  │  Redis (optional)    │
└──────────────────┘  └──────────────────────┘
```

### Trust flow (Phase 1)

1. Client authenticates with Hub via OIDC Authorization Code + PKCE
2. Client receives `access_token` (opaque) and `id_token` (JWT)
3. Client requests a SIA (Signed Identity Assertion) from Hub, scoped to a target Pod
4. Client presents SIA to Pod → Pod validates via Hub JWKS → Pod issues PAT + WS ticket
5. Client uses PAT for REST calls, WS ticket for Gateway connection

---

## 3. Shared Conventions

### 3.1 ID Format

All entities use **ULID**-based prefixed IDs:

| Entity      | Prefix  | Example                           |
| ----------- | ------- | --------------------------------- |
| User        | `usr_`  | `usr_01H8MZXK9Q5BNRG7YDZS4A2C3E`  |
| Session     | `ses_`  | `ses_01KPQRST2U3VWXYZ...`         |
| Pod         | `pod_`  | `pod_01J9NXYK3R6CMSH8ZEWTB5D4F7G` |
| Community   | `com_`  | `com_01KPQRST...`                 |
| Channel     | `ch_`   | `ch_01KPQRST...`                  |
| Role        | `role_` | `role_01KPQRST...`                |
| Invite      | (none)  | 8-char alphanumeric code          |
| Message     | (none)  | Snowflake ID (i64)                |
| Attachment  | `att_`  | `att_01KPQRST...`                 |
| Audit entry | `aud_`  | `aud_01KPQRST...`                 |
| SIA JTI     | `sia_`  | `sia_01KPQRST...`                 |

Use the `ulid` crate in Rust. Format: `{prefix}{ulid}`.

### 3.2 Snowflake IDs for Messages

Messages use 64-bit Snowflake IDs (RFC §8.3):

```
Bits 63–22: Timestamp (ms since 2025-01-01T00:00:00Z)  — 42 bits
Bits 21–12: Pod ID shard                                — 10 bits
Bits 11–0:  Sequence                                    — 12 bits
```

Implement a `Snowflake` generator as a shared utility in `voxora-common`.

### 3.3 API Versioning

All endpoints are prefixed with `/api/v1/`. Example: `POST /api/v1/auth/login`.

### 3.4 Error Response Format

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Username must be 2-32 characters",
    "details": [{ "field": "username", "message": "too short" }]
  }
}
```

Use a shared `ApiError` type in each Rust app that implements `IntoResponse`.

### 3.5 Timestamps

All timestamps are RFC 3339 / ISO 8601 in UTC. Stored as `TIMESTAMPTZ` in PostgreSQL.

### 3.6 Pagination

Cursor-based pagination using entity IDs (not offsets). Query params: `before`, `after`, `limit` (default 50, max 100).

Response envelope:

```json
{
  "data": [...],
  "has_more": true
}
```

---

## 4. Work Streams

Phase 1 is organized into three parallel work streams with explicit dependency points.

| Stream | App        | Can Start Immediately      | Blocked On                     |
| ------ | ---------- | -------------------------- | ------------------------------ |
| WS-1   | Hub API    | Yes                        | —                              |
| WS-2   | Pod API    | Partially (DB, CRUD)       | WS-1 (JWKS + SIA) for auth     |
| WS-3   | Web Client | Partially (shell, routing) | WS-1 (OIDC) + WS-2 (REST + WS) |

**Critical path:** Hub OIDC + SIA issuance → Pod SIA validation → Client login flow → Everything else.

---

## WS-1: Hub API

### WS-1.1 Project Structure

```
apps/hub-api/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point, server setup
│   ├── config.rs            # Env-based configuration
│   ├── db/
│   │   ├── mod.rs
│   │   └── pool.rs          # SQLx PgPool setup
│   ├── models/
│   │   ├── mod.rs
│   │   ├── user.rs
│   │   ├── session.rs
│   │   └── pod.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── health.rs
│   │   ├── oidc.rs           # OIDC endpoints
│   │   ├── sia.rs            # SIA issuance
│   │   ├── users.rs          # User registration + profiles
│   │   └── pods.rs           # Pod registry
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── oidc_provider.rs  # Authorization code + PKCE logic
│   │   ├── tokens.rs         # Access/refresh/ID token generation
│   │   ├── sia.rs            # SIA JWT signing
│   │   ├── jwks.rs           # JWKS endpoint + key management
│   │   └── password.rs       # Argon2id hashing
│   ├── middleware/
│   │   ├── mod.rs
│   │   └── auth.rs           # Bearer token extraction + validation
│   └── error.rs              # ApiError type
├── migrations/
│   ├── 001_create_users.sql
│   ├── 002_create_sessions.sql
│   └── 003_create_pods.sql
└── project.json
```

### WS-1.2 Tasks (ordered by dependency)

#### Task H-1: Configuration & Database Setup

**Priority: P0 — must be done first**

- Add dependencies to `Cargo.toml`: `sqlx` (with `postgres`, `runtime-tokio`, `tls-rustls`, `migrate` features), `dotenvy`, `config` or manual env parsing
- Create `config.rs`: reads from env vars (`DATABASE_URL`, `REDIS_URL`, `HUB_DOMAIN`, `SIGNING_KEY_SEED`, `PORT`)
- Create `db/pool.rs`: initialize `sqlx::PgPool`, run migrations on startup
- Create a `docker-compose.yml` at repo root for local PostgreSQL + Redis

**Env vars:**

```env
DATABASE_URL=postgresql://voxora:voxora@localhost:5432/hub
REDIS_URL=redis://localhost:6379
HUB_DOMAIN=http://localhost:4001
PORT=4001
RUST_LOG=hub_api=debug,tower_http=debug
```

#### Task H-2: User Registration

**Priority: P0**
**Depends on: H-1**

Implement `POST /api/v1/users`:

- Request body: `{ username, email, password, display_name }`
- Validate username rules (2–32 chars, `[a-zA-Z0-9_.-]`, case-insensitive uniqueness)
- Validate password (min 10 chars)
- Hash password with Argon2id (use `argon2` crate)
- Generate `usr_` prefixed ULID for ID
- Insert into `users` table
- Return user object (without password hash)
- Return 409 if username or email taken

**DB Table**: `users` — see RFC §10.1.1. For Phase 1, omit `mfa_secret`, `mfa_enabled`, `banner_url`, `bio`. Add them as nullable columns but don't implement MFA logic yet.

#### Task H-3: OIDC Provider — Authorization Code + PKCE

**Priority: P0**
**Depends on: H-1, H-2**

Implement the core OIDC flow. This is the most complex Hub task.

**Endpoints:**

1. `GET /.well-known/openid-configuration` — Discovery document

   ```json
   {
     "issuer": "http://localhost:4001",
     "authorization_endpoint": "http://localhost:4001/oidc/authorize",
     "token_endpoint": "http://localhost:4001/oidc/token",
     "userinfo_endpoint": "http://localhost:4001/oidc/userinfo",
     "jwks_uri": "http://localhost:4001/oidc/.well-known/jwks.json",
     "response_types_supported": ["code"],
     "grant_types_supported": ["authorization_code", "refresh_token"],
     "subject_types_supported": ["public"],
     "id_token_signing_alg_values_supported": ["EdDSA"],
     "scopes_supported": [
       "openid",
       "profile",
       "email",
       "pods",
       "offline_access"
     ],
     "token_endpoint_auth_methods_supported": ["none"],
     "code_challenge_methods_supported": ["S256"]
   }
   ```

2. `GET /oidc/authorize` — Authorization endpoint
   - Query params: `response_type=code`, `client_id`, `redirect_uri`, `scope`, `state`, `code_challenge`, `code_challenge_method=S256`, `nonce`
   - For Phase 1, this will serve a simple login form (server-rendered HTML or redirect to the SPA's login page)
   - On successful login: generate authorization code (opaque, 60-second TTL, stored in Redis), redirect to `redirect_uri?code=...&state=...`
   - Store with the code: `user_id`, `code_challenge`, `redirect_uri`, `scopes`, `nonce`

3. `POST /oidc/token` — Token endpoint
   - Grant type `authorization_code`: validate code, verify PKCE (`code_verifier` → SHA256 → compare to `code_challenge`), return `{ access_token, refresh_token, id_token, token_type, expires_in, scope }`
   - Grant type `refresh_token`: validate refresh token, issue new access + refresh tokens (rotate refresh token)
   - Access token: opaque string (`hat_` prefix), 15-minute TTL, stored in Redis with user_id + scopes
   - Refresh token: opaque string (`hrt_` prefix), 30-day sliding TTL, stored in `sessions` table
   - ID token: signed JWT (EdDSA / Ed25519) with `sub`, `iss`, `aud`, `exp`, `iat`, `nonce`, plus profile claims based on scopes

4. `GET /oidc/userinfo` — UserInfo endpoint
   - Requires Bearer access token
   - Returns claims based on token scopes

5. `GET /oidc/.well-known/jwks.json` — JWKS endpoint (see H-4)

6. `POST /oidc/revoke` — Revoke a token
   - Accept `token` + `token_type_hint` (access_token / refresh_token)
   - Revoke in Redis / DB

**Implementation notes:**

- Use `ed25519-dalek` for Ed25519 key generation and signing
- Use `jsonwebtoken` crate for JWT encoding/decoding (supports EdDSA)
- Store authorization codes in Redis with 60s TTL
- Store access tokens in Redis with 900s (15min) TTL
- The web client is a public client (no client_secret); PKCE is mandatory
- For Phase 1, use a single hardcoded `client_id` for the web client (e.g., `voxora-web`)

#### Task H-4: JWKS & Key Management

**Priority: P0**
**Depends on: H-1**

- On startup, generate or load an Ed25519 keypair for SIA + ID token signing
- For Phase 1, derive the key deterministically from `SIGNING_KEY_SEED` env var (in production this would come from a KMS)
- Expose `GET /oidc/.well-known/jwks.json` with the public key in JWK format:
  ```json
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
- The `kid` should include a version or date identifier
- Key rotation is out of scope for Phase 1, but the structure should support multiple keys in the array

#### Task H-5: SIA Issuance

**Priority: P0**
**Depends on: H-3, H-4**

Implement `POST /api/v1/oidc/sia`:

- Requires Bearer access token with `pods` scope
- Request body: `{ "pod_id": "pod_01J9NXYK..." }`
- Validate pod_id exists and is active in the registry
- Sign a JWT with:
  ```json
  {
    "alg": "EdDSA",
    "kid": "<key-id>",
    "typ": "voxora-sia+jwt"
  }
  {
    "iss": "http://localhost:4001",
    "sub": "usr_01H8MZ...",
    "aud": "<pod_id>",
    "iat": <now>,
    "exp": <now + 300>,
    "jti": "sia_<ulid>",
    "username": "alice",
    "display_name": "Alice",
    "avatar_url": null,
    "email": "alice@example.com",
    "email_verified": true,
    "flags": [],
    "hub_version": 1
  }
  ```
- Return `{ "sia": "<jwt>", "expires_at": "..." }`
- SIA lifetime: 5 minutes

#### Task H-6: Pod Registry

**Priority: P1**
**Depends on: H-3**

Implement Pod registration and discovery:

1. `POST /api/v1/pods/register` — Register a new Pod
   - Requires Bearer access token
   - Creates pod record, generates `pod_` ID, generates `client_id` + `client_secret` for the Pod
   - Returns `{ pod_id, client_id, client_secret, status }`

2. `POST /api/v1/pods/{pod_id}/heartbeat` — Pod heartbeat
   - Authenticated via client credentials (Pod's `client_id` + `client_secret` as Basic auth or Bearer from client_credentials grant)
   - Updates `last_heartbeat`, `member_count`, `online_count`, etc.

3. `GET /api/v1/pods` — List Pods
   - Public (or with access token)
   - Supports `?sort=popular|newest`, `?page=1`, `?per_page=25`
   - Only returns `status=active` pods

4. `GET /api/v1/pods/{pod_id}` — Pod details

**DB Table**: `pods` — see RFC §10.1.5. For Phase 1, omit `verification`, `managed`, billing-related fields. Default `verification` to `unverified`.

#### Task H-7: User Profiles

**Priority: P1**
**Depends on: H-3**

1. `GET /api/v1/users/@me` — Current user (from access token)
2. `PATCH /api/v1/users/@me` — Update profile (display_name, avatar_url)
3. `GET /api/v1/users/{user_id}` — Public profile (id, username, display_name, avatar_url, created_at)
4. `GET /api/v1/users/@me/pods` — List Pods user is a member of (stored as bookmarks on Hub)

#### Task H-8: Auth Middleware

**Priority: P0**
**Depends on: H-3**

Create an Axum extractor that:

1. Reads `Authorization: Bearer <token>` header
2. Looks up the opaque access token in Redis
3. Returns `AuthUser { user_id, scopes }` or 401
4. Wrap protected routes with this extractor

---

## WS-2: Pod API

### WS-2.1 Project Structure

```
apps/pod-api/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── db/
│   │   ├── mod.rs
│   │   └── pool.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── user.rs           # pod_users
│   │   ├── community.rs
│   │   ├── channel.rs
│   │   ├── message.rs
│   │   ├── member.rs
│   │   ├── role.rs
│   │   ├── reaction.rs
│   │   └── invite.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── health.rs
│   │   ├── auth.rs           # SIA login + PAT refresh
│   │   ├── communities.rs
│   │   ├── channels.rs
│   │   ├── messages.rs
│   │   ├── members.rs
│   │   ├── roles.rs
│   │   ├── reactions.rs
│   │   └── invites.rs
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── sia_validator.rs  # Fetch JWKS, validate SIA JWTs
│   │   ├── pat.rs            # PAT generation + validation
│   │   └── permissions.rs    # RBAC bitfield logic
│   ├── gateway/
│   │   ├── mod.rs
│   │   ├── server.rs         # WebSocket upgrade + connection mgmt
│   │   ├── session.rs        # Per-connection state
│   │   ├── events.rs         # Event types + serialization
│   │   ├── handler.rs        # Incoming opcode dispatch
│   │   └── fanout.rs         # Channel-based broadcast
│   ├── middleware/
│   │   ├── mod.rs
│   │   └── auth.rs           # PAT extraction
│   ├── snowflake.rs          # (or in voxora-common)
│   └── error.rs
├── migrations/
│   ├── 001_create_pod_users.sql
│   ├── 002_create_communities.sql
│   ├── 003_create_roles.sql
│   ├── 004_create_channels.sql
│   ├── 005_create_channel_overrides.sql
│   ├── 006_create_community_members.sql
│   ├── 007_create_messages.sql
│   ├── 008_create_reactions.sql
│   ├── 009_create_invites.sql
│   └── 010_create_audit_log.sql
└── project.json
```

### WS-2.2 Tasks (ordered by dependency)

#### Task P-1: Configuration & Database Setup

**Priority: P0**

Same pattern as H-1 but for the Pod database.

**Env vars:**

```env
DATABASE_URL=postgresql://voxora:voxora@localhost:5432/pod
HUB_URL=http://localhost:4001
POD_ID=pod_01J9NXYK3R6CMSH8ZEWTB5D4F7G
POD_CLIENT_ID=pod_client_01KPQRST
POD_CLIENT_SECRET=vxs_...
PORT=4002
RUST_LOG=pod_api=debug,tower_http=debug
```

#### Task P-2: SIA Validation & Local User Creation

**Priority: P0**
**Depends on: P-1, Hub H-4 (published JWKS)**

Implement `POST /api/v1/auth/login`:

- Request body: `{ "sia": "<jwt>" }`
- Fetch Hub JWKS from `{HUB_URL}/oidc/.well-known/jwks.json` (cache for 1 hour, re-fetch on `kid` miss)
- Validate SIA:
  1. Verify signature using Ed25519 public key from JWKS
  2. Check `exp` — reject if expired
  3. Check `aud` — must match this Pod's `POD_ID`
  4. Check `iss` — must match `HUB_URL`
  5. Check `jti` — reject if seen before (maintain in-memory or Redis cache for 5 minutes)
- Create or update `pod_users` record (upsert on `id = sub`)
- Generate PAT: opaque token (`pat_` prefix), 1-hour TTL, stored in Redis or DB
- Generate WS ticket: opaque token (`wst_` prefix), 30-second TTL, single-use, stored in Redis
- Return:
  ```json
  {
    "access_token": "pat_...",
    "token_type": "Bearer",
    "expires_in": 3600,
    "refresh_token": "prt_...",
    "ws_ticket": "wst_...",
    "ws_url": "ws://localhost:4002/gateway",
    "user": { "id", "username", "display_name", "avatar_url", "roles": ["member"], "joined_at" }
  }
  ```

Implement `POST /api/v1/auth/refresh`:

- Request body: `{ "refresh_token": "prt_..." }`
- Validate, rotate, return new PAT + refresh token

#### Task P-3: Community CRUD

**Priority: P1**
**Depends on: P-2**

1. `POST /api/v1/communities` — Create community
   - Requires authenticated user (PAT)
   - Creates community, sets creator as owner
   - Auto-creates `@everyone` role (position 0, default permissions)
   - Auto-creates `#general` text channel
   - Auto-adds creator as member with owner role
   - Returns community with channels and roles

2. `GET /api/v1/communities` — List communities on this Pod
   - Public or authenticated

3. `GET /api/v1/communities/{id}` — Get community details
   - Includes channels, roles, member_count

4. `PATCH /api/v1/communities/{id}` — Update community
   - Requires `MANAGE_COMMUNITY` permission
   - Updatable: name, description, icon_url

5. `DELETE /api/v1/communities/{id}` — Delete community
   - Requires owner

#### Task P-4: Channel CRUD (Text Only)

**Priority: P1**
**Depends on: P-3**

1. `GET /api/v1/communities/{id}/channels` — List channels
   - Filtered by `VIEW_CHANNEL` permission

2. `POST /api/v1/communities/{id}/channels` — Create channel
   - Requires `MANAGE_CHANNELS` permission
   - Type: text only for Phase 1 (type = 0)
   - Fields: name, topic, position, slowmode_seconds, nsfw

3. `PATCH /api/v1/channels/{id}` — Update channel

4. `DELETE /api/v1/channels/{id}` — Delete channel

#### Task P-5: Messages

**Priority: P1**
**Depends on: P-4, Snowflake generator**

1. `POST /api/v1/channels/{id}/messages` — Send message
   - Requires `SEND_MESSAGES` permission
   - Request: `{ content, nonce, reply_to? }`
   - Generate Snowflake ID
   - Persist to DB
   - Publish to Gateway fanout (see P-8)
   - Return message object

2. `GET /api/v1/channels/{id}/messages` — Get message history
   - Cursor-based: `?before=<id>&limit=50`
   - Also supports `after` and `around`

3. `PATCH /api/v1/channels/{channel_id}/messages/{id}` — Edit message
   - Only by author
   - Sets `edited_at`

4. `DELETE /api/v1/channels/{channel_id}/messages/{id}` — Delete message
   - By author OR by user with `MANAGE_MESSAGES`

#### Task P-6: Reactions

**Priority: P2**
**Depends on: P-5**

1. `PUT /api/v1/channels/{channel_id}/messages/{id}/reactions/{emoji}` — Add reaction
   - Requires `USE_REACTIONS` permission
   - Max 20 unique emoji per message

2. `DELETE /api/v1/channels/{channel_id}/messages/{id}/reactions/{emoji}` — Remove own reaction

Reactions are broadcast via Gateway as `MESSAGE_REACTION_ADD` / `MESSAGE_REACTION_REMOVE`.

#### Task P-7: Basic RBAC

**Priority: P1**
**Depends on: P-3**

Implement the permission bitfield system (RFC §7.2):

**Phase 1 permissions (minimum set):**

| Permission         | Bit |
| ------------------ | --- |
| `VIEW_CHANNEL`     | 0   |
| `SEND_MESSAGES`    | 1   |
| `MANAGE_MESSAGES`  | 3   |
| `MANAGE_CHANNELS`  | 4   |
| `MANAGE_COMMUNITY` | 5   |
| `MANAGE_ROLES`     | 6   |
| `KICK_MEMBERS`     | 7   |
| `BAN_MEMBERS`      | 8   |
| `INVITE_MEMBERS`   | 9   |
| `USE_REACTIONS`    | 16  |
| `ADMINISTRATOR`    | 31  |

**Default roles for Phase 1:**

| Role             | Permissions                                                        |
| ---------------- | ------------------------------------------------------------------ |
| `@everyone`      | `VIEW_CHANNEL \| SEND_MESSAGES \| USE_REACTIONS \| INVITE_MEMBERS` |
| `moderator`      | Above + `MANAGE_MESSAGES \| KICK_MEMBERS \| BAN_MEMBERS`           |
| `admin`          | Above + `MANAGE_CHANNELS \| MANAGE_COMMUNITY \| MANAGE_ROLES`      |
| Owner (implicit) | `ADMINISTRATOR` (all permissions)                                  |

**Permission resolution:**

```
effective = (union of all role allows) & ~(union of all role denies)
if effective & ADMINISTRATOR != 0 { return ALL_PERMISSIONS }
```

Channel overrides are out of scope for Phase 1 (schema created but not enforced).

**Endpoints:**

1. `GET /api/v1/communities/{id}/roles` — List roles
2. `POST /api/v1/communities/{id}/roles` — Create role (requires `MANAGE_ROLES`)
3. `PATCH /api/v1/communities/{id}/roles/{role_id}` — Update role
4. `DELETE /api/v1/communities/{id}/roles/{role_id}` — Delete role

**Member management:**

1. `GET /api/v1/communities/{id}/members` — List members
2. `GET /api/v1/communities/{id}/members/{user_id}` — Get member
3. `PATCH /api/v1/communities/{id}/members/{user_id}` — Update member (add/remove roles, set nickname)
4. `DELETE /api/v1/communities/{id}/members/{user_id}` — Kick member (requires `KICK_MEMBERS`)
5. `PUT /api/v1/communities/{id}/bans/{user_id}` — Ban member
6. `DELETE /api/v1/communities/{id}/bans/{user_id}` — Unban

#### Task P-8: WebSocket Gateway (Core Events)

**Priority: P0**
**Depends on: P-2**

Implement the WebSocket Gateway at `GET /gateway` (upgrades to WebSocket).

**Opcodes for Phase 1:**

| Op  | Name          | Direction       |
| --- | ------------- | --------------- |
| 0   | DISPATCH      | Server → Client |
| 1   | HEARTBEAT     | Client → Server |
| 2   | IDENTIFY      | Client → Server |
| 6   | HEARTBEAT_ACK | Server → Client |

**Connection lifecycle:**

1. Client connects to `ws://localhost:4002/gateway?v=1&encoding=json`
2. Client sends IDENTIFY with WS ticket:
   ```json
   { "op": 2, "d": { "ticket": "wst_..." } }
   ```
3. Server validates ticket (single-use, 30s TTL), resolves user
4. Server sends READY:
   ```json
   {
     "op": 0, "t": "READY", "s": 1,
     "d": {
       "session_id": "gw_...",
       "user": { ... },
       "communities": [ { "id", "name", "channels": [...], "roles": [...], "member_count" } ],
       "heartbeat_interval": 41250
     }
   }
   ```
5. Client sends HEARTBEAT every `heartbeat_interval` ms:
   ```json
   { "op": 1, "d": { "seq": 42 } }
   ```
6. Server responds with HEARTBEAT_ACK:
   ```json
   { "op": 6, "d": { "ack": 42 } }
   ```

**Dispatch events for Phase 1:**

| Event                     | Trigger                    |
| ------------------------- | -------------------------- |
| `READY`                   | After IDENTIFY             |
| `MESSAGE_CREATE`          | New message                |
| `MESSAGE_UPDATE`          | Message edited             |
| `MESSAGE_DELETE`          | Message deleted            |
| `MESSAGE_REACTION_ADD`    | Reaction added             |
| `MESSAGE_REACTION_REMOVE` | Reaction removed           |
| `CHANNEL_CREATE`          | Channel created            |
| `CHANNEL_UPDATE`          | Channel modified           |
| `CHANNEL_DELETE`          | Channel deleted            |
| `COMMUNITY_UPDATE`        | Community settings changed |
| `MEMBER_JOIN`             | New member                 |
| `MEMBER_LEAVE`            | Member left                |
| `MEMBER_UPDATE`           | Roles/nickname changed     |

**Fanout architecture:**

- Maintain an in-memory map: `channel_id → HashSet<ConnectionId>`
- When a message is created (via REST), publish the event to all connections subscribed to that channel
- For Phase 1, single-process fanout is sufficient (no Redis pub/sub needed)
- Use `tokio::sync::broadcast` or per-connection `mpsc` channels

**Rate limit:** 120 commands per 60 seconds per connection.

#### Task P-9: Invites

**Priority: P2**
**Depends on: P-3**

1. `POST /api/v1/communities/{id}/invites` — Create invite
   - Requires `INVITE_MEMBERS` permission
   - Generate 8-char alphanumeric code
   - Optional: `max_uses`, `max_age_seconds`
   - Returns invite object

2. `GET /api/v1/invites/{code}` — Get invite info (public)
   - Returns community name, icon, member count, inviter

3. `POST /api/v1/invites/{code}/accept` — Accept invite (join community)
   - Requires authenticated user (PAT)
   - Adds user as community member with `@everyone` role
   - Broadcasts `MEMBER_JOIN` via Gateway

4. `DELETE /api/v1/communities/{id}/invites/{code}` — Revoke invite

---

## WS-3: Web Client

### WS-3.1 Project Structure

```
apps/web-client/
├── src/
│   ├── main.tsx
│   ├── app/
│   │   └── app.tsx              # Root component + router
│   ├── pages/
│   │   ├── login.tsx            # OIDC login initiation
│   │   ├── callback.tsx         # OIDC redirect callback
│   │   ├── home.tsx             # Pod browser / community list
│   │   └── community.tsx        # Main chat view
│   ├── components/
│   │   ├── layout/
│   │   │   ├── sidebar.tsx      # Community list + channel list
│   │   │   ├── chat-area.tsx    # Message list + input
│   │   │   ├── member-list.tsx  # Right sidebar
│   │   │   └── header.tsx       # Channel header
│   │   ├── messages/
│   │   │   ├── message.tsx      # Single message component
│   │   │   ├── message-list.tsx # Scrollable message list
│   │   │   ├── message-input.tsx
│   │   │   └── reaction.tsx
│   │   ├── auth/
│   │   │   └── protected-route.tsx
│   │   └── settings/
│   │       └── user-settings.tsx
│   ├── stores/
│   │   ├── auth.ts              # OIDC tokens, SIA, Hub auth state
│   │   ├── pod.ts               # Current Pod connection, PAT
│   │   ├── communities.ts       # Community + channel state
│   │   └── messages.ts          # Message cache per channel
│   ├── lib/
│   │   ├── api/
│   │   │   ├── hub.ts           # Hub API client
│   │   │   └── pod.ts           # Pod API client
│   │   ├── gateway/
│   │   │   ├── connection.ts    # WebSocket connection manager
│   │   │   ├── events.ts        # Event type definitions
│   │   │   └── handler.ts       # Dispatch event handler
│   │   ├── oidc.ts              # PKCE helpers, auth flow
│   │   └── utils.ts
│   ├── types/
│   │   ├── api.ts               # API request/response types
│   │   ├── gateway.ts           # WebSocket event types
│   │   └── models.ts            # Shared model types
│   └── styles/
│       └── styles.css
├── index.html
├── vite.config.mts
└── package.json
```

### WS-3.2 Additional Dependencies

Install in `apps/web-client`:

```
pnpm add zustand react-router-dom
pnpm add -D @types/react-router-dom
```

### WS-3.3 Tasks (ordered by dependency)

#### Task C-1: Routing & Layout Shell

**Priority: P0**

- Install `react-router-dom`
- Set up routes:
  - `/login` → Login page
  - `/callback` → OIDC callback handler
  - `/` → Home (pod browser, redirect to login if unauthenticated)
  - `/community/:communityId` → Community view
  - `/community/:communityId/channel/:channelId` → Channel view
- Create the three-panel layout shell (sidebar | chat | member list) with placeholder content
- Create `ProtectedRoute` component that redirects to `/login` if no auth state

#### Task C-2: OIDC Login Flow

**Priority: P0**
**Depends on: Hub H-3**

Implement the full PKCE flow:

1. **Login page**: "Login with Voxora" button
2. **On click**:
   - Generate `code_verifier` (random 64-byte base64url string)
   - Compute `code_challenge = base64url(SHA-256(code_verifier))`
   - Generate `state` (random 32-byte string)
   - Store `code_verifier` and `state` in `sessionStorage`
   - Redirect to Hub: `{HUB_URL}/oidc/authorize?response_type=code&client_id=voxora-web&redirect_uri={CALLBACK_URL}&scope=openid+profile+email+pods+offline_access&state={state}&code_challenge={challenge}&code_challenge_method=S256`
3. **Callback page** (`/callback?code=...&state=...`):
   - Verify `state` matches sessionStorage
   - Exchange code for tokens: `POST {HUB_URL}/oidc/token` with `{ grant_type: "authorization_code", code, redirect_uri, code_verifier, client_id: "voxora-web" }`
   - Store `access_token`, `refresh_token`, `id_token` in Zustand auth store (persisted to `localStorage`)
   - Redirect to `/`

4. **Token refresh**: Background timer that refreshes access token 1 minute before expiry using refresh_token grant

5. **Logout**: Clear all tokens, redirect to `/login`

#### Task C-3: Pod Connection Flow

**Priority: P0**
**Depends on: C-2, Pod P-2**

1. Request SIA from Hub: `POST {HUB_URL}/api/v1/oidc/sia` with `{ pod_id }` using Hub access token
2. Login to Pod: `POST {POD_URL}/api/v1/auth/login` with `{ sia }`
3. Store PAT, refresh_token, ws_ticket, ws_url in Zustand pod store
4. Connect to Gateway WebSocket using ws_ticket (see C-5)
5. On PAT expiry: use Pod refresh token to get new PAT

#### Task C-4: Community & Channel Navigation

**Priority: P1**
**Depends on: C-3**

- **Sidebar (left)**:
  - Top section: list of communities the user is a member of (from READY event)
  - Click community → show its channels below
  - Channel list grouped by position
  - Click channel → navigate to `/community/{id}/channel/{channelId}`
  - Active channel highlighted

- **Header**: Shows current channel name and topic

- **Join flow**: "Join Community" button → enter invite code → `POST /invites/{code}/accept` → refresh community list

#### Task C-5: WebSocket Gateway Client

**Priority: P0**
**Depends on: C-3**

Implement a `GatewayConnection` class/module:

1. Connect to `ws_url` from login response
2. Send IDENTIFY with `ws_ticket`
3. On READY: populate community/channel/member stores
4. Start heartbeat interval from READY's `heartbeat_interval`
5. On DISPATCH events: update Zustand stores
   - `MESSAGE_CREATE` → append to message store
   - `MESSAGE_UPDATE` → update in message store
   - `MESSAGE_DELETE` → remove from message store
   - `MESSAGE_REACTION_ADD/REMOVE` → update reactions on message
   - `CHANNEL_CREATE/UPDATE/DELETE` → update channel store
   - `COMMUNITY_UPDATE` → update community store
   - `MEMBER_JOIN/LEAVE/UPDATE` → update member store
6. On close: attempt reconnect with exponential backoff (1s, 2s, 4s, 8s, max 30s)

#### Task C-6: Message Sending & Receiving

**Priority: P1**
**Depends on: C-5, Pod P-5**

- **Message list**: Scrollable container showing messages for the current channel
  - Load initial history via REST: `GET /channels/{id}/messages?limit=50`
  - New messages arrive via Gateway `MESSAGE_CREATE`
  - Scroll to bottom on new message (if already at bottom)
  - Infinite scroll up: load older messages with `?before=<oldest_id>`
  - Display: avatar, username, timestamp, content, edited indicator
  - Render message content as plain text (Markdown rendering can come later)

- **Message input**: Text input at bottom of chat area
  - Send on Enter (Shift+Enter for newline)
  - `POST /channels/{id}/messages` with `{ content, nonce: uuid() }`
  - Optimistic insert: show message immediately with pending state, reconcile with `MESSAGE_CREATE` from Gateway using `nonce`
  - Edit: click own message → inline edit → `PATCH /channels/{channel_id}/messages/{id}`
  - Delete: click own message → delete → `DELETE /channels/{channel_id}/messages/{id}`

- **Reactions**: Click emoji picker on hover → `PUT .../reactions/{emoji}`. Display reaction counts below message.

#### Task C-7: Basic Settings

**Priority: P2**
**Depends on: C-2**

- User settings page accessible from sidebar user area
- Display current user info (from Hub)
- Allow editing display_name via `PATCH {HUB_URL}/api/v1/users/@me`
- Logout button

---

## Shared Library: `voxora-common`

Create a new Cargo library to share code between Hub and Pod:

```
libs/voxora-common/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── id.rs         # ULID generation with prefixes
│   ├── snowflake.rs  # Snowflake ID generator
│   ├── error.rs      # Shared error types + ApiError
│   └── models.rs     # Shared types (UserFlags, etc.)
```

Add to workspace `Cargo.toml`:

```toml
members = [
  "apps/hub-api",
  "apps/pod-api",
  "libs/voxora-common"
]
```

Both `hub-api` and `pod-api` depend on `voxora-common = { path = "../../libs/voxora-common" }`.

---

## 9. Database Migrations

### Hub Database

Run against `hub` PostgreSQL database. Use `sqlx-cli` for migration management.

```sql
-- 001_create_users.sql
CREATE TABLE users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    username_lower  TEXT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL,
    email           TEXT UNIQUE,
    email_verified  BOOLEAN NOT NULL DEFAULT FALSE,
    password_hash   TEXT,
    avatar_url      TEXT,
    flags           BIGINT NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 002_create_sessions.sql
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
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

-- 003_create_pods.sql
CREATE TABLE pods (
    id              TEXT PRIMARY KEY,
    owner_id        TEXT NOT NULL REFERENCES users(id),
    name            TEXT NOT NULL,
    description     TEXT,
    icon_url        TEXT,
    url             TEXT NOT NULL,
    region          TEXT,
    client_id       TEXT NOT NULL UNIQUE,
    client_secret   TEXT NOT NULL,
    public          BOOLEAN NOT NULL DEFAULT TRUE,
    capabilities    TEXT[] NOT NULL DEFAULT '{"text"}',
    max_members     INTEGER NOT NULL DEFAULT 10000,
    version         TEXT,
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
```

### Pod Database

Run against `pod` PostgreSQL database.

```sql
-- 001_create_pod_users.sql
CREATE TABLE pod_users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    avatar_url      TEXT,
    hub_flags       BIGINT NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'active',
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 002_create_communities.sql
CREATE TABLE communities (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    icon_url        TEXT,
    owner_id        TEXT NOT NULL REFERENCES pod_users(id),
    default_channel TEXT,
    member_count    INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 003_create_roles.sql
CREATE TABLE roles (
    id              TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    color           INTEGER,
    position        INTEGER NOT NULL DEFAULT 0,
    permissions     BIGINT NOT NULL DEFAULT 0,
    mentionable     BOOLEAN NOT NULL DEFAULT FALSE,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_roles_community ON roles(community_id);

-- 004_create_channels.sql
CREATE TABLE channels (
    id              TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    parent_id       TEXT REFERENCES channels(id),
    name            TEXT NOT NULL,
    topic           TEXT,
    type            SMALLINT NOT NULL DEFAULT 0,
    position        INTEGER NOT NULL DEFAULT 0,
    slowmode_seconds INTEGER NOT NULL DEFAULT 0,
    nsfw            BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_channels_community ON channels(community_id);

-- 005_create_channel_overrides.sql
CREATE TABLE channel_overrides (
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    target_type     SMALLINT NOT NULL,
    target_id       TEXT NOT NULL,
    allow           BIGINT NOT NULL DEFAULT 0,
    deny            BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, target_type, target_id)
);

-- 006_create_community_members.sql
CREATE TABLE community_members (
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    nickname        TEXT,
    roles           TEXT[] NOT NULL DEFAULT '{}',
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (community_id, user_id)
);
CREATE INDEX idx_members_user ON community_members(user_id);

-- 007_create_messages.sql
CREATE TABLE messages (
    id              BIGINT PRIMARY KEY,
    channel_id      TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id       TEXT NOT NULL REFERENCES pod_users(id),
    content         TEXT,
    type            SMALLINT NOT NULL DEFAULT 0,
    flags           INTEGER NOT NULL DEFAULT 0,
    reply_to        BIGINT,
    edited_at       TIMESTAMPTZ,
    pinned          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_messages_channel ON messages(channel_id, id DESC);
CREATE INDEX idx_messages_author ON messages(author_id);

-- 008_create_reactions.sql
CREATE TABLE reactions (
    message_id      BIGINT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES pod_users(id),
    emoji           TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, user_id, emoji)
);

-- 009_create_invites.sql
CREATE TABLE invites (
    code            TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    channel_id      TEXT REFERENCES channels(id),
    inviter_id      TEXT NOT NULL REFERENCES pod_users(id),
    max_uses        INTEGER,
    use_count       INTEGER NOT NULL DEFAULT 0,
    max_age_seconds INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ
);

-- 010_create_audit_log.sql
CREATE TABLE audit_log (
    id              TEXT PRIMARY KEY,
    community_id    TEXT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    actor_id        TEXT NOT NULL REFERENCES pod_users(id),
    action          TEXT NOT NULL,
    target_type     TEXT,
    target_id       TEXT,
    changes         JSONB,
    reason          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_audit_community ON audit_log(community_id, created_at DESC);
```

---

## 10. Integration Test Plan

These are end-to-end flows that validate the system works as a whole. Run after individual unit tests pass.

### Test 1: Full Auth Flow

1. Register user via Hub (`POST /api/v1/users`)
2. Complete OIDC login (authorize → token exchange)
3. Request SIA for a Pod
4. Login to Pod with SIA
5. Verify PAT works for Pod REST API calls
6. Verify WS ticket allows Gateway connection
7. Verify READY event received

### Test 2: Message Round-Trip

1. Authenticate (as above)
2. Create community
3. Send message via REST
4. Verify `MESSAGE_CREATE` event received on Gateway
5. Fetch message history via REST — verify message present

### Test 3: Multi-User Chat

1. Register two users, both connect to same Pod
2. Both join the same community
3. User A sends message
4. Verify User B receives `MESSAGE_CREATE` via Gateway
5. User B reacts to the message
6. Verify User A receives `MESSAGE_REACTION_ADD`

### Test 4: Invite Flow

1. User A creates a community
2. User A creates an invite
3. User B fetches invite info (unauthenticated)
4. User B accepts invite (authenticated)
5. Verify User A receives `MEMBER_JOIN` via Gateway

### Test 5: RBAC Enforcement

1. User creates community (is owner)
2. User B joins via invite (is member with `@everyone` role)
3. User B tries to create a channel → 403 Forbidden
4. Owner grants `MANAGE_CHANNELS` to User B's role
5. User B creates a channel → 201 Created

---

## 11. Development Environment

### docker-compose.yml (at repo root)

```yaml
services:
  hub-db:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: hub
      POSTGRES_USER: voxora
      POSTGRES_PASSWORD: voxora
    ports:
      - "5432:5432"
    volumes:
      - hub-db-data:/var/lib/postgresql/data

  pod-db:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: pod
      POSTGRES_USER: voxora
      POSTGRES_PASSWORD: voxora
    ports:
      - "5433:5432"
    volumes:
      - pod-db-data:/var/lib/postgresql/data

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"

volumes:
  hub-db-data:
  pod-db-data:
```

### .env files

Create `.env` files for each app (gitignored):

**`apps/hub-api/.env`**

```env
DATABASE_URL=postgresql://voxora:voxora@localhost:5432/hub
REDIS_URL=redis://localhost:6379/0
HUB_DOMAIN=http://localhost:4001
SIGNING_KEY_SEED=dev-seed-do-not-use-in-production
PORT=4001
RUST_LOG=hub_api=debug,tower_http=debug
```

**`apps/pod-api/.env`**

```env
DATABASE_URL=postgresql://voxora:voxora@localhost:5433/pod
REDIS_URL=redis://localhost:6379/1
HUB_URL=http://localhost:4001
POD_ID=pod_dev_local
PORT=4002
RUST_LOG=pod_api=debug,tower_http=debug
```

### Running

```bash
# Start infrastructure
docker compose up -d

# Run migrations (once sqlx-cli is installed)
cargo install sqlx-cli --no-default-features --features postgres
cd apps/hub-api && sqlx migrate run
cd apps/pod-api && sqlx migrate run

# Start services (in separate terminals or via Nx)
pnpm nx serve hub-api
pnpm nx serve pod-api
pnpm nx serve web-client
```

---

## 12. Dependency Reference

### Rust Crates (Hub + Pod shared)

| Crate                                                     | Purpose            |
| --------------------------------------------------------- | ------------------ |
| `axum` 0.7                                                | HTTP framework     |
| `tokio` 1.x                                               | Async runtime      |
| `serde` / `serde_json`                                    | Serialization      |
| `sqlx` 0.8 (postgres, runtime-tokio, tls-rustls, migrate) | Database           |
| `tracing` / `tracing-subscriber`                          | Structured logging |
| `dotenvy`                                                 | .env file loading  |
| `ulid`                                                    | ULID generation    |
| `tower-http` (cors, trace)                                | HTTP middleware    |
| `chrono` (serde feature)                                  | Timestamps         |

### Hub-Only Crates

| Crate                        | Purpose                          |
| ---------------------------- | -------------------------------- |
| `ed25519-dalek`              | Ed25519 key generation + signing |
| `jsonwebtoken`               | JWT encoding/decoding            |
| `argon2`                     | Password hashing                 |
| `rand`                       | Cryptographic random generation  |
| `base64`                     | Base64url encoding for PKCE, JWK |
| `sha2`                       | SHA-256 for PKCE code_challenge  |
| `redis` (tokio-comp feature) | Redis client                     |

### Pod-Only Crates

| Crate                        | Purpose                        |
| ---------------------------- | ------------------------------ |
| `jsonwebtoken`               | JWT decoding (SIA validation)  |
| `reqwest` (json, rustls-tls) | HTTP client for Hub JWKS fetch |
| `redis` (tokio-comp feature) | Redis client                   |
| `tokio-tungstenite`          | WebSocket server               |

### Web Client (npm)

| Package            | Purpose          |
| ------------------ | ---------------- |
| `zustand`          | State management |
| `react-router-dom` | Routing          |

---

## 13. Task Dependency Graph

```
H-1 (Hub DB + Config)
 ├── H-2 (User Registration)
 │    └── H-3 (OIDC Provider) ──────┐
 │         ├── H-5 (SIA Issuance) ──┼── P-2 (SIA Validation) ──┐
 │         ├── H-6 (Pod Registry)   │                           │
 │         ├── H-7 (User Profiles)  │                           │
 │         └── H-8 (Auth Middleware)│                           │
 │                                   │                           │
 └── H-4 (JWKS) ────────────────────┘                           │
                                                                 │
P-1 (Pod DB + Config) ──────────────────────────────────────────┤
                                                                 │
                                                    P-2 (SIA Validation)
                                                     ├── P-3 (Community CRUD) ──┐
                                                     │    ├── P-4 (Channels) ───┤
                                                     │    ├── P-7 (RBAC)        │
                                                     │    └── P-9 (Invites)     │
                                                     │                          │
                                                     ├── P-8 (Gateway) ─────────┤
                                                     │                          │
                                                     └──────────────────── P-5 (Messages)
                                                                            └── P-6 (Reactions)

C-1 (Layout Shell) ───┐
C-2 (OIDC Login) ─────┤
                       ├── C-3 (Pod Connection) ──┐
                       │                          ├── C-4 (Navigation)
                       │                          ├── C-5 (Gateway Client)
                       │                          │    └── C-6 (Messages)
                       └── C-7 (Settings)
```

### Recommended Implementation Order

**Sprint 1 (Weeks 1–3): Foundation**

- H-1, H-4, P-1 (database, config, key setup)
- H-2 (user registration)
- H-8 (auth middleware)
- C-1 (layout shell)
- `voxora-common` (IDs, snowflake, error types)

**Sprint 2 (Weeks 4–6): Auth**

- H-3 (OIDC provider — this is the most complex task)
- H-5 (SIA issuance)
- P-2 (SIA validation + Pod login)
- C-2 (OIDC login flow)
- C-3 (Pod connection)

**Sprint 3 (Weeks 7–10): Core Features**

- H-6 (Pod registry)
- H-7 (User profiles)
- P-3, P-4, P-7 (communities, channels, RBAC)
- P-8 (WebSocket Gateway)
- C-4, C-5 (navigation, gateway client)

**Sprint 4 (Weeks 11–14): Messaging & Polish**

- P-5 (messages)
- P-6 (reactions)
- P-9 (invites)
- C-6 (message sending/receiving)
- C-7 (settings)
- Integration tests
- Bug fixes and polish

**Sprint 5 (Weeks 15–16): Buffer**

- Integration testing across all flows
- Performance testing
- Documentation
- Bug fixes

---

## 14. Out of Scope for Phase 1

These items are explicitly deferred to later phases. Do NOT implement them:

- MFA (TOTP, WebAuthn) — Phase 2
- Voice / video channels — Phase 2
- Threads — Phase 2
- Pins — Phase 2
- File attachments / uploads — Phase 2
- URL embeds / link previews — Phase 2
- Audit log UI — Phase 2 (schema is created, log writes can be added opportunistically)
- Typing indicators — Phase 2
- Presence (online/offline/idle) — Phase 2
- Pod verification flow — Phase 2
- Push notifications — Phase 2
- Desktop client — Phase 2
- Mobile clients — Phase 3
- Billing — Phase 3
- E2EE — Phase 4
- Social login (GitHub/Google/Apple) — Phase 3
- Bot API — Phase 3
- Channel permission overrides (schema exists, enforcement deferred) — Phase 2
- Custom emoji — Phase 4
- Forum / Stage / Announcement channels — Phase 3
- Gateway RESUME / reconnect with replay — Phase 2 (simple reconnect + re-IDENTIFY is sufficient for Phase 1)

---

_End of Phase 1 Implementation Guide_
