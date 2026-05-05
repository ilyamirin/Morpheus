#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

COMPOSE=(docker compose -f docker-compose.e2e.yml)

scripts/e2e/bootstrap-synapse.sh
"${COMPOSE[@]}" up -d --build --force-recreate

wait_http() {
  local url="$1"
  local name="$2"
  for _ in $(seq 1 90); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "${name} ready"
      return 0
    fi
    sleep 2
  done
  echo "${name} did not become ready at ${url}" >&2
  "${COMPOSE[@]}" ps >&2
  return 1
}

wait_http "http://127.0.0.1:18081/readyz" "morpheus-books"
wait_http "http://127.0.0.1:18082/readyz" "morpheus-cases"
wait_http "http://127.0.0.1:18083/readyz" "morpheus-fashion"
wait_http "http://127.0.0.1:18008/_matrix/client/versions" "synapse-books"
wait_http "http://127.0.0.1:18009/_matrix/client/versions" "synapse-cases"
wait_http "http://127.0.0.1:18010/_matrix/client/versions" "synapse-fashion"

cargo run -p morpheus-cli -- demo seed \
  --scenario three-retail-instances \
  --config-dir config/e2e

catalog_count() {
  local service="$1"
  local table="$2"
  "${COMPOSE[@]}" exec -T "$service" \
    psql -U morpheus -d morpheus -tAc "SELECT count(*) FROM ${table};" | tr -d '[:space:]'
}

assert_count() {
  local service="$1"
  local table="$2"
  local expected="$3"
  local actual
  for _ in $(seq 1 60); do
    actual="$(catalog_count "$service" "$table")"
    if [[ "$actual" == "$expected" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "${service}.${table}: expected ${expected}, got ${actual}" >&2
  return 1
}

assert_count postgres-books catalog_sellers 14
assert_count postgres-books catalog_products 28
assert_count postgres-books catalog_offers 28
assert_count postgres-cases catalog_sellers 14
assert_count postgres-cases catalog_products 28
assert_count postgres-cases catalog_offers 28
assert_count postgres-fashion catalog_sellers 14
assert_count postgres-fashion catalog_products 28
assert_count postgres-fashion catalog_offers 28
assert_count postgres-fashion orders 1
assert_count postgres-fashion payments 1
assert_count postgres-fashion entitlements 1

echo "three-synapse e2e ok"
