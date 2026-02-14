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
│   │   └── pool.rs          # Diesel async pool setup
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
│   ├── 20260210120000_create_users/
│   │   ├── up.sql
│   │   └── down.sql
│   ├── 20260210120001_create_sessions/
│   │   ├── up.sql
│   │   └── down.sql
│   └── 20260210120002_create_pods/
│       ├── up.sql
│       └── down.sql
└── project.json
```

### WS-1.2 Tasks (ordered by dependency)

#### Task H-1: Configuration & Database Setup

**Priority: P0 — must be done first**

- Add dependencies to `Cargo.toml`: `diesel` (postgres), `diesel-async` (deadpool), `diesel_migrations`, `dotenvy`, `config` or manual env parsing
- Create `config.rs`: reads from env vars (`DATABASE_URL`, `REDIS_URL`, `HUB_DOMAIN`, `SIGNING_KEY_SEED`, `PORT`)
- Create `db/pool.rs`: initialize Diesel async pool
- Add a standalone migration runner (e.g. `cargo run -p hub-api --bin migrate`) and do not auto-run migrations on startup
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
│   ├── main.tsx                    # Entry point, renders RouterProvider
│   ├── styles.css                  # Tailwind directives + global styles
│   ├── routeTree.gen.ts            # Auto-generated route tree (TanStack Router)
│   ├── routes/
│   │   ├── __root.tsx              # Root layout (providers, Toaster, etc.)
│   │   ├── login.tsx               # /login — OIDC login initiation
│   │   ├── signup.tsx              # /signup — Registration
│   │   ├── callback.tsx            # /callback — OIDC redirect handler
│   │   ├── _authenticated.tsx      # Layout route — auth guard + three-panel shell
│   │   ├── _authenticated/
│   │   │   ├── index.tsx           # / — Pod Browser (home page)
│   │   │   ├── settings.tsx        # /settings — User settings
│   │   │   └── pod/
│   │   │       ├── $podId.tsx                          # /pod/$podId — Pod layout
│   │   │       └── $podId/
│   │   │           └── community/
│   │   │               ├── $communityId.tsx            # /pod/$podId/community/$communityId layout
│   │   │               └── $communityId/
│   │   │                   └── channel/
│   │   │                       └── $channelId.tsx      # /pod/$podId/community/$communityId/channel/$channelId
│   ├── components/
│   │   ├── ui/                     # shadcn/ui primitives (auto-generated)
│   │   │   ├── button.tsx
│   │   │   ├── input.tsx
│   │   │   ├── dialog.tsx
│   │   │   ├── dropdown-menu.tsx
│   │   │   ├── select.tsx
│   │   │   ├── label.tsx
│   │   │   ├── avatar.tsx
│   │   │   ├── scroll-area.tsx
│   │   │   ├── separator.tsx
│   │   │   ├── tooltip.tsx
│   │   │   ├── popover.tsx
│   │   │   ├── skeleton.tsx
│   │   │   ├── badge.tsx
│   │   │   ├── textarea.tsx
│   │   │   ├── editor.tsx          # Plate editor primitives
│   │   │   └── sonner.tsx          # Toast notifications
│   │   ├── layout/
│   │   │   ├── sidebar.tsx         # Community icon strip + channel list (multi-pod aware)
│   │   │   ├── chat-area.tsx       # Message list + input
│   │   │   ├── member-list.tsx     # Right sidebar
│   │   │   └── header.tsx          # Channel header bar
│   │   ├── communities/
│   │   │   └── community-dialogs.tsx  # Create Community + Join Invite dialogs (with pod selector)
│   │   ├── messages/
│   │   │   ├── channel-context.tsx # ChannelProvider + useChannel() — provides podId + channelId
│   │   │   ├── message-item.tsx    # Single message component (uses useChannel)
│   │   │   ├── message-list.tsx    # Virtualized scrollable message list (uses useChannel)
│   │   │   ├── message-input.tsx   # Plate rich-text editor with send (uses useChannel)
│   │   │   ├── reactions.tsx       # Reaction badges (uses useChannel)
│   │   │   └── rich-text-content.tsx # Rich text renderer for message content
│   │   ├── editor/                 # Plate editor kits
│   │   │   ├── message-kit.tsx     # Minimal Plate config for message input
│   │   │   └── ...                 # Other editor plugin kits
│   │   └── settings/
│   │       └── user-settings.tsx
│   ├── stores/
│   │   ├── auth.ts                 # OIDC tokens, SIA, Hub auth state
│   │   ├── pod.ts                  # Multi-pod connections (PAT, WS ticket, gateway per pod)
│   │   ├── communities.ts          # Community + channel state (keyed by podId)
│   │   └── messages.ts             # Message cache per channel (keyed by podId:channelId)
│   ├── lib/
│   │   ├── api/
│   │   │   ├── hub-client.ts       # Hub API client (openapi-fetch, typed from hub.d.ts)
│   │   │   ├── hub.d.ts            # Auto-generated Hub OpenAPI types
│   │   │   ├── pod-client.ts       # Pod API client factory (one per pod)
│   │   │   └── pod.d.ts            # Auto-generated Pod OpenAPI types
│   │   ├── gateway/
│   │   │   ├── connection.ts       # GatewayConnection class (one instance per pod)
│   │   │   ├── handler.ts          # Dispatch event handler (READY, MESSAGE_*, etc.)
│   │   │   └── useGatewayStatus.ts # Hook for connection status display
│   │   ├── oidc.ts                 # PKCE helpers, auth flow
│   │   └── utils.ts                # cn() helper, misc utilities
├── components.json                 # shadcn/ui configuration
├── index.html
├── vite.config.mts
└── package.json
```

