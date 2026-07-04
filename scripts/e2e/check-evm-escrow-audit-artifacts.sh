#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

REPORT="${MORPHEUS_EVM_AUDIT_REPORT:-}"
if [[ -z "$REPORT" ]]; then
  echo "MORPHEUS_EVM_AUDIT_REPORT must point to an external audit report before production funds" >&2
  exit 1
fi

if [[ ! -s "$REPORT" ]]; then
  echo "audit report does not exist or is empty: ${REPORT}" >&2
  exit 1
fi

require_text() {
  local pattern="$1"
  local label="$2"
  if ! rg -qi "$pattern" "$REPORT"; then
    echo "audit report missing ${label}: ${REPORT}" >&2
    exit 1
  fi
}

require_text "auditor|reviewer|firm" "auditor identity"
require_text "scope" "scope"
require_text "MorpheusEscrow|escrow" "escrow contract scope"
require_text "commit|revision|bytecode|deployment" "reviewed artifact identity"
require_text "finding|issue|severity" "findings"
require_text "remediation|resolved|accepted risk" "remediation status"

echo "evm escrow audit artifacts ok"
