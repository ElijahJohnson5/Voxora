#!/usr/bin/env bash
set -euo pipefail

# Creates the voxora PostgreSQL role and the hub/pod databases.
# Assumes a local PostgreSQL server is running and the current user
# has superuser (or createdb/createrole) privileges.

PG_HOST="${PGHOST:-localhost}"
PG_PORT="${PGPORT:-5432}"
PG_USER="voxora"
PG_PASS="voxora"
HUB_DB="hub"
HUB_TEST_DB="hub_test"
POD_DB="pod"
POD_TEST_DB="pod_test"

PSQL="psql -h ${PG_HOST} -p ${PG_PORT}"

echo "==> Creating role '${PG_USER}' (if it doesn't exist)..."
${PSQL} postgres -tc "SELECT 1 FROM pg_roles WHERE rolname = '${PG_USER}'" | grep -q 1 \
  || ${PSQL} postgres -c "CREATE ROLE ${PG_USER} WITH LOGIN PASSWORD '${PG_PASS}' CREATEDB;"

for DB in "$HUB_DB" "$HUB_TEST_DB" "$POD_DB" "$POD_TEST_DB"; do
  echo "==> Creating database '${DB}' (if it doesn't exist)..."
  ${PSQL} postgres -tc "SELECT 1 FROM pg_database WHERE datname = '${DB}'" | grep -q 1 \
    || ${PSQL} postgres -c "CREATE DATABASE ${DB} OWNER ${PG_USER};"
done

PG_BASE="postgresql://${PG_USER}:${PG_PASS}@${PG_HOST}:${PG_PORT}"

echo "==> Running hub-api migrations on '${HUB_DB}'..."
DATABASE_URL="${PG_BASE}/${HUB_DB}" \
  cargo run -p hub-api --bin migrate 2>&1

echo "==> Running hub-api migrations on '${HUB_TEST_DB}'..."
DATABASE_URL="${PG_BASE}/${HUB_TEST_DB}" \
  cargo run -p hub-api --bin migrate 2>&1

echo "==> Running pod-api migrations on '${POD_DB}'..."
DATABASE_URL="${PG_BASE}/${POD_DB}" \
  cargo run -p pod-api --bin pod-migrate 2>&1

echo "==> Running pod-api migrations on '${POD_TEST_DB}'..."
DATABASE_URL="${PG_BASE}/${POD_TEST_DB}" \
  cargo run -p pod-api --bin pod-migrate 2>&1

echo ""
echo "Done! Databases ready:"
echo "  hub       -> ${PG_BASE}/${HUB_DB}"
echo "  hub_test  -> ${PG_BASE}/${HUB_TEST_DB}"
echo "  pod       -> ${PG_BASE}/${POD_DB}"
echo "  pod_test  -> ${PG_BASE}/${POD_TEST_DB}"