### WS-3.2 Additional Dependencies

Install in `apps/web-client`:

```bash
# Core
pnpm add zustand @tanstack/react-router
pnpm add -D @tanstack/router-plugin @tanstack/router-devtools

# Styling
pnpm add tailwindcss @tailwindcss/vite
pnpm add class-variance-authority clsx tailwind-merge lucide-react
pnpm add sonner                       # Toast notifications (used by shadcn Sonner)

# Rich text editor
pnpm add platejs platejs-react        # Plate editor framework

# Virtualized scrolling
pnpm add virtua                       # Lightweight virtualizer for message list

# API client
pnpm add openapi-fetch                # Type-safe API client generated from OpenAPI specs

# shadcn/ui — initialize, then add components as needed
pnpm dlx shadcn@latest init
pnpm dlx shadcn@latest add button input dialog dropdown-menu avatar \
  scroll-area separator tooltip popover skeleton badge textarea sonner \
  select label alert-dialog
```

### WS-3.3 Tailwind & shadcn Setup Notes

**`tailwind.config.ts`** — Use the shadcn/ui preset which defines CSS custom-property-based theme tokens (`--background`, `--foreground`, `--primary`, etc.) for light/dark mode support.

**`src/styles.css`** — Tailwind directives:

```css
@import "tailwindcss";
```

**`src/lib/utils.ts`** — Standard `cn()` helper used by all shadcn components:

```ts
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
```

**`vite.config.mts`** — Add the TanStack Router plugin for file-based route generation:

```ts
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [
    tanstackRouter({ target: "react", autoCodeSplitting: true }),
    tailwindcss(),
    react(),
  ],
  // ...
});
```

The plugin auto-generates `routeTree.gen.ts` from the `src/routes/` directory on save.

### WS-3.4 Tasks (ordered by dependency)

#### Task C-1: Routing, Tailwind & Layout Shell

**Priority: P0** | **Status: Done**

- Install TanStack Router, Tailwind CSS, and shadcn/ui (see WS-3.2)
- Configure Vite with `@tanstack/router-plugin/vite` and `@tailwindcss/vite`
- Initialize shadcn (`pnpm dlx shadcn@latest init`) — choose "New York" style, slate base color, CSS variables enabled
- Add initial shadcn components: `button`, `input`, `scroll-area`, `separator`, `avatar`, `tooltip`, `skeleton`, `select`, `label`, `dialog`, `badge`
- Create the file-based route tree:
  - `__root.tsx` — wraps the app in providers (Zustand context if needed), renders `<Outlet />` and `<Toaster />`
  - `login.tsx` — public login page
  - `signup.tsx` — public registration page
  - `callback.tsx` — public OIDC callback
  - `_authenticated.tsx` — layout route that checks auth state, redirects to `/login` if unauthenticated, renders three-panel shell (sidebar + `<Outlet />` + member list + header)
  - `_authenticated/index.tsx` — Pod Browser home page (connect/disconnect pods, discover pods)
  - `_authenticated/pod/$podId.tsx` — pod-scoped layout (wraps community routes)
  - `_authenticated/pod/$podId/community/$communityId.tsx` — community layout
  - `_authenticated/pod/$podId/community/$communityId/channel/$channelId.tsx` — channel chat view (wraps messages in `ChannelProvider`)
  - `_authenticated/settings.tsx` — user settings
