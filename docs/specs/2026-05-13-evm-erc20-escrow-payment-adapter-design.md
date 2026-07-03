# EVM ERC-20 Escrow Payment Adapter Design

Date: 2026-05-13

Updated: 2026-07-03

## Summary

Morpheus should add an `evm_escrow` payment adapter that lets an ERC-20 stablecoin payment be held in an EVM smart contract until order completion or arbitration.

The adapter must preserve the current protocol boundary: Matrix/Morpheus records marketplace lifecycle state and evidence, while the EVM contract is the source of truth for token custody. Payment events continue to use the existing `io.marketplace.payment.*` event shapes. The adapter supplies verifiable evidence such as chain id, contract address, token address, transaction hash, block number, block hash, log index, amount, and escrow id.

The contract stack is **Vyper-first**. Contracts are written in Vyper and tested with Moccasin/Titanoboa. Foundry is used as supporting EVM tooling, primarily Anvil for local chains and Cast for manual inspection and smoke calls.

Initial scope is one configured ERC-20 stablecoin on a local/testnet EVM chain and one escrow contract per Morpheus instance.

## Approved Direction

Approved direction: **Vyper ERC-20 stablecoin escrow with seller release and arbiter override**.

The first implementation should support:

- payment adapter id: `evm_escrow`;
- ERC-20 stablecoin only, not native ETH/MATIC;
- one configured chain id;
- one configured escrow contract;
- one or more allowlisted token contracts, with one default token for the UI/demo;
- seller-instance release after Morpheus order completion;
- arbiter refund or partial refund during dispute/arbitration;
- backend watcher that converts final on-chain escrow logs into Morpheus payment events;
- Vyper contract source and Vyper-native tests as the canonical contract implementation;
- Foundry `anvil` and `cast` as local-chain and chain-inspection tools.

Out of scope for the first version:

- multi-chain UX;
- native coin escrow;
- EIP-2612 permit;
- cross-chain bridges;
- upgradeable contract architecture;
- open-ended token selection by buyers;
- Solidity/OpenZeppelin inheritance-based contract design;
- treating Foundry as the primary Vyper build/test framework;
- storing private keys, wallet secrets, or bearer payment URLs in Matrix events.

## Toolchain Model

The contract workspace should live inside the Morpheus repo but stay isolated from the Rust workspace:

```text
contracts/
  moccasin.toml
  src/
    MorpheusEscrow.vy
    MockERC20.vy
  tests/
    test_escrow.py
    test_invariants.py
  script/
    deploy.py
  abi/
    MorpheusEscrow.json
    MockERC20.json
  deployments/
    local.json
    testnet.example.json
  foundry/
    foundry.toml
    README.md
```

Responsibilities:

- Vyper compiler: compiles `.vy` contracts and emits ABI/bytecode.
- Moccasin/Titanoboa: owns contract unit tests, invariant-style tests, deployment helpers, and Vyper-native local execution.
- Foundry Anvil: runs a local EVM node for Morpheus E2E.
- Foundry Cast: performs manual chain calls, sends transactions in smoke scripts, decodes receipts, and inspects logs.
- Morpheus Rust E2E: reads generated deployment files and exercises the adapter through RPC.

Foundry should not be the source of truth for compiling Vyper. If a future Foundry release makes Vyper support first-class enough for this project, the spec can be revisited.

## Local Execution

Locally, the contract is stored in `contracts/src/MorpheusEscrow.vy` and executed inside a local Anvil EVM chain.

Local flow:

```text
Morpheus repo
  -> Moccasin/Vyper compiles .vy contracts
  -> Anvil runs local EVM JSON-RPC
  -> deploy.py deploys MockERC20 and MorpheusEscrow
  -> deployments/local.json stores chain id, contract addresses, token address, and deploy block
  -> morpheus-server reads local config
  -> Morpheus watcher scans Anvil logs through RPC
```

Anvil is only the local execution environment. The contract does not run inside Rust, Matrix, Synapse, or the Morpheus store.

## Production Deployment Options

The production deployment target is configurable per Morpheus instance.

Recommended progression:

