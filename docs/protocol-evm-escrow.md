# EVM Escrow Payment Adapter

`evm_escrow` lets Morpheus orders use an ERC-20 escrow contract for token custody.
Morpheus and Matrix remain the marketplace lifecycle source of truth; the EVM chain
is the custody source of truth. The bridge between them is `order_hash`, generated
from locked order terms and adapter configuration.

## Local Development

Local contracts are stored in `contracts/src`:

- `MorpheusEscrow.vy` holds ERC-20 funds until seller release, arbiter refund, or arbiter partial refund.
- `MockERC20.vy` provides a local token for contract and smoke tests.

The local execution environment is a Foundry Anvil chain. The Vyper contracts do
not run inside Rust, Matrix, Synapse, or the Morpheus store.

```text
Morpheus repo
  -> Moccasin/Titanoboa tests Vyper contracts
  -> Anvil runs local EVM JSON-RPC at 127.0.0.1:8545
  -> contracts/script/deploy.py deploys MockERC20 and MorpheusEscrow
  -> contracts/deployments/local.json stores chain id, addresses, and deploy block
  -> morpheus-server reads [payments.evm_escrow] config
```

Useful local commands:

```sh
cd contracts
mox test

anvil --chain-id 31337
mox run script/deploy.py --network local
cast code "$(jq -r .escrow_contract deployments/local.json)" --rpc-url http://127.0.0.1:8545
```

Run the full local server/watcher flow:

```sh
make e2e-evm-escrow
```

Foundry is supporting tooling only: Anvil provides local JSON-RPC, and Cast is used
for manual inspection. Moccasin/Titanoboa remain the source of truth for compiling
and testing Vyper contracts.

## Configuration

The adapter is opt-in through `config/local.toml` or an equivalent deployment config.
When enabled, `instance.payment_adapters` must include `evm_escrow`.

```toml
[payments.evm_escrow]
enabled = true
chain_id = 31337
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
escrow_contract = "0x..."
default_token = "0x..."
confirmations = 1
poll_interval_secs = 2
start_block = 0
max_scan_blocks = 100
deployments_path = "contracts/deployments/local.json"

[[payments.evm_escrow.tokens]]
symbol = "USDC"
contract = "0x..."
decimals = 6
currency = "USDC"
```

RPC URLs, private keys, and signer credentials must stay outside Matrix events and
committed config. Use environment variables, wallet tooling, or an external signer.

## Payment Flow

```text
Buyer creates order with payment_adapter = evm_escrow
  -> Seller accepts order
  -> Seller requests EVM escrow payment intent
  -> Server computes order_hash and confirmation metadata
  -> Buyer wallet approves token spend
  -> Buyer wallet deposits ERC-20 into MorpheusEscrow
  -> Watcher verifies finalized contract log
  -> Server publishes Morpheus payment event
```

Contract log mapping:

- `EscrowDeposited` -> `io.marketplace.payment.authorized`
- `EscrowReleased` -> `io.marketplace.payment.captured`
- `EscrowRefunded` -> `io.marketplace.payment.refunded`
- `EscrowPartiallyRefunded` -> `io.marketplace.payment.refunded` with partial amount evidence

Buyer-submitted transaction hashes are UX hints only. Morpheus payment state must
come from verified contract logs read through trusted RPC.

## Watcher Operation

The embedded watcher starts only when `[payments.evm_escrow].enabled = true`.
It reads the JSON-RPC URL from `rpc_url_env`, scans bounded block ranges from the
durable checkpoint, waits configured confirmations, verifies successful receipts,
deduplicates `(chain_id, tx_hash, log_index)`, and publishes payment events only
from matching finalized logs. `start_block` and `max_scan_blocks` keep local
replay and production backfills explicit and bounded.

Morpheus never treats a submitted transaction hash as final payment state. Wallet
transaction hashes are useful for UX and debugging only; projected payment state
is updated after the watcher verifies finalized logs.

## Wallet Roles

- Buyer wallet submits ERC-20 `approve` and escrow `deposit`.
- Seller/operator wallet submits escrow `release`.
- Arbiter wallet submits escrow `refund` and `partial_refund`.
- Morpheus server does not hold private keys or sign custody-changing transactions.

## Watcher Verification

The watcher accepts payment state only after it can match the decoded contract log
to the payment intent:

- chain id equals configured `chain_id`;
- log address equals configured `escrow_contract`;
- token equals the intent token;
- amount equals the intent amount in token units;
- buyer, seller, and arbiter-relevant addresses match the intent for that event type;
- `order_hash` equals the server-computed locked order hash;
- log identity `(chain_id, tx_hash, log_index)` has not already been processed.

## Production Options

Recommended rollout path:

1. Local Anvil for contract and watcher development.
2. Public testnet for wallet, RPC, confirmation, and explorer dry runs.
3. Per-instance escrow contract on an L2 for early production.

Production deployment options:

- **Per-instance public L2 contract**: recommended first production shape because roles, limits, and incident blast radius are isolated.
- **Shared public L2 contract**: lower deployment overhead, but harder role governance and incident response.
- **Ethereum mainnet contract**: strongest settlement assumptions with higher cost, better suited to high-value flows.
- **Private or permissioned EVM chain**: useful for controlled enterprise demos, but weaker public-settlement guarantees.

Before any mainnet deployment, the contract needs dedicated security review,
invariant/property testing, monitored RPC providers, conservative deposit limits,
and an explicit incident response process.

## Production Guardrails

- Do not use mainnet funds before an external contract audit.
- Use monitored RPC providers and alert on lag, failed calls, and reorg-sensitive ranges.
- Configure network-specific confirmations instead of reusing local Anvil values.
- Use conservative deposit limits until the adapter has production history.
- Keep wallet private keys, RPC credentials, and signer material outside Matrix events and committed config.
- Keep a pause/admin runbook ready before testnet or production funds.
- Treat deployment JSON as an operator artifact; production config must still pin the intended contract and token addresses explicitly.