- Create the three-panel layout shell using Tailwind utility classes:
  - Left sidebar — two-column: community icon strip (w-16, grouped by pod) + channel list (w-44)
  - Center chat area (flex-1) — header + messages + input
  - Right member list (w-60, collapsible) — member avatars + names
- Use shadcn `ScrollArea` for all scrollable panels
- Use shadcn `Separator`, `Avatar`, `Tooltip` throughout the layout

**TanStack Router auth guard pattern:**

```tsx
// src/routes/_authenticated.tsx
import { createFileRoute, redirect, Outlet } from "@tanstack/react-router";
import { useAuthStore } from "../stores/auth";

export const Route = createFileRoute("/_authenticated")({
  beforeLoad: () => {
    const { accessToken } = useAuthStore.getState();
    if (!accessToken) {
      throw redirect({ to: "/login" });
    }
  },
  component: AuthenticatedLayout, // renders Sidebar + Header + Outlet + MemberList
});
```

**Typesafe navigation examples:**

```tsx
import { Link, useNavigate, useParams } from "@tanstack/react-router";

// Typesafe Link — all routes are pod-scoped
<Link
  to="/pod/$podId/community/$communityId/channel/$channelId"
  params={{ podId: "pod_01J9...", communityId: "com_01KP...", channelId: "ch_01KP..." }}
  className={cn("text-sm", isActive && "text-foreground font-semibold")}
>
  #general
</Link>;

// Typesafe params — inferred from the route definition
const { podId, communityId, channelId } = useParams({
  from: "/_authenticated/pod/$podId/community/$communityId/channel/$channelId",
});
```

#### Task C-2: OIDC Login Flow

**Priority: P0** | **Status: Done**
**Depends on: Hub H-3**

Implement the full PKCE flow:

1. **Login page** (`src/routes/login.tsx`):
   - Centered card layout using shadcn `Button` for the "Login with Voxora" CTA
   - Styled with Tailwind: `flex items-center justify-center min-h-screen bg-background`
2. **On click**:
   - Generate `code_verifier` (random 64-byte base64url string)
   - Compute `code_challenge = base64url(SHA-256(code_verifier))`
   - Generate `state` (random 32-byte string)
   - Store `code_verifier` and `state` in `sessionStorage`
   - Redirect to Hub: `{HUB_URL}/oidc/authorize?response_type=code&client_id=voxora-web&redirect_uri={CALLBACK_URL}&scope=openid+profile+email+pods+offline_access&state={state}&code_challenge={challenge}&code_challenge_method=S256`
3. **Callback route** (`src/routes/callback.tsx`):
   - Use TanStack Router's `searchParams` validation to parse `code` and `state` from the URL:
     ```tsx
     export const Route = createFileRoute("/callback")({
       validateSearch: (search: Record<string, unknown>) => ({
         code: search.code as string,
         state: search.state as string,
       }),
       component: CallbackPage,
     });
     ```
   - Verify `state` matches sessionStorage
   - Exchange code for tokens: `POST {HUB_URL}/oidc/token` with `{ grant_type: "authorization_code", code, redirect_uri, code_verifier, client_id: "voxora-web" }`
   - Store `access_token`, `refresh_token`, `id_token` in Zustand auth store (persisted to `localStorage`)
   - Navigate to `/` via `useNavigate()` (typesafe)
   - Show a loading skeleton (shadcn `Skeleton`) during the token exchange

4. **Token refresh**: Background timer that refreshes access token 1 minute before expiry using refresh_token grant

5. **Logout**: Clear all tokens, navigate to `/login`

#### Task C-3: Pod Connection Flow (Multi-Pod)

**Priority: P0** | **Status: Done**
**Depends on: C-2, Pod P-2**