1. **Local Anvil** for contract and watcher development.
2. **Public testnet** for wallet, RPC, confirmation, and explorer-facing dry runs.
3. **Per-instance escrow contract on an L2** for early production.

Production options:

- **Per-instance public L2 contract**: one escrow contract per Morpheus instance on a network such as Base, Arbitrum, Optimism, Polygon, or another selected EVM L2. This is the recommended production shape because roles, limits, and incident blast radius are isolated.
- **Shared public L2 contract**: one escrow contract for multiple Morpheus instances. This reduces deployments but makes roles, governance, limits, and incident response more complex.
- **Ethereum mainnet contract**: strongest settlement assumptions, higher transaction costs. Best for high-value flows, not MVP.
- **Private or permissioned EVM chain**: useful for enterprise/B2B demos or controlled deployments, but weakens the open public-settlement story.

Production rollout should use manual enablement, conservative deposit limits, monitored RPC providers, and explicit network allowlists.

## Contract Model

The escrow contract should be small, explicit, and easy to audit.

Core Vyper functions:

```text
deposit(order_hash: bytes32, token: address, amount: uint256, seller: address, buyer: address, arbiter: address)
release(order_hash: bytes32)
refund(order_hash: bytes32)
partial_refund(order_hash: bytes32, buyer_amount: uint256)
```

Core states:

```text
EMPTY -> DEPOSITED -> RELEASED
EMPTY -> DEPOSITED -> REFUNDED
EMPTY -> DEPOSITED -> PARTIALLY_REFUNDED
```

Storage model:

```text
admin: address
paused: bool
seller_operators: HashMap[address, bool]
arbiters: HashMap[address, bool]
allowed_tokens: HashMap[address, bool]
escrows: HashMap[bytes32, Escrow]
```

`Escrow` stores:

- status;
- token;
- amount;
- seller;
- buyer;
- arbiter;
- deposited_at block timestamp or block number.

Vyper design rules:

- inline access checks in every external function;
- explicit pause checks in every state-changing payment function;
- Vyper `@nonreentrant` on token-transfer paths;
- explicit ERC-20 interface for `transfer`, `transferFrom`, and `balanceOf` as needed;
- checked external calls using Vyper external call syntax;
- no inheritance-based role framework;
- no hidden modifier logic;
- no arbitrary external calls except the configured ERC-20 token transfer calls.

Events:

```text
EscrowDeposited(order_hash indexed, buyer indexed, seller indexed, token, amount)
EscrowReleased(order_hash indexed, seller indexed, token, amount)
EscrowRefunded(order_hash indexed, buyer indexed, token, amount)
EscrowPartiallyRefunded(order_hash indexed, buyer indexed, seller indexed, token, buyer_amount, seller_amount)
```

The contract must reject:

- deposits for non-allowlisted tokens;
- deposits where `msg.sender != buyer`;
- duplicate deposits for the same `order_hash`;
- zero amounts;
- zero buyer, seller, arbiter, or token addresses;
- release/refund calls before deposit;
- state transitions after a terminal state;
- partial refunds where `buyer_amount == 0` or `buyer_amount >= amount`;
- calls by unauthorized seller operators or arbiters;
- payment operations while paused.

## Order Hash Binding

`order_hash` binds the on-chain escrow to immutable Morpheus order terms. The backend should compute it from canonical fields locked by `order.created`, including:

- protocol id and version;
- instance id;
- order id;
- customer id;
- seller id;
- offer id;
- offer revision;
- price amount and currency;
- payment adapter;
- payment capture policy;
- chain id;
- token contract;
- token amount in smallest units;
- escrow contract;
- seller EVM address;
- buyer EVM address;
- arbiter actor and arbiter EVM address.

The hash must be deterministic and documented before mainnet use. For the MVP, it can be a server-side canonical JSON SHA-256 value stored in payment evidence and passed to the EVM contract as `bytes32`. If future wallet-side signing is added, this should become an EIP-712 typed data domain.

## Morpheus Runtime Model

Add an `evm_escrow` adapter service inside `morpheus-server`.

Responsibilities:

