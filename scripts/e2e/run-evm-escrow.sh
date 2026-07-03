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
require_command mox

RPC_URL="${MORPHEUS_EVM_RPC_URL:-http://127.0.0.1:8545}"
ANVIL_LOG="${TMPDIR:-/tmp}/morpheus-anvil.log"

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
  mox run script/deploy.py --network local
)

test -s contracts/deployments/local.json
cargo test -p morpheus-server evm_escrow

echo "evm escrow e2e ok"