The client supports connecting to **multiple pods simultaneously**. Each pod has its own PAT, refresh token, WS ticket, and Gateway connection.

**Pod store (`stores/pod.ts`):**
- `pods: Record<string, PodConnectionData>` — keyed by `podId`
- Each entry: `{ podId, podUrl, podName, podIcon, pat, refreshToken, wsTicket, wsUrl, user, connected, connecting, error }`
- One `GatewayConnection` instance per pod (stored outside Zustand in a `Map<string, GatewayConnection>`)

**Connection flow (per pod):**
1. Request SIA from Hub: `POST {HUB_URL}/api/v1/oidc/sia` with `{ pod_id }` using Hub access token
2. Login to Pod: `POST {POD_URL}/api/v1/auth/login` with `{ sia }`
3. Store PAT, refresh_token, ws_ticket, ws_url in pod store entry
4. Create and connect Gateway WebSocket using ws_ticket (see C-5)
5. On READY: populate community/channel stores keyed by `podId`
6. Schedule PAT refresh before expiry
7. On disconnect: clean up gateway, mark pod as disconnected

**Pod Browser (`_authenticated/index.tsx`):**
- Home page showing "My Pods" (from Hub `GET /api/v1/users/@me/pods`) merged with locally connected pods
- "Discover Pods" grid (from Hub `GET /api/v1/pods?sort=popular`)
- Search filter for discover section
- Connect/Disconnect/Open buttons per pod
- "Open" navigates to first community's default channel: `/pod/$podId/community/$communityId/channel/$channelId`

#### Task C-4: Community & Channel Navigation (Multi-Pod)

**Priority: P1** | **Status: Done**
**Depends on: C-3**

- **Sidebar (left)** — two-column layout:
  - **Column 1 (w-16, icon strip)**: communities grouped by pod, each pod section has a header label + community `Avatar` icons + `Tooltip`. Below all pods: Create Community (+) and Join Invite buttons, and user avatar linking to Settings.
  - **Column 2 (w-44, channel list)**: only visible when a community is active. Shows community name + list of channels as `Button` variant `ghost`.
  - Click community → navigates to `/pod/$podId/community/$communityId/channel/$channelId` (default channel)
  - Click channel → typesafe navigation with `podId` in the URL:
    ```tsx
    navigate({
      to: "/pod/$podId/community/$communityId/channel/$channelId",
      params: { podId, communityId, channelId: channel.id },
    });
    ```
  - Active channel gets `bg-accent` styling
  - Active community gets `ring-2 ring-primary` on avatar
  - All route params read from URL via `useMatch` (no prop drilling through layout)

- **Header** — reads `podId`, `communityId`, `channelId` from `useMatch`, shows channel name (bold) + topic (muted) + gateway connection status badge

- **Member list** — reads route params from `useMatch`, fetches members keyed by `[podId][communityId]`

- **Create Community / Join Invite dialogs** — when multiple pods are connected, show a `Select` dropdown to pick the target pod. When only one pod is connected, the selector is hidden. Callbacks receive `podId` from the dialog.

- **Home button** in icon strip navigates to `/` (Pod Browser)

#### Task C-5: WebSocket Gateway Client (Multi-Pod)

**Priority: P0** | **Status: Done**
**Depends on: C-3**

Implement a `GatewayConnection` class (`lib/gateway/connection.ts`). **One instance per connected pod**, stored in a `Map<string, GatewayConnection>` outside Zustand (not serializable).

Each `GatewayConnection` instance:

1. Connect to `ws_url` from pod login response
2. Send IDENTIFY with `ws_ticket`
3. On READY: populate community/channel/member stores **keyed by `podId`** (e.g., `communities[podId][communityId]`, `channels[podId][communityId][]`)
4. Start heartbeat interval from READY's `heartbeat_interval`
5. On DISPATCH events: update Zustand stores with `podId` context
   - `MESSAGE_CREATE` → append to message store (key: `podId:channelId`)
   - `MESSAGE_UPDATE` → update in message store
   - `MESSAGE_DELETE` → remove from message store
   - `MESSAGE_REACTION_ADD/REMOVE` → update reactions on message
   - `CHANNEL_CREATE/UPDATE/DELETE` → update channel store under `[podId][communityId]`
   - `COMMUNITY_UPDATE` → update community store under `[podId]`
   - `MEMBER_JOIN/LEAVE/UPDATE` → update member store under `[podId][communityId]`
