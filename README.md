# Voxora

A federated real-time communication platform. The Hub manages identity and pod discovery, Pods host independent communities with channels and messaging, and the Web Client provides the user interface.

## Architecture

| Service        | Path              | Port | Description                                  |
| -------------- | ----------------- | ---- | -------------------------------------------- |
| **Hub API**    | `apps/hub-api`    | 4001 | Identity provider, pod registry, OIDC issuer |
| **Pod API**    | `apps/pod-api`    | 4002 | Community server (messages, channels, voice) |
| **Web Client** | `apps/web-client` | 4200 | React SPA                                    |

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (v20+)
- [pnpm](https://pnpm.io/) (v10+)
- [Docker](https://www.docker.com/) & Docker Compose

## Getting Started

### 1. Start infrastructure

Spin up PostgreSQL, Redis, and the TURN server:

```bash
docker compose up -d
```

This creates four databases (`hub`, `hub_test`, `pod`, `pod_test`) with user `voxora` / password `voxora`.

### 2. Copy environment files

```bash
cp apps/hub-api/.example.env apps/hub-api/.env
cp apps/pod-api/.example.env apps/pod-api/.env
cp apps/web-client/.example.env apps/web-client/.env
```

The Hub API defaults work out of the box with Docker Compose. The Pod API requires credentials — see step 4.

### 3. Run database migrations if not using docker compose

```bash
# Hub
DATABASE_URL=postgresql://voxora:voxora@localhost:5432/hub cargo run -p hub-api --bin migrate

# Pod
DATABASE_URL=postgresql://voxora:voxora@localhost:5432/pod cargo run -p pod-api --bin pod-migrate
```

Or run all migrations at once (also sets up test databases):

```bash
./scripts/setup-db.sh
```

### 4. Register a Pod with the Hub

The Pod API needs credentials from the Hub. Run the interactive setup wizard:

```bash
cargo run -p pod-api --bin pod-setup
```

This will prompt you for the Hub URL and a pod name, then generate `POD_ID`, `POD_CLIENT_ID`, and `POD_CLIENT_SECRET` values and write them to `apps/pod-api/.env`.

### 5. Install frontend dependencies

```bash
pnpm install
```

### 6. Start all services

```bash
./scripts/dev.sh
```

This runs the Hub API, Pod API, and Web Client in parallel with color-coded output:

- **Hub API** — http://localhost:4001
- **Pod API** — http://localhost:4002
- **Web Client** — http://localhost:4200

Press `Ctrl+C` to stop all services.

### Running services individually

```bash
# Hub API
cargo run -p hub-api

# Pod API
cargo run -p pod-api

# Web Client
pnpm nx serve web-client
```

## Project Structure

```
voxora/
├── apps/
│   ├── hub-api/          # Rust/Axum — Hub identity & registry API
│   ├── pod-api/          # Rust/Axum — Pod community API + WebSocket gateway
│   └── web-client/       # React/Vite — Web frontend
├── libs/
│   └── common/           # Shared Rust library (ID generation, etc.)
├── scripts/
│   ├── dev.sh            # Start all services in parallel
│   ├── setup-db.sh       # Create databases & run all migrations
│   └── init-db.sql       # Docker entrypoint for DB creation
├── docs/                 # RFCs and implementation plans
├── docker-compose.yml    # Local infrastructure (Postgres, Redis, TURN)
├── Cargo.toml            # Rust workspace root
└── package.json          # Node workspace root (Nx)
```

## Infrastructure

| Service       | Image                | Port(s)          |
| ------------- | -------------------- | ---------------- |
| PostgreSQL 16 | `postgres:16-alpine` | 5432             |
| Redis 7       | `redis:7-alpine`     | 6379             |
| TURN (coturn) | `coturn/coturn`      | 3478 (UDP + TCP) |

## OpenAPI Docs

Both APIs can generate OpenAPI specs:

```bash
cargo run -p hub-api --bin generate-openapi
cargo run -p pod-api --bin generate-openapi
```
