#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "${name} is required" >&2
    exit 1
  fi
}

require_command anvil
require_command cargo
require_command cast
require_command curl
require_command docker
require_command mox
require_command python3

RPC_URL="http://127.0.0.1:8545"
ANVIL_LOG="${TMPDIR:-/tmp}/morpheus-anvil.log"
DATABASE_URL="postgres://morpheus:morpheus@localhost:5432/morpheus"
ADMIN_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"

anvil --chain-id 31337 >"$ANVIL_LOG" 2>&1 &
ANVIL_PID=$!
trap 'kill "$ANVIL_PID" >/dev/null 2>&1 || true' EXIT

wait_anvil() {
  for _ in $(seq 1 30); do
    if cast block-number --rpc-url "$RPC_URL" >/dev/null 2>&1; then
      echo "anvil ready at ${RPC_URL}"
      return 0
    fi
    sleep 1
  done
  echo "anvil did not become ready at ${RPC_URL}" >&2
  tail -40 "$ANVIL_LOG" >&2 || true
  return 1
}

wait_anvil

(
  cd contracts
  mox test -q
  MORPHEUS_EVM_DEPLOYER="$ADMIN_KEY" mox run script/deploy.py --network local
)

test -s contracts/deployments/local.json
docker compose up -d postgres

POSTGRES_READY=0
for _ in $(seq 1 30); do
  if docker compose exec -T postgres pg_isready -U morpheus -d morpheus >/dev/null 2>&1; then
    POSTGRES_READY=1
    break
  fi
  sleep 1
done
if [[ "$POSTGRES_READY" != "1" ]]; then
  echo "postgres did not become ready" >&2
  exit 1
fi

cargo run -p morpheus-cli -- db migrate --database-url "$DATABASE_URL" --database-kind postgres
MORPHEUS_ADMIN_TOKEN="admin-token" \
MORPHEUS_SELLER_TOKEN="seller-token" \
MORPHEUS_BUYER_TOKEN="buyer-token" \
MORPHEUS_EVM_RPC_URL="$RPC_URL" \
python3 scripts/e2e/evm-escrow-flow.py

echo "evm escrow e2e ok"
