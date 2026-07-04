# EVM Escrow Full Implementation Design

Date: 2026-07-03

## Summary

Morpheus will complete the `evm_escrow` adapter as a code-complete MVP with production guardrails. The adapter will support real wallet-driven ERC-20 escrow transactions, an embedded JSON-RPC watcher in `morpheus-server`, verified payment event publication, and a local Anvil E2E path.

The backend will not hold or use an EVM signer key. Buyers, sellers, and arbiters execute custody-changing contract calls through their wallets. Morpheus treats the EVM chain as the custody source of truth and updates marketplace state only after the watcher verifies finalized escrow contract logs.

External audit, real mainnet deployment, monitored production RPC, and incident operations remain explicit release gates outside this implementation.

## Goals

- Add a real JSON-RPC watcher that scans escrow contract logs, verifies finality and receipt success, and publishes Morpheus payment events.
- Replace the current wallet plan-only behavior with real `viem` wallet transactions for `approve`, `deposit`, `release`, `refund`, and `partial_refund`.
- Keep release/refund wallet-driven: seller wallet sends `release`, arbiter wallet sends `refund` or `partial_refund`.
- Keep Morpheus protocol state derived from verified logs, not from user-submitted transaction hashes.
- Provide local E2E coverage using Anvil, Vyper deployments, and automated test transactions.
- Add production guardrails: config validation, watcher health/status, bounded scanning, retry-safe publishing, and operator documentation.

## Non-Goals

- Backend-managed private keys or backend-signed EVM transactions.
- Mainnet deployment automation.
- External audit completion.
- Multi-chain buyer UX.
- Native ETH escrow.
- Permit/EIP-2612 support.
- A separate watcher service binary in the first implementation.

## Architecture

The implementation uses the existing Morpheus boundary:

- Matrix/Morpheus is the marketplace lifecycle source of truth.
- The EVM escrow contract is the token custody source of truth.
- `order_hash` binds on-chain escrow state to locked Morpheus order/payment terms.

`morpheus-server` will run an embedded watcher when `[payments.evm_escrow].enabled = true`. The watcher reads the configured RPC URL from `rpc_url_env`, scans the configured escrow contract for known event topics, waits configured confirmations, verifies transaction receipts, decodes logs, matches logs to payment intent terms, and publishes existing `io.marketplace.payment.*` events.

The static UI will gain a minimal frontend build step. Source JavaScript can import `viem`; the built bundle remains a static asset served by `morpheus-server`. Wallet actions submit on-chain transactions, then show "submitted / waiting for watcher" state. UI state changes to authorized/captured/refunded only after the watcher publishes a Morpheus event and projections update.

## Components

### Rust JSON-RPC Client

Add a small JSON-RPC client for EVM calls. It should use structured request/response DTOs and narrow methods instead of a broad web3 abstraction.

Required methods:

- `eth_blockNumber`
- `eth_getLogs`
- `eth_getTransactionReceipt`

The client must parse hex quantities strictly, surface RPC errors with actionable messages, and never silently coerce malformed logs into valid events.

### Embedded Watcher

The watcher runs as a background Tokio task owned by `morpheus-server`.

On every tick:

1. Load `latest_scanned_block` from store.
2. Read current head block via RPC.
3. Compute `safe_to = head - confirmations`.
4. Scan from `latest_scanned_block + 1` to `safe_to` in bounded ranges.
5. Filter logs by escrow contract and known event topics.
6. Fetch and verify transaction receipt for each candidate log.
7. Decode known escrow events.
8. Look up expected payment terms by `order_hash`.
9. Verify chain id, escrow contract, token, amount, buyer, seller, and arbiter-relevant fields.
10. Persist log idempotency and decoded evidence.
11. Publish the matching Morpheus payment event.
12. Advance checkpoint only when all logs in the scanned range are safely handled or safely ignored.

Watcher status must be visible through admin output: enabled state, head block, finalized block, latest scanned block, accepted logs, duplicate logs, rejected logs, last error, and scan range limits.

### Payment Intent Lookup

The watcher must match logs to expected terms from persisted payment projections. `payment.intent.created` already contains the confirmation payload, including `order_hash`, token, amount units, escrow contract, and actors. The watcher should build an internal expected-payment index from store data instead of trusting UI-submitted transaction hashes.

If a valid log is final but the matching payment projection is not available yet, the watcher should not publish a payment event. It should either keep the checkpoint behind that block or record a retryable pending candidate. The implementation should prefer a simple retry-safe model: do not advance past a final unmatched log unless it is recorded as pending and will be retried.

### Wallet UI With Viem

Add a minimal UI build step for the existing static UI.

The UI source should use `viem` for:

- `ERC20.approve(escrow_contract, amount_units)`
- `MorpheusEscrow.deposit(order_hash, token, amount_units, seller, buyer, arbiter)`
- `MorpheusEscrow.release(order_hash)`
- `MorpheusEscrow.refund(order_hash)`
- `MorpheusEscrow.partial_refund(order_hash, buyer_amount)`