1. Build `payment.intent.created` for accepted orders using configured chain/token/contract data.
2. Expose confirmation metadata that the buyer UI can use for wallet payment.
3. Watch escrow contract logs through a configured EVM RPC endpoint.
4. Verify finality by waiting for configured confirmations.
5. Deduplicate logs by `(chain_id, tx_hash, log_index)`.
6. Match logs back to an order by `order_hash`.
7. Verify token, amount, buyer, seller, arbiter, and chain id against the payment intent.
8. Publish protocol-valid payment events after verification.

Mapping:

- `EscrowDeposited` -> `io.marketplace.payment.authorized`
- `EscrowReleased` -> `io.marketplace.payment.captured`
- `EscrowRefunded` -> `io.marketplace.payment.refunded`
- `EscrowPartiallyRefunded` -> `io.marketplace.payment.refunded` with partial amount evidence

The current seller payment endpoints can remain for the mock adapter. The EVM adapter should add dedicated server-side paths or internal jobs so sellers do not manually invent provider references.

## Chain Detection And Finality

Morpheus detects relevant on-chain transactions through contract event logs, not through buyer-supplied transaction hashes.

Watcher flow:

```text
1. Load chain config:
   chain_id
   rpc_url
   escrow_contract
   token_contracts
   confirmations
   start_block or last checkpoint

2. Scan logs:
   eth_getLogs over block ranges
   or websocket subscriptions for new logs

3. Filter logs:
   address == escrow_contract
   topic0 == EscrowDeposited/EscrowReleased/EscrowRefunded/EscrowPartiallyRefunded
   indexed order_hash == known order_hash

4. Verify transaction:
   receipt exists
   receipt status == success
   block hash and block number are stable
   log_index is present

5. Wait for finality:
   current_block - event_block >= confirmations

6. Validate event payload:
   order_hash matches payment intent
   token matches configured token
   amount matches amount_units
   buyer/seller/arbiter match intent
   chain_id matches config

7. Persist checkpoint:
   latest_scanned_block
   chain_id + tx_hash + log_index
   decoded event
   emitted Morpheus payment event id

8. Publish Matrix event:
   Deposited -> payment.authorized
   Released -> payment.captured
   Refunded/PartiallyRefunded -> payment.refunded
```

If a buyer submits a transaction hash to the UI, it is only a hint for UX. The watcher still verifies the log independently through trusted RPC before updating Morpheus state.

## Config

Extend config with an optional EVM escrow section:

```toml
[payments.evm_escrow]
enabled = true
chain_id = 31337
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
escrow_contract = "0x..."
default_token = "0x..."
confirmations = 1
poll_interval_secs = 2
deployments_path = "contracts/deployments/local.json"

[[payments.evm_escrow.tokens]]
symbol = "USDC"
contract = "0x..."
decimals = 6
currency = "USDC"
```

For public testnet or production, `confirmations` should be network-specific and more conservative than the Anvil default.

`instance.payment_adapters` must include `evm_escrow` before offers or orders can use it.

RPC URLs and any operator private keys must come from environment variables, wallet tooling, or an external signer. They must not be serialized into Matrix events or docs examples with real values.

## Store Changes

Persist enough watcher state to resume safely after restart:

- observed EVM logs with `(chain_id, tx_hash, log_index)`;
- latest scanned block per chain/contract;
- payment id to order hash mapping;
- on-chain escrow status;
- decoded event payload;
- evidence JSON used for emitted Morpheus payment events.

The store should keep existing projections unchanged where possible. Any new table should be adapter-specific, not a rewrite of the general payment projection model.

## API And UI

Buyer UI should show wallet payment only when the selected order uses `evm_escrow`.

MVP buyer flow:

1. Create order.
2. Wait for seller acceptance and payment intent.
3. Show token, network, amount, escrow contract, and order hash.
4. Ask wallet to approve the escrow contract for `amount_units`.
5. Ask wallet to call `deposit`.
6. Optionally show the submitted transaction hash as pending UX only.
7. Show pending state until the watcher publishes `payment.authorized`.

Seller UI:

- continue to show order lifecycle actions;
- hide manual mock payment controls for `evm_escrow` orders;
- show on-chain escrow status from projections/evidence;
- allow seller completion flow to trigger or instruct `release`, depending on whether an operator signer is configured.

