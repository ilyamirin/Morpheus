# EVM ERC-20 Escrow Payment Adapter Design

Date: 2026-05-13

## Summary

Morpheus should add an `evm_escrow` payment adapter that lets an ERC-20 stablecoin payment be held in an EVM smart contract until order completion or arbitration.

The adapter must preserve the current protocol boundary: Matrix/Morpheus records marketplace lifecycle state and evidence, while the EVM contract is the source of truth for token custody. Payment events continue to use the existing `io.marketplace.payment.*` event shapes. The adapter supplies verifiable evidence such as chain id, contract address, token address, transaction hash, block number, block hash, log index, amount, and escrow id.

Initial scope is one configured ERC-20 stablecoin on a testnet and one escrow contract per Morpheus instance.

## Approved Direction

Approved direction: **ERC-20 stablecoin escrow with seller release and arbiter override**.

The first implementation should support:

- payment adapter id: `evm_escrow`;
- ERC-20 stablecoin only, not native ETH/MATIC;
- one configured chain id;
- one configured escrow contract;
- one or more allowlisted token contracts, with one default token for the UI/demo;
- seller-instance release after Morpheus order completion;
- arbiter refund or partial refund during dispute/arbitration;
- backend watcher that converts final on-chain escrow logs into Morpheus payment events.

Out of scope for the first version:

- multi-chain UX;
- native coin escrow;
- EIP-2612 permit;
- cross-chain bridges;
- upgradeable contract architecture;
- open-ended token selection by buyers;
- storing private keys, wallet secrets, or bearer payment URLs in Matrix events.

## Contract Model

The escrow contract should be small and purpose-specific.

Core functions:

```solidity
deposit(bytes32 orderHash, address token, uint256 amount, address seller, address buyer, address arbiter)
release(bytes32 orderHash)
refund(bytes32 orderHash)
partialRefund(bytes32 orderHash, uint256 buyerAmount)
```

Core states:

```text
None -> Deposited -> Released
None -> Deposited -> Refunded
None -> Deposited -> PartiallyRefunded
```

Roles:

- `DEFAULT_ADMIN_ROLE`: manages roles, token allowlist, pause state, and instance-level configuration.
- `SELLER_OPERATOR_ROLE`: may release deposited funds after Morpheus has completed the order.
- `ARBITER_ROLE`: may refund or partially refund a deposited escrow.

Recommended implementation primitives:

- OpenZeppelin `AccessControl` for roles;
- OpenZeppelin `Pausable` for emergency stop;
- OpenZeppelin `ReentrancyGuard` and checks-effects-interactions around token transfers;
- OpenZeppelin `SafeERC20` for ERC-20 transfer handling.

Events:

```solidity
EscrowDeposited(bytes32 indexed orderHash, address indexed buyer, address indexed seller, address token, uint256 amount)
EscrowReleased(bytes32 indexed orderHash, address indexed seller, address token, uint256 amount)
EscrowRefunded(bytes32 indexed orderHash, address indexed buyer, address token, uint256 amount)
EscrowPartiallyRefunded(bytes32 indexed orderHash, address indexed buyer, address indexed seller, address token, uint256 buyerAmount, uint256 sellerAmount)
```

The contract must reject:

- deposits for non-allowlisted tokens;
- duplicate deposits for the same `orderHash`;
- zero amounts;
- release/refund calls before deposit;
- state transitions after a terminal state;
- partial refunds where `buyerAmount >= amount`;
- calls by unauthorized accounts.

## Order Hash Binding

`orderHash` binds the on-chain escrow to immutable Morpheus order terms. The backend should compute it from canonical fields locked by `order.created`, including:

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
- arbiter actor or configured arbiter address.

The hash must be deterministic and documented before mainnet use. For the MVP, it can be a server-side canonical JSON SHA-256 value stored in payment evidence and passed to the EVM contract as `bytes32`. If future wallet-side signing is added, this should become an EIP-712 typed data domain.

## Morpheus Runtime Model

Add an `evm_escrow` adapter service inside `morpheus-server`.

Responsibilities:

1. Build `payment.intent.created` for accepted orders using configured chain/token/contract data.
2. Expose confirmation metadata that the buyer UI can use for wallet payment.
3. Watch escrow contract logs through a configured EVM RPC endpoint.
4. Verify finality by waiting for configured confirmations.
5. Deduplicate logs by `(chain_id, tx_hash, log_index)`.
6. Match logs back to an order by `orderHash`.
7. Publish protocol-valid payment events after verification.

Mapping:

- `EscrowDeposited` -> `io.marketplace.payment.authorized`
- `EscrowReleased` -> `io.marketplace.payment.captured`
- `EscrowRefunded` -> `io.marketplace.payment.refunded`
- `EscrowPartiallyRefunded` -> `io.marketplace.payment.refunded` with partial amount evidence

