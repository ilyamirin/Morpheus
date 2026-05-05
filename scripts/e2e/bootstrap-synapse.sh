#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

instances=(
  "books books.example"
  "cases cases.example"
  "fashion fashion.example"
)

federation_dir=".local/e2e/federation"
cert_dir="${federation_dir}/certs"
mkdir -p "$cert_dir"

if [[ ! -f "${federation_dir}/ca.crt" || ! -f "${federation_dir}/ca.key" ]]; then
  openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "${federation_dir}/ca.key" \
    -out "${federation_dir}/ca.crt" \
    -days 3650 \
    -subj "/CN=Morpheus E2E Federation CA" >/dev/null 2>&1
fi

generate_federation_cert() {
  local name="$1"
  local domain="$2"
  local key="${cert_dir}/${domain}.key"
  local csr="${cert_dir}/${domain}.csr"
  local crt="${cert_dir}/${domain}.crt"
  local ext="${cert_dir}/${domain}.ext"
  local nginx_conf="${federation_dir}/nginx-${name}.conf"

  if [[ ! -f "$key" || ! -f "$crt" ]]; then
    openssl req -newkey rsa:2048 -nodes \
      -keyout "$key" \
      -out "$csr" \
      -subj "/CN=${domain}" >/dev/null 2>&1
    cat >"$ext" <<EOF
subjectAltName = DNS:${domain}
extendedKeyUsage = serverAuth
EOF
    openssl x509 -req \
      -in "$csr" \
      -CA "${federation_dir}/ca.crt" \
      -CAkey "${federation_dir}/ca.key" \
      -CAcreateserial \
      -out "$crt" \
      -days 3650 \
      -sha256 \
      -extfile "$ext" >/dev/null 2>&1
    rm -f "$csr" "$ext"
  fi

  cat >"$nginx_conf" <<EOF
server {
  listen 8448 ssl;
  server_name ${domain};

  ssl_certificate /etc/nginx/certs/${domain}.crt;
  ssl_certificate_key /etc/nginx/certs/${domain}.key;
  ssl_protocols TLSv1.2 TLSv1.3;

  location / {
    proxy_pass http://synapse-${name}:8008;
    proxy_http_version 1.1;
    proxy_set_header Host \$host;
    proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto https;
  }
}
EOF
}

ensure_federation_config() {
  local homeserver="$1"
  if ! grep -q "^federation_verify_certificates:" "$homeserver"; then
    cat >>"$homeserver" <<'YAML'

# Morpheus E2E private federation TLS.
federation_verify_certificates: true
federation_custom_ca_list:
  - /etc/morpheus/e2e-ca.pem
ip_range_whitelist:
  - '172.16.0.0/12'
YAML
  else
    sed -i.bak 's/^federation_verify_certificates:.*/federation_verify_certificates: true/' "$homeserver"
    sed -i.bak 's#/data/e2e-ca.pem#/etc/morpheus/e2e-ca.pem#g' "$homeserver"
    rm -f "${homeserver}.bak"
    if ! grep -q "^federation_custom_ca_list:" "$homeserver"; then
      cat >>"$homeserver" <<'YAML'
federation_custom_ca_list:
  - /etc/morpheus/e2e-ca.pem
YAML
    fi
  fi
  if ! grep -q "^ip_range_whitelist:" "$homeserver"; then
    cat >>"$homeserver" <<'YAML'
ip_range_whitelist:
  - '172.16.0.0/12'
YAML
  fi
}

for item in "${instances[@]}"; do
  read -r name domain <<<"$item"
  data_dir=".local/e2e/synapse-${name}"
  mkdir -p "$data_dir"
  generate_federation_cert "$name" "$domain"

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
  ensure_federation_config "${data_dir}/homeserver.yaml"
done

echo "synapse bootstrap ok"
