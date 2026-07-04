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

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "${name} is required" >&2
    exit 1
  fi
}

require_command cargo
require_command cast
require_command curl
require_command mox

require_env MORPHEUS_TESTNET_RPC_URL
require_env MORPHEUS_TESTNET_CHAIN_ID

EXPECTED_CHAIN_ID="$MORPHEUS_TESTNET_CHAIN_ID"
RPC_URL="$MORPHEUS_TESTNET_RPC_URL"
CONFIRMATIONS="${MORPHEUS_TESTNET_CONFIRMATIONS:-5}"
DEPLOY="${MORPHEUS_EVM_TESTNET_DEPLOY:-0}"
DEPLOYMENT_OUT="${MORPHEUS_EVM_TESTNET_DEPLOYMENT_OUT:-.local/e2e/testnet-evm-escrow.json}"

CHAIN_ID="$(cast chain-id --rpc-url "$RPC_URL")"
if [[ "$CHAIN_ID" != "$EXPECTED_CHAIN_ID" ]]; then
  echo "chain id mismatch: expected ${EXPECTED_CHAIN_ID}, got ${CHAIN_ID}" >&2
  exit 1
fi

HEAD="$(cast block-number --rpc-url "$RPC_URL")"
if [[ "$HEAD" -lt "$CONFIRMATIONS" ]]; then
  echo "head block ${HEAD} is below confirmation depth ${CONFIRMATIONS}" >&2
  exit 1
fi

(
  cd contracts
  mox test -q
)

if [[ "$DEPLOY" == "1" ]]; then
  require_env MORPHEUS_EVM_DEPLOYER
  mkdir -p "$(dirname "$DEPLOYMENT_OUT")"
  (
    cd contracts
    MORPHEUS_EVM_CHAIN_ID="$CHAIN_ID" \
    MORPHEUS_EVM_DEPLOYMENT_OUT="../${DEPLOYMENT_OUT}" \
    mox run script/deploy.py --url "$RPC_URL" --private-key "$MORPHEUS_EVM_DEPLOYER"
  )
  test -s "$DEPLOYMENT_OUT"
  echo "testnet deployment written to ${DEPLOYMENT_OUT}"
else
  echo "testnet deploy skipped; set MORPHEUS_EVM_TESTNET_DEPLOY=1 to deploy"
fi

if [[ -n "${MORPHEUS_TESTNET_MORPHEUS_URL:-}" ]]; then
  require_env MORPHEUS_ADMIN_TOKEN
  curl -fsS \
    -H "Authorization: Bearer ${MORPHEUS_ADMIN_TOKEN}" \
    "${MORPHEUS_TESTNET_MORPHEUS_URL%/}/admin/evm-escrow/status" >/dev/null
  echo "morpheus testnet watcher status ok"
else
  echo "morpheus status check skipped; set MORPHEUS_TESTNET_MORPHEUS_URL and MORPHEUS_ADMIN_TOKEN"
fi

echo "evm escrow testnet drill ok"
