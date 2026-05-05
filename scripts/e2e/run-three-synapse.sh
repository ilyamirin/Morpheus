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

urlencode() {
  python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"
}

json_field() {
  local field="$1"
  python3 -c 'import json, sys; print(json.load(sys.stdin)[sys.argv[1]])' "$field"
}

federation_smoke() {
  local txn_id="federation-smoke-$(date +%s)"
  local room_response
  local room_id
  room_response="$(curl -fsS \
    -X POST "http://127.0.0.1:18008/_matrix/client/v3/createRoom?access_token=books-as-token&user_id=$(urlencode '@market:books.example')" \
    -H "content-type: application/json" \
    -d '{
      "visibility": "private",
      "preset": "private_chat",
      "name": "Morpheus E2E federation smoke",
      "topic": "Verifies local TLS federation between E2E Synapse homeservers",
      "invite": ["@market:cases.example", "@market:fashion.example"],
      "creation_content": {"m.federate": true}
    }')"
  room_id="$(printf '%s' "$room_response" | json_field room_id)"

  curl -fsS \
    -X POST "http://127.0.0.1:18009/_matrix/client/v3/join/$(urlencode "$room_id")?access_token=cases-as-token&user_id=$(urlencode '@market:cases.example')" \
    -H "content-type: application/json" \
    -d '{}' >/dev/null
  curl -fsS \
    -X POST "http://127.0.0.1:18010/_matrix/client/v3/join/$(urlencode "$room_id")?access_token=fashion-as-token&user_id=$(urlencode '@market:fashion.example')" \
    -H "content-type: application/json" \
    -d '{}' >/dev/null
  curl -fsS \
    -X PUT "http://127.0.0.1:18010/_matrix/client/v3/rooms/$(urlencode "$room_id")/send/m.room.message/${txn_id}?access_token=fashion-as-token&user_id=$(urlencode '@market:fashion.example')" \
    -H "content-type: application/json" \
    -d '{"msgtype": "m.notice", "body": "morpheus federation smoke"}' >/dev/null

  echo "federation smoke ok (${room_id})"
}

federation_smoke

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