Admin UI:

- show adapter enabled/disabled state;
- show chain id, escrow contract, default token, confirmations;
- show local deployment file path when running against Anvil;
- show watcher health and latest scanned block.

## Error Handling

Backend errors should be explicit and stable:

- `EVM_ESCROW_DISABLED`
- `EVM_UNSUPPORTED_CHAIN`
- `EVM_UNSUPPORTED_TOKEN`
- `EVM_AMOUNT_MISMATCH`
- `EVM_ORDER_HASH_MISMATCH`
- `EVM_ACTOR_ADDRESS_MISMATCH`
- `EVM_LOG_NOT_FINAL`
- `EVM_DUPLICATE_LOG`
- `EVM_RPC_UNAVAILABLE`
- `EVM_CONTRACT_REVERTED`
- `EVM_DEPLOYMENT_NOT_FOUND`

The watcher must not emit Morpheus payment events from unfinalized logs. If an RPC endpoint is unavailable, the adapter should report degraded health and retry from the last durable scanned block.

## Security Notes

The contract holds real funds, so the first implementation should remain deliberately small.

Required safeguards:

- allowlisted ERC-20 token contracts;
- no arbitrary external calls except token transfers;
- inline authorization checks;
- explicit pause checks;
- checks-effects-interactions for release/refund flows;
- Vyper `@nonreentrant` on state-changing token transfer paths;
- role separation between admin, seller operator, and arbiter;
- no upgradeability in the first contract unless a separate audited upgrade policy exists;
- per-order terminal states to prevent double release/refund;
- exact amount matching between Morpheus intent and on-chain deposit;
- watcher-side verification of receipt status, confirmations, token, amount, and actor addresses.

Before any mainnet deployment, the contract needs dedicated review, property/invariant tests, and at least one external audit pass.

## Testing

Moccasin/Titanoboa contract tests:

- deposit succeeds for allowlisted token and exact amount;
- deposit requires `msg.sender == buyer`;
- duplicate deposit fails;
- non-allowlisted token fails;
- release transfers full amount to seller;
- refund transfers full amount to buyer;
- partial refund splits funds correctly;
- unauthorized release/refund fails;
- terminal states cannot transition again;
- paused contract blocks deposit/release/refund;
- event payloads contain the expected indexed order hash and actors.

Foundry tool smoke tests:

- Anvil starts with deterministic accounts;
- deployment script writes `deployments/local.json`;
- Cast can read escrow state;
- Cast can decode `EscrowDeposited` and `EscrowReleased` logs from local receipts.

Rust tests:

- config validation accepts valid `evm_escrow` config;
- local deployment file is loaded correctly;
- intent generation creates deterministic `order_hash`;
- unsupported token/chain is rejected;
- watcher deduplicates logs;
- watcher waits for configured confirmations;
- watcher rejects mismatched token, amount, buyer, seller, or arbiter;
- deposited log publishes `payment.authorized`;
- released log publishes `payment.captured`;
- refunded log publishes `payment.refunded`;
- adapter evidence passes protocol validation.

E2E test:

- Anvil local EVM node runs;
- MockERC20 and MorpheusEscrow are deployed from Vyper artifacts;
- buyer creates order;
- seller accepts;
- payment intent is generated;
- buyer approves and deposits ERC-20 into escrow;
- watcher publishes authorization;
- seller completes and releases;
- watcher publishes capture;
- final order projection reaches captured/completed state.

## Rollout

Recommended rollout order:

1. Update design and protocol notes.
2. Add isolated `contracts/` workspace with Vyper/Moccasin/Titanoboa.
3. Implement and test `MorpheusEscrow.vy` and `MockERC20.vy`.
4. Add Anvil/Cast local tooling and deployment JSON output.
5. Add Rust config and adapter interfaces.
6. Add watcher persistence.
7. Add EVM log watcher and event publisher.
8. Add buyer UI wallet flow.
9. Add local Anvil E2E.
10. Run public testnet dry run with capped token amounts.

Production rollout should require manual enablement per instance and conservative deposit limits until the contract and watcher have been exercised in testnet conditions.