6. On close: attempt reconnect with exponential backoff (1s, 2s, 4s, 8s, max 30s, max 5 retries)
7. Connection status: `useGatewayStatus(podId)` hook reads `connecting`/`connected`/`error` from pod store, displayed as a `Badge` in the header

#### Task C-6: Message Sending & Receiving

**Priority: P1** | **Status: Done**
**Depends on: C-5, Pod P-5**

**ChannelContext** (`components/messages/channel-context.tsx`): The channel view wraps all message components in a `<ChannelProvider podId={podId} channelId={channelId}>`. Child components call `useChannel()` to get `{ podId, channelId }` without prop drilling. This eliminates `podId`/`channelId` props from `MessageList`, `MessageInput`, `MessageItem`, and `MessageReactions`.

- **Message list** (`message-list.tsx`) — uses `virtua` `Virtualizer` for virtualized scrolling:
  - Load initial history via REST: `GET /channels/{id}/messages?limit=50` (deferred in route loader)
  - New messages arrive via Gateway `MESSAGE_CREATE`
  - Scroll to bottom on new message (if already at bottom)
  - Infinite scroll up/down: fetch older/newer messages with `?before=<oldest_id>` / `?after=<newest_id>`
  - Compact messages (same author within 5 min): timestamp on hover, no avatar
  - Loading state: skeleton placeholder based on channel `message_count`

- **Message item** (`message-item.tsx`) — `memo`-wrapped:
  - Each message: `Avatar` (left), username (`text-sm font-semibold`), timestamp (`text-xs text-muted-foreground`), content, edited indicator
  - Content rendered via `RichTextContent` (parses JSON for rich text, plain text for simple strings)
  - Action buttons on hover (CSS-only): react, edit (own), delete (own)
  - Inline edit: swaps content with a Plate editor, Enter to save, Esc to cancel
  - Member lookup: resolves `author_id` → display name/avatar via community members store

- **Message input** (`message-input.tsx`) — **Plate rich-text editor** (not plain textarea):
  - Uses `MessageKit` plugin set (basic marks only for messages)
  - Send on Enter (Shift+Enter for newline)
  - Serialization: single plain-text paragraphs stored as plain text, rich content stored as JSON
  - `POST /channels/{id}/messages` with `{ content, nonce: uuid() }`
  - Optimistic insert: show message immediately with `opacity-50` pending state, reconcile with `MESSAGE_CREATE` from Gateway using `nonce`

- **Reactions** (`reactions.tsx`): `Badge` components below message, toggle on click (`PUT`/`DELETE .../reactions/{emoji}`). Full emoji picker is TODO.

#### Task C-7: Basic Settings

**Priority: P2** | **Status: Done**
**Depends on: C-2**

- Settings page at `/settings` (file: `src/routes/_authenticated/settings.tsx`)
- Form built with shadcn `Input`, `Button`, `Avatar`
- Display current user info (from Hub)
- Allow editing display_name via `PATCH {HUB_URL}/api/v1/users/@me`
- Success/error feedback via `sonner` toast
- Logout button — shadcn `Button` variant `destructive`

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

Run against `hub` PostgreSQL database. Use Diesel migrations (via `diesel` CLI or the `migrate` binary).

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

# Run migrations (Hub uses Diesel migrations)
cargo run -p hub-api --bin migrate
# When Pod migrations are implemented with Diesel, run from its crate root:
# cd apps/pod-api && diesel migration run

