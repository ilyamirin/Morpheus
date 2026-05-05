#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

instances=(
  "books books.example"
  "cases cases.example"
  "fashion fashion.example"
)

for item in "${instances[@]}"; do
  read -r name domain <<<"$item"
  data_dir=".local/e2e/synapse-${name}"
  mkdir -p "$data_dir"

  if [[ ! -f "${data_dir}/homeserver.yaml" ]]; then
    docker run --rm \
      -v "$PWD/${data_dir}:/data" \
      -e "SYNAPSE_SERVER_NAME=${domain}" \
      -e "SYNAPSE_REPORT_STATS=no" \
      matrixdotorg/synapse:latest generate
  fi

  cargo run -p morpheus-cli -- synapse registration \
    --config "config/e2e/${name}.toml" \
    --out "${data_dir}/morpheus-registration.yaml"

  if ! grep -q "/data/morpheus-registration.yaml" "${data_dir}/homeserver.yaml"; then
    cat >>"${data_dir}/homeserver.yaml" <<'YAML'

app_service_config_files:
  - /data/morpheus-registration.yaml
YAML
  fi
done

echo "synapse bootstrap ok"