Wallet actions are role-scoped:

- Buyer sees approve/deposit when an `evm_escrow` order has payment confirmation and is waiting for deposit.
- Seller sees release when the order/payment status allows seller release.
- Arbiter sees refund/partial refund actions only for dispute/arbitration flows where the projected state and ruling permit them.

The UI must not mark payment as authorized, captured, or refunded after transaction submission. It should display submitted transaction hashes as pending UX only and rely on watcher-published projections for final state.

### Release And Refund Flow

Release/refund execution is wallet-driven:

- Seller wallet sends `release(order_hash)`.
- Arbiter wallet sends `refund(order_hash)`.
- Arbiter wallet sends `partial_refund(order_hash, buyer_amount)`.

Morpheus verifies the resulting logs before publishing:

- `EscrowReleased` -> `io.marketplace.payment.captured`
- `EscrowRefunded` -> `io.marketplace.payment.refunded`
- `EscrowPartiallyRefunded` -> `io.marketplace.payment.refunded` with partial amount evidence

The backend must not publish capture/refund events from UI button clicks or submitted transaction hashes.

### Local E2E

The local E2E should exercise the full custody loop:

1. Start Anvil.
2. Run Moccasin contract tests.
3. Deploy `MockERC20` and `MorpheusEscrow`.
4. Start `morpheus-server` with enabled `evm_escrow` config.
5. Create an `evm_escrow` order.
6. Seller accepts and creates payment intent.
7. Test wallet approves and deposits ERC-20.
8. Watcher publishes `payment.authorized`.
9. Seller wallet releases escrow.
10. Watcher publishes `payment.captured`.
11. Optional arbiter path runs a separate refund/partial-refund scenario.

Automation may use `cast` with deterministic Anvil private keys for E2E transaction submission. Browser wallet automation is not required for the E2E gate.

## Data Flow

```text
payment.intent.created
  -> store payment body with confirmation(order_hash, token, amount_units, actors)

watcher tick
  -> eth_blockNumber
  -> safe_to = head - confirmations
  -> eth_getLogs(checkpoint + 1, safe_to, escrow_contract, known topics)
  -> eth_getTransactionReceipt(tx_hash)
  -> receipt status == success
  -> decode escrow log
  -> expected terms lookup by order_hash
  -> verify terms
  -> record log idempotently
  -> publish Matrix payment event
  -> advance checkpoint
```

## Error Handling

- RPC unavailable: keep checkpoint unchanged, expose degraded watcher health, retry next tick.
- Unfinalized logs: skip until enough confirmations.
- Receipt missing: retry later.
- Receipt failed: record rejected log reason, do not publish payment event.
- Unknown topic: ignore or record rejected reason, do not publish payment event.
- Decode failure: record rejected reason, do not publish payment event.
- Terms mismatch: record rejected reason, do not publish payment event.
- Duplicate `(chain_id, tx_hash, log_index)`: no-op.
- Publish failure: do not advance checkpoint past the failed verified event unless the event is durably queued for retry.
- Store failure: keep checkpoint unchanged and surface last error.

## Testing

Rust tests:

- JSON-RPC response parsing for block numbers, logs, receipts, and RPC errors.
- Log decoding for each escrow event.
- Watcher finality enforcement.
- Receipt success enforcement.
- Deposit log publishes `payment.authorized`.
- Release log publishes `payment.captured`.
- Refund and partial refund logs publish `payment.refunded`.
- Mismatched token, amount, buyer, seller, arbiter, chain id, and contract address are rejected.
- Duplicate logs are idempotent.
- Publish failure is retry-safe.
- Admin watcher status reports checkpoint and error state.

UI tests:

- Built bundle includes viem-driven wallet actions.
- Buyer deposit action submits approve/deposit and reports pending watcher state.
- Seller release action submits release and reports pending watcher state.
- Arbiter refund/partial action submits refund tx and reports pending watcher state.
- UI does not display final payment state from transaction submission alone.

E2E:

- `make e2e-evm-escrow` runs when Foundry and Moccasin are installed.
- E2E validates deployed contract code exists, payment intent includes order hash, deposit transitions to authorized through watcher, and release transitions to captured through watcher.

## Production Guardrails

Config must include:

- `rpc_url_env`
- `confirmations`
- `poll_interval_secs`
- bounded scan range per tick
- escrow contract address
- allowlisted token contracts

Operator docs must state:

- no real mainnet funds before external audit;
- use monitored RPC providers in production;
- choose network-specific confirmations;
- keep wallet/private-key material outside Matrix events and committed config;
- run with conservative deposit limits;
- maintain pause/admin runbook for the escrow contract.

## Open Release Gates

These are not completed by this implementation:

- external smart-contract audit;
- public testnet soak test;
- production RPC provider monitoring;
- mainnet/L2 deployment ceremony;
- incident response drill.

They are explicit gates before production funds.
