#!/usr/bin/env bash
set -euo pipefail

# Colors for each service
RED='\033[0;31m'    # hub-api
GREEN='\033[0;32m'  # pod-api
BLUE='\033[0;34m'   # web
NC='\033[0m'        # reset

cleanup() {
    trap - SIGINT SIGTERM EXIT
    echo ""
    echo "Stopping all services..."
    kill 0
}
trap cleanup SIGINT SIGTERM EXIT

# Hub API (port 4001)
(cargo run -p hub-api 2>&1 | while IFS= read -r line; do
    printf "${RED}[hub]${NC} %s\n" "$line"
done) &

# Pod API (port 4002)
(cargo run -p pod-api 2>&1 | while IFS= read -r line; do
    printf "${GREEN}[pod]${NC} %s\n" "$line"
done) &

# Web Client (port 4200)
(pnpm nx serve web-client 2>&1 | while IFS= read -r line; do
    printf "${BLUE}[web]${NC} %s\n" "$line"
done) &

echo "Services starting..."
echo "  Hub API:    http://localhost:4001"
echo "  Pod API:    http://localhost:4002"
echo "  Web Client: http://localhost:4200"
echo ""
echo "Press Ctrl+C to stop all services."

wait
