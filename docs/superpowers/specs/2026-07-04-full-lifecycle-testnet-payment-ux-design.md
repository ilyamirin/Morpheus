# Full Lifecycle Testnet Payment UX Rehearsal Design

Date: 2026-07-04

## Goal

Build a browser-based rehearsal flow for the complete EVM escrow lifecycle on a
public testnet. The flow must exercise buyer deposit, seller release, arbiter
full refund, arbiter partial refund, watcher confirmation, explorer links, and
visible diagnostics without enabling real-money production payments.

The rehearsal should feel close to the future production L2 experience while
remaining explicitly testnet-only.

## Scope

This design includes:

- Buyer flow: wallet connect, chain and address validation, ERC-20 allowance
  check, approve, deposit, pending transaction state, confirmations, and final
  escrow-funded state.
- Seller flow: funded escrow visibility, release action, watcher-confirmed
  captured state.
- Admin or arbiter flow: full refund and partial refund actions with explicit
  confirmation before signing.
- Shared payment status panel: chain id, escrow contract, token, amount, order
  hash, transaction hash, confirmation depth, watcher state, and explorer links.
- Testnet rehearsal mode: enabled through configuration and clearly separated
  from production mode.
- Browser lifecycle test coverage with mocked wallet, provider, and API state.
- Manual testnet runbook updates for a real wallet and public testnet RPC.

This design does not include:

- Mainnet or production L2 real-money enablement.
- Gas sponsorship or relayer support.
- Multi-arbiter governance.
- Automatic release or refund timers.
- Non-EVM payment rails.

## Architecture

The feature extends the existing Morpheus server UI payment surface under
`crates/morpheus-server/ui/src` rather than adding a separate app.

Primary components:

| Component | Responsibility |
|---|---|
| `evmWallet.js` | Low-level wallet actions: connect, approve, deposit, release, refund, and partial refund. |
| Payment lifecycle UI module | Renders buyer, seller, and admin role actions for the escrow lifecycle. |
| Payment status panel | Shows intent data, chain, token, amount, transaction evidence, confirmations, and watcher status. |
| Explorer link builder | Builds testnet explorer links for transactions, contracts, and addresses from network config. |
| Rehearsal mode config | Enables testnet-only copy, warnings, and network metadata without production assumptions. |
| Playwright lifecycle test | Exercises the full browser flow with mocked wallet, provider, and server responses. |

Backend changes should remain minimal. If existing intent, evidence, and watcher
status responses already expose the necessary fields, the UI should consume those
fields. If fields are missing, add only the narrow response fields needed for the
status panel and role validation.

The UI must not decide that payment is final based on a submitted wallet
transaction. Final payment state comes only from watcher-confirmed Morpheus
payment events or watcher/status API evidence.

## Data Flow

```text
Seller accepts order
  -> UI loads or requests EVM payment intent
  -> Buyer opens payment panel
  -> UI checks wallet chain and buyer address
  -> Buyer approves ERC-20 allowance
  -> Buyer deposits into escrow contract
  -> UI records submitted tx as a pending UX hint
  -> Watcher scans finalized testnet logs
  -> Server publishes authorized payment event
  -> UI shows escrow funded

Escrow funded
  -> Seller opens settlement panel
  -> Seller submits release
  -> Watcher verifies EscrowReleased log
  -> UI shows payment captured

Alternative dispute path
  -> Admin or arbiter opens action panel
  -> Admin chooses full refund or partial refund
  -> UI requires explicit confirmation
  -> Wallet submits refund or partial_refund
  -> Watcher verifies refund log
  -> UI shows refunded or partially refunded state
```

## UI State Model

The browser flow should represent these states explicitly:

- `intent_ready`
- `wallet_connected`
- `approval_pending`
- `deposit_submitted`
- `deposit_confirming`
- `escrow_funded`
- `release_submitted`
- `captured`
- `refund_submitted`
- `refunded`
- `partial_refund_submitted`
- `partially_refunded`
- `watcher_lagging`
- `rpc_error`
- `wallet_error`
- `chain_mismatch`

The UI must make the distinction between "transaction submitted" and "payment
verified by Morpheus" clear in every role-specific flow.

## Error Handling And UX Rules

| Situation | Expected UX |
|---|---|
| Wallet is not connected | Show the connect action and supported testnet chain ids. |
| Wallet is on the wrong chain | Show the expected chain and a switch-network action or instruction. |
| Wallet address does not match the required role | Block the action and show the expected role address. |
| ERC-20 allowance is insufficient | Show the approve step before deposit. |
| Transaction is submitted but watcher has not finalized it | Show pending transaction, explorer link, and watcher-waiting state. |
| RPC or watcher is lagging | Do not mark payment successful; show diagnostics. |
| Receipt is reorged or mismatched | Show rejected or retry-needed state without moving to funded or captured. |
| Refund or partial refund is about to be signed | Show irreversible consequence and buyer/seller amounts before signing. |
| Contract or token does not match configuration | Block the action and show config mismatch. |

Buyer and seller surfaces should be concise: state, next action, and explorer
link. Admin surfaces can show raw diagnostics such as block number, transaction
hash, log index, order hash, and evidence payload.

## Testing And Acceptance Criteria

The implementation is accepted when:

1. `npm run test:ui-wallet-flow` covers deposit, release, full refund, partial
   refund, watcher lag, chain mismatch, and wallet rejection with mocked browser
   state.
2. The manual testnet runbook describes a full browser rehearsal using a real
   wallet and public testnet RPC.
3. `npm run build:ui` passes.
4. Existing Rust payment intent, status, and watcher tests pass.
5. `make e2e-evm-escrow` remains green.
6. No UI state treats a submitted transaction hash as final payment success
   without watcher evidence.
7. Testnet warnings and explorer links are driven by configuration rather than
   hardcoded to one network.

## Rollout Notes

This feature is a rehearsal step, not a production money gate. It should help
operators and developers validate the payment lifecycle, explain wallet actions
to users, and diagnose watcher/RPC issues before any real-money launch.

After this lands, the next production-readiness layer should focus on monitored
testnet drills, alert thresholds, incident shutdown controls, and external audit
artifact enforcement for the exact deployed escrow source.