The current seller payment endpoints can remain for the mock adapter. The EVM adapter should add dedicated server-side paths or internal jobs so sellers do not manually invent provider references.

## Config

Extend config with an optional EVM escrow section:

```toml
[payments.evm_escrow]
enabled = true
chain_id = 11155111
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
escrow_contract = "0x..."
default_token = "0x..."
confirmations = 6
poll_interval_secs = 10

[[payments.evm_escrow.tokens]]
symbol = "USDC"
contract = "0x..."
decimals = 6
currency = "USDC"
```

`instance.payment_adapters` must include `evm_escrow` before offers or orders can use it.

RPC URLs and any operator private keys must come from environment variables or an external signer. They must not be serialized into Matrix events or docs examples with real values.

## Store Changes

Persist enough watcher state to resume safely after restart:

- observed EVM logs with `(chain_id, tx_hash, log_index)`;
- latest scanned block per chain/contract;
- payment id to order hash mapping;
- on-chain escrow status;
- evidence JSON used for emitted Morpheus payment events.

The store should keep existing projections unchanged where possible. Any new table should be adapter-specific, not a rewrite of the general payment projection model.

## API And UI

Buyer UI should show wallet payment only when the selected order uses `evm_escrow`.

MVP buyer flow:

1. Create order.
2. Wait for seller acceptance and payment intent.
3. Show token, network, amount, and escrow contract.
4. Ask wallet to approve the escrow contract for `amount_units`.
5. Ask wallet to call `deposit`.
6. Show pending state until the watcher publishes `payment.authorized`.

Seller UI:

- continue to show order lifecycle actions;
- hide manual mock payment controls for `evm_escrow` orders;
- show on-chain escrow status from projections/evidence;
- allow seller completion flow to trigger or instruct `release`, depending on whether an operator signer is configured.

Admin UI:

- show adapter enabled/disabled state;
- show chain id, escrow contract, default token, confirmations;
- show watcher health and latest scanned block.

## Error Handling

Backend errors should be explicit and stable:

- `EVM_ESCROW_DISABLED`
- `EVM_UNSUPPORTED_CHAIN`
- `EVM_UNSUPPORTED_TOKEN`
- `EVM_AMOUNT_MISMATCH`
- `EVM_ORDER_HASH_MISMATCH`
- `EVM_LOG_NOT_FINAL`
- `EVM_DUPLICATE_LOG`
- `EVM_RPC_UNAVAILABLE`
- `EVM_CONTRACT_REVERTED`

The watcher must not emit Morpheus payment events from unfinalized logs. If an RPC endpoint is unavailable, the adapter should report degraded health and retry from the last durable scanned block.

## Security Notes

The contract holds real funds, so the first implementation should remain deliberately small.

Required safeguards:

- allowlisted ERC-20 token contracts;
- no arbitrary external calls except token transfers;
- checks-effects-interactions for release/refund flows;
- reentrancy guard on state-changing token transfer paths;
- pausable emergency stop;
- role separation between admin, seller operator, and arbiter;
- no upgradeability in the first contract unless a separate audited upgrade policy exists;
- per-order terminal states to prevent double release/refund;
- exact amount matching between Morpheus intent and on-chain deposit.

Before any mainnet deployment, the contract needs dedicated review, fuzz/property tests, and at least one external audit pass.

## Testing

Contract tests:

- deposit succeeds for allowlisted token and exact amount;
- duplicate deposit fails;
- non-allowlisted token fails;
- release transfers full amount to seller;
- refund transfers full amount to buyer;
- partial refund splits funds correctly;
- unauthorized release/refund fails;
- terminal states cannot transition again;
- paused contract blocks deposit/release/refund.

Rust tests:

- config validation accepts valid `evm_escrow` config;
- intent generation creates deterministic `orderHash`;
- unsupported token/chain is rejected;
- watcher deduplicates logs;
- watcher waits for configured confirmations;
- deposited log publishes `payment.authorized`;
- released log publishes `payment.captured`;
- refunded log publishes `payment.refunded`;
- adapter evidence passes protocol validation.

E2E test:

- local EVM node with mock USDC;
- buyer creates order;
- seller accepts;
- payment intent is generated;
- buyer deposits ERC-20 into escrow;
- watcher publishes authorization;
- seller completes and releases;
- watcher publishes capture;
- final order projection reaches captured/completed state.

## Rollout

Recommended rollout order:

1. Add design and protocol notes.
2. Build and test Solidity contract in an isolated package.
3. Add Rust config and adapter interfaces.
4. Add watcher persistence.
5. Add EVM log watcher and event publisher.
6. Add buyer UI wallet flow.
7. Add local EVM E2E.
8. Testnet dry run with capped token amounts.

Production rollout should require manual enablement per instance and conservative deposit limits until the contract and watcher have been exercised in testnet conditions.
