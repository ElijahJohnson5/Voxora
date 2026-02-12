#!/usr/bin/env bash
set -euo pipefail

# Creates the voxora PostgreSQL role and the hub/pod databases.
# Assumes a local PostgreSQL server is running and the current user
# has superuser (or createdb/createrole) privileges.

PG_USER="voxora"
PG_PASS="voxora"
HUB_DB="hub"
HUB_TEST_DB="hub_test"
POD_DB="pod"
POD_TEST_DB="pod_test"

echo "==> Creating role '${PG_USER}' (if it doesn't exist)..."
psql postgres -tc "SELECT 1 FROM pg_roles WHERE rolname = '${PG_USER}'" | grep -q 1 \
  || psql postgres -c "CREATE ROLE ${PG_USER} WITH LOGIN PASSWORD '${PG_PASS}' CREATEDB;"

for DB in "$HUB_DB" "$HUB_TEST_DB" "$POD_DB" "$POD_TEST_DB"; do
  echo "==> Creating database '${DB}' (if it doesn't exist)..."
  psql postgres -tc "SELECT 1 FROM pg_database WHERE datname = '${DB}'" | grep -q 1 \
    || psql postgres -c "CREATE DATABASE ${DB} OWNER ${PG_USER};"
done

echo "==> Running hub-api migrations on '${HUB_DB}'..."
DATABASE_URL="postgresql://${PG_USER}:${PG_PASS}@localhost:5432/${HUB_DB}" \
  cargo run -p hub-api --bin migrate 2>&1

echo "==> Running hub-api migrations on '${HUB_TEST_DB}'..."
DATABASE_URL="postgresql://${PG_USER}:${PG_PASS}@localhost:5432/${HUB_TEST_DB}" \
  cargo run -p hub-api --bin migrate 2>&1

echo ""
echo "Done! Databases ready:"
echo "  hub       -> postgresql://${PG_USER}:${PG_PASS}@localhost:5432/${HUB_DB}"
echo "  hub_test  -> postgresql://${PG_USER}:${PG_PASS}@localhost:5432/${HUB_TEST_DB}"
echo "  pod       -> postgresql://${PG_USER}:${PG_PASS}@localhost:5432/${POD_DB}"
echo "  pod_test  -> postgresql://${PG_USER}:${PG_PASS}@localhost:5432/${POD_TEST_DB}"