# Start services (in separate terminals or via Nx)
pnpm nx serve hub-api
pnpm nx serve pod-api
pnpm nx serve web-client
```

---

## 12. Dependency Reference

### Rust Crates (Hub + Pod shared)

| Crate                            | Purpose            |
| -------------------------------- | ------------------ |
| `axum` 0.7                       | HTTP framework     |
| `tokio` 1.x                      | Async runtime      |
| `serde` / `serde_json`           | Serialization      |
| `diesel` 2.x (postgres)          | Database (sync)    |
| `diesel-async` 0.5 (deadpool)    | Database (async)   |
| `diesel_migrations` 2.x          | Migrations         |
| `tracing` / `tracing-subscriber` | Structured logging |
| `dotenvy`                        | .env file loading  |
| `ulid`                           | ULID generation    |
| `tower-http` (cors, trace)       | HTTP middleware    |
| `chrono` (serde feature)         | Timestamps         |

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

| Package                     | Purpose                                    |
| --------------------------- | ------------------------------------------ |
| `zustand`                   | State management (persisted to localStorage) |
| `@tanstack/react-router`    | Typesafe file-based routing                |
| `@tanstack/router-plugin`   | Vite plugin for route tree code generation |
| `@tanstack/router-devtools` | Router devtools (dev only)                 |
| `tailwindcss`               | Utility-first CSS framework                |
| `@tailwindcss/vite`         | Tailwind CSS Vite plugin                   |
| `class-variance-authority`  | Component variant helper (used by shadcn)  |
| `clsx` + `tailwind-merge`   | Class name merging (`cn()` utility)        |
| `lucide-react`              | Icon library (used by shadcn components)   |
| `sonner`                    | Toast notification library                 |
| `platejs` + `platejs-react` | Rich text editor framework (message input) |
| `virtua`                    | Lightweight virtualizer (message list)     |
| `openapi-fetch`             | Type-safe API client from OpenAPI specs    |
| shadcn/ui components        | Pre-built accessible UI primitives         |

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
                       ├── C-3 (Multi-Pod Connection + Pod Browser) ──┐
                       │                                              ├── C-4 (Navigation, multi-pod sidebar)
                       │                                              ├── C-5 (Gateway Client, per-pod WS)
                       │                                              │    └── C-6 (Messages + ChannelContext)
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

- Hub Notification Relay (cross-pod unread badges) — Phase 2 (see §14.1)
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
- Push notifications (Web Push / Service Worker) — Phase 2
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

### 14.1 Tiered Notification System (Phase 2 Design)

**Problem:** In Phase 1, the client opens one WebSocket per connected pod. This works for 1–5 pods but doesn't scale — browsers cap concurrent WebSockets, and each connection carries a heartbeat timer and incoming dispatch traffic even when the user isn't looking at that pod.

**Phase 1 mitigation:** Cap active pod WebSocket connections at 10. The current auto-reconnect-all-on-load behavior is acceptable for early usage.

**Phase 2 solution — Tiered notifications with preferred pods:**

Rather than routing all notifications through the Hub (which creates cost for the Hub operator), Phase 2 uses a tiered approach where users get real-time notifications through multiple mechanisms, and the Hub relay is reserved for paid/managed pods.

#### 14.1.1 Preferred Pods (Free Real-Time)

Users can mark up to **10 pods as "preferred"** in their settings. The client maintains **open WebSocket connections** to all preferred pods at all times, giving full real-time delivery without any relay or paid tier.

This is the primary free-tier mechanism. Users pin the pods they care about most, and those stay fully live. Stored on the Hub via `PATCH /api/v1/users/@me/preferences`.

```
Client connection model:
- 1 WS per preferred pod (up to 10) — full real-time dispatch
- 1 WS to Hub (if relay is available for any non-preferred pod)
- Non-preferred pods without relay: long-poll
```

#### 14.1.2 Notification Tiers (Non-Preferred Pods)

For pods NOT in the user's preferred list, the client negotiates the best available method:

| Priority | Method | When available | Cost to Hub |
| -------- | ------ | -------------- | ----------- |
| 1 | Hub notification relay | Pod is managed, pod has paid plan, or user has paid plan | Paid/included |
| 2 | Client long-polling | Always (fallback) | Zero |

**Hub notification relay (paid/managed):**

The Hub acts as a dumb notification router. It never sees message content — only delivery signals. Pods control all content; the Hub controls the delivery graph.

```
Pod → Hub:    "users [usr_A, usr_B] have new activity in com_xyz"
Hub → Client: notification envelope (pod_id, community_id, channel_id, unread_count, has_mention)
Client:       bumps badge counts in sidebar, fetches actual messages from Pod when user navigates
```

Relay is enabled when ANY of:
- The pod is **managed** (Hub-hosted — included in hosting)
- The pod operator has a **paid pod subscription** that includes relay
- The user has a **paid user subscription** that includes relay

Flow:

1. User connects to Hub WS on login (new Hub Gateway endpoint)
2. Pod receives a new message in channel X
3. Pod resolves which community members need notification (members not connected to the Pod's own Gateway AND not on a preferred-pod WS)
4. Pod pushes to Hub: `POST /api/v1/notifications/push` (authenticated via Pod `client_id`/`client_secret`)
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
5. Hub looks up which users have an active Hub WS and forwards the envelope
6. Client increments unread badge on that pod/community/channel in the sidebar
7. When user navigates to that pod, client opens the Pod WS and fetches messages normally

**What the Hub never sees:** message content, author identity, timestamps, attachments — just "user X has N new in channel Y on pod Z."

**Long polling (free fallback):**

If Hub relay is not available, the client polls each non-preferred pod's `GET /api/v1/unread-counts` endpoint on a 30–60 second interval. Functional but not instant.

#### 14.1.3 Client Negotiation

On login, the Hub returns notification capability per pod:

```json
{
  "pods": [
    { "pod_id": "pod_01...", "name": "My Gaming Pod", "preferred": true },
    { "pod_id": "pod_02...", "name": "Work Team", "relay": true },
    { "pod_id": "pod_03...", "name": "Hobby Club", "relay": false }
  ],
  "user_tier": "free"
}
```

Client logic (priority order):
1. **Preferred pod** → maintain direct WebSocket (full real-time)
2. **`relay: true`** → subscribe via Hub WebSocket (real-time badges)
3. **Fallback** → long-poll the pod's unread endpoint (delayed badges)

#### 14.1.4 Desktop (Electron) Specifics

Electron apps run in the background (system tray), so all connection methods keep working when the window is minimized. OS-native notifications are triggered via `electron.Notification` regardless of which delivery method brought the event.

#### 14.1.5 Mobile Push (Phase 3)

Mobile requires APNs (iOS) / FCM (Android) since the OS kills background connections. The same tiered model applies:

All mobile push flows through the Hub. APNs/FCM credentials are app-scoped — only the app publisher (Voxora) holds the keys needed to push to the Voxora app. Pods cannot push directly to devices, which also prevents abuse (a malicious pod could spam notifications if it had direct push access).

| Tier | Mobile push | Rate limit |
| ---- | ----------- | ---------- |
| Managed pod | Full push via Hub (all activity) | None |
| Paid pod plan | Full push via Hub (all activity) | None |
| Free pod | Mentions-only push via Hub | 100/user/day per pod |
| No setup | No mobile push, fetch on app open | N/A |

The free tier "mentions-only" push keeps Hub cost negligible (each push is a single small HTTP POST to APNs/FCM) while ensuring users on free self-hosted pods still get notified for things that matter most. Rate limiting per-pod prevents abuse.

Mobile clients register their device push token with the Hub (not with individual pods). The Hub maintains the user → device token mapping and dispatches when pods report activity via `POST /api/v1/notifications/push`. The push payload is minimal — just enough for the client to display "New mention in #channel on Pod Name" and navigate on tap.

#### 14.1.6 Why This Design

- **No user is stuck without notifications** — preferred pods give free real-time for the pods that matter most
- **Hub relay cost scales with revenue** — only relay for paying customers
- **Preserves the trust boundary** — Hub handles identity + routing, pods handle content
- **Natural upsell** — Hub relay is the zero-config premium option, long polling is the free fallback

**New components required:**
- Hub: `PATCH /api/v1/users/@me/preferences` for preferred pods
- Hub: client-facing Gateway endpoint (`/gateway`) with IDENTIFY + notification dispatch
- Hub: `POST /api/v1/notifications/push` endpoint for pods (gated by pod/user tier)
- Hub: in-memory routing table (user_id → Hub WS connection)
- Pod: `GET /api/v1/unread-counts` endpoint for long-poll fallback
- Pod: notification push logic after MESSAGE_CREATE (for users not on Pod Gateway and not on preferred-pod WS)
- Client: preferred pod connection manager (up to 10 direct WS)
- Client: Hub WS connection manager (for relay-enabled pods)
- Client: long-poll fallback for remaining pods
- Client: unified unread count store across all notification methods

---

_End of Phase 1 Implementation Guide_
