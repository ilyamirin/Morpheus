# EVM Escrow Production Runbook

This runbook covers the first production-ready operating shape for the
`evm_escrow` adapter. Morpheus records marketplace state in Matrix and its store;
the EVM escrow contract is the custody source of truth for ERC-20 funds.

## Deployment Model

Recommended rollout order:

1. Local Anvil for development and CI smoke tests.
2. Public testnet for wallet, RPC, explorer, confirmation, and runbook drills.
3. One escrow contract per Morpheus instance on the selected production L2.

The production contract should be deployed from the Vyper source in
`contracts/src/MorpheusEscrow.vy` using the same Moccasin deployment path used by
local E2E. Foundry tools are supporting tools: `anvil` for local execution and
`cast` for operator inspection or smoke calls.

## Required Operator Inputs

- EVM network and `chain_id`.
- Escrow contract address.
- Supported ERC-20 token addresses, symbols, currencies, and decimals.
- Market-specific escrow policy: min/max order amount, high-value threshold,
  timeout windows, estimated fee metadata, and risk categories.
- RPC provider URL stored in the environment variable named by `rpc_url_env`.
- Network-specific `confirmations`, `max_scan_blocks`, and `rescan_depth`.
- Deployment artifact path for local/test environments.
- Named arbiters, authority model, accepted evidence, response windows, and
  escalation contacts for disputes.
- Admin, seller, buyer, homeserver, and Matrix appservice credentials outside Git.

Production config must pin contract and token addresses explicitly. Deployment
JSON is an operator artifact, not a substitute for reviewed runtime config.

## Config Checklist

```toml
[payments.evm_escrow]
enabled = true
chain_id = 31337
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
escrow_contract = "0x..."
default_token = "0x..."
confirmations = 12
poll_interval_secs = 5
start_block = 0
max_scan_blocks = 1000
rescan_depth = 12

[[payments.evm_escrow.tokens]]
symbol = "USDC"
contract = "0x..."
decimals = 6
currency = "USDC"

[payments.evm_escrow.policy]
min_order_amount = "1.00"
max_order_amount = "100.00"
high_value_amount = "50.00"
deposit_timeout_secs = 900
fulfillment_timeout_secs = 86400
buyer_review_timeout_secs = 3600
dispute_timeout_secs = 172800
estimated_fee_units = "1000000000000000"
fee_token_symbol = "ETH"
risk_categories = ["electronics", "preorder"]
```

Use larger confirmations and overlap on networks with slower finality or more
observable reorg risk. Keep `max_scan_blocks` below provider log-query limits.
Treat the policy block as required for production: empty defaults make local
development easier, but production operators must explicitly set market limits,
timeouts, fee metadata, and risk categories.

## Readiness Checks

Before accepting escrow orders:

1. Validate the runtime config:

   ```sh
   cargo run -p morpheus-cli -- config validate --config config/e2e/evm-escrow.toml
   ```

2. Verify contract code exists at the configured address:

   ```sh
   cast code "$ESCROW_CONTRACT" --rpc-url "$MORPHEUS_EVM_RPC_URL"
   ```

3. Verify the admin status endpoint:

   ```sh
   curl -fsS -H "Authorization: Bearer $MORPHEUS_ADMIN_TOKEN" \
     "$MORPHEUS_URL/admin/evm-escrow/status"
   ```

The status response must show `enabled: true`, the intended `chain_id`,
`escrow_contract`, `confirmations`, `max_scan_blocks`, `rescan_depth`, current
checkpoint, watcher `last_scan` or `last_error` fields, and the full `policy`
metadata: min/max order amount, high-value threshold, timeout windows,
estimated fee, fee token symbol, and risk categories.

## Watcher Operation

The embedded watcher scans finalized block ranges through the configured RPC
endpoint. Each scan starts from the last durable checkpoint, with `rescan_depth`
overlap on follow-up scans. Duplicate logs are ignored by the durable
`(chain_id, tx_hash, log_index)` identity, so overlapping scans should not
republish payment events.

Accepted logs must pass all checks:

- receipt success and matching transaction/block identity;
- configured chain id and escrow contract;
- token, amount, buyer, seller, and order hash match the payment intent;
- log has the required number of confirmations;
- log identity has not already been processed.

## Manual Replay

Use replay after RPC outage, watcher restart, or a suspected missed finalized log:

```sh
curl -fsS -X POST -H "Authorization: Bearer $MORPHEUS_ADMIN_TOKEN" \
  "$MORPHEUS_URL/admin/evm-escrow/replay"
```

Replay uses the same scan logic as the background watcher and updates the shared
runtime status. If a reorg or provider indexing delay is suspected, keep the
checkpoint in place and rely on `rescan_depth` overlap first. Direct checkpoint
changes should be a database-admin operation with an incident note and a known
safe block target.

## Monitoring

Alert on:

- watcher `last_error` present for more than one polling interval;
- checkpoint not advancing while finalized escrow activity exists;
- RPC failures, rate limits, or log-query range errors;
- repeated rejected logs;
- unexpected contract address, token address, or chain id in evidence;
- admin replay returning an error.
- estimated network fees approaching the typical order value for the market.

Pause creation of low-value escrow orders when configured fee metadata or live
network fee monitoring shows that expected fees make the order uneconomic. Resume
only after the fee threshold, supported network, or minimum order amount has been
updated and communicated to sellers and buyers.

Store RPC credentials, private keys, and signer material in the deployment secret
manager. Morpheus server must not hold buyer, seller, or arbiter private keys.

## Incident Actions

If RPC is degraded, switch the `rpc_url_env` value to a healthy provider and
restart the server while keeping the durable checkpoint intact. If a bad contract
address or token address was configured, stop accepting new escrow orders,
correct config, restart, and replay from the last known safe checkpoint.

For suspected contract vulnerability, stop new escrow order creation immediately,
preserve logs, export status and checkpoint evidence, and route funds only
through the reviewed arbiter/admin procedure for that deployment. The current
contract bytecode does not expose an operator pause method, so production
deployments must treat order-intake shutdown, conservative limits, and arbiter
procedures as the available incident controls unless a reviewed pause mechanism
is added in a later contract version.

## Launch Gates

Do not use production funds until all gates pass:

- Vyper unit tests pass with Moccasin/Titanoboa.
- Rust workspace tests pass.
- UI wallet tests and UI build pass.
- Local `make e2e-evm-escrow` passes.
- Testnet escrow deposit, release, refund, and replay drill pass.
- External smart-contract review is complete for the deployed bytecode.
- Market-specific `max_order_amount`, `high_value_amount`, timeout windows,
  estimated fee metadata, and `risk_categories` are configured and visible in
  `/admin/evm-escrow/status`.
- Named arbiters, accepted evidence, response windows, and decision finality are
  documented for the market before high-value or risky categories are enabled.
- Estimated network fees are monitored against typical order value, with explicit
  pause criteria for low-value orders when fees consume an unacceptable share of
  the order.
- Deposit limits, order-intake shutdown authority, arbiter authority, and
  incident contacts are documented for the selected network.
