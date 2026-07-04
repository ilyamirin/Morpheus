# Payment Design Manifesto

Morpheus payments are designed for federated classifieds and marketplaces where
buyers and sellers often do not know each other, legal recovery is weak, internet
access may be unreliable, and transaction fees can decide whether the product is
usable at all. The payment system must therefore be cheap, understandable,
auditable, and resilient before it is maximally decentralized.

## Core Position

Morpheus separates marketplace truth from custody truth.

- Morpheus and Matrix are the source of truth for marketplace lifecycle,
  negotiation, evidence, projections, and federation.
- The payment rail is the source of truth for custody and settlement.
- For EVM escrow, the contract holds ERC-20 funds until release, refund, partial
  refund, or arbitration outcome.
- The bridge between the two worlds is compact, deterministic evidence: chain id,
  contract, token, amount, participants, order hash, transaction hash, block data,
  and log identity.

We do not put product data, delivery artifacts, private chat, identity documents,
phone numbers, addresses, credentials, or secrets on-chain. We also do not treat a
wallet-submitted transaction hash as final payment state. Payment state changes
only after Morpheus verifies finalized payment-rail evidence.

## Design Principles

### Escrow Before Trust

For unfamiliar counterparties, the default high-trust primitive is not reputation
alone. It is escrow. Funds should be locked before fulfillment and released only
after completion, timeout, or an explicit dispute path.

Escrow is not a replacement for trust. It is the minimum shared mechanism that
makes trust cheaper to build.

### Disputes Are Product Flow, Not Edge Cases

Classified and marketplace transactions regularly fail in partial ways: item not
as described, late delivery, missing component, damaged goods, or buyer no-show.
The system must support explicit dispute states, arbiter decisions, full refunds,
and partial refunds.

The dispute model should be visible before payment. Buyers and sellers should
know who can arbitrate, what evidence is accepted, how long each side has to
respond, and what outcomes are possible.

### Keep On-Chain Load Minimal

The chain should not become the database. The contract should only hold custody
state and compact settlement facts. Large evidence, media, messages, delivery
proofs, and seller/buyer metadata belong off-chain in Morpheus-controlled storage
or Matrix-backed protocol events, referenced by hashes where useful.

This keeps costs low, protects privacy, and makes the system viable for low-value
orders.

### Design For Developing Markets

Payment features must assume:

- small ticket sizes;
- high sensitivity to gas fees and FX spread;
- mobile-first users;
- unstable networks and delayed confirmations;
- limited access to formal legal recovery;
- mixed digital and physical delivery;
- local operators and arbiters with uneven operational maturity.

This leads to a conservative rollout strategy: low limits first, cheap networks
first, stablecoins first, explicit runbooks first, and higher-risk mechanisms only
after operational history.

### Privacy Is A Payment Feature

Transparent settlement rails can leak commercial behavior. Morpheus should avoid
exposing the item, address, real identity, phone number, negotiation details, or
delivery artifacts through public payment evidence. Order hashes and compact
event evidence are preferable to descriptive on-chain metadata.

### Incentives Should Be Configurable

Seller bonds, buyer deposits, penalties, and arbiter fees can improve incentives,
but they also raise friction. Morpheus should support these as policy tools, not
force them on every market.

Recommended default:

1. No extra bond for low-value, low-risk orders.
2. Optional seller bond for new sellers, high-value goods, preorders, or risky
   categories.
3. Optional buyer deposit for high no-show risk or costly physical handling.
4. Clear caps so penalties never surprise users.

### Operational Safety Beats Clever Contracts

Smart contracts that hold funds must be treated as production infrastructure.
The minimum bar is tests, invariant checks, manual review, external audit before
real funds, monitored RPC, replay procedures, order-intake shutdown runbooks,
and network-specific confirmation policy.

## What We Build Into Morpheus

### Now

- EVM ERC-20 escrow adapter with verified finalized-log watcher.
- Payment intents that pin token, amount, chain, contract, participants, and
  order hash.
- Release, refund, and partial refund lifecycle mapping.
- Durable deduplication by `(chain_id, tx_hash, log_index)`.
- Admin status and replay tools for watcher operation.
- The EVM escrow payment policy is enforced for configured order limits and
  exposed to buyer, seller, and operator surfaces.
- Timeout and fee metadata are included in payment confirmation evidence so users
  can understand the payment window before signing.
- Documentation for local Anvil, testnet, and production L2 rollout.

### Next

- Category-specific and token-specific payment policy profiles.
- Auto-release and auto-refund rules driven by the configured escrow timeout
  model.
- Arbitration policy document: accepted evidence, arbiter authority, response
  windows, fees, and finality of decisions.
- Risk-tiered limits for new sellers, high-risk categories, and unaudited
  networks.
- Privacy review for every new payment evidence field.

### Later

- Optional seller/buyer bonds for high-risk orders.
- Reputation weighting from completed escrow outcomes.
- Multi-arbiter or local-community arbitration models.
- Gas sponsorship or relayer support where it can be done without custody risk.
- Additional payment rails if they satisfy the same evidence, privacy, cost, and
  operational requirements.

## What We Avoid

- Putting private marketplace data on-chain.
- Treating transaction submission as payment finality.
- Supporting expensive networks for low-value orders by default.
- Launching production funds before audit and incident drills.
- Making bonds mandatory for all orders.
- Embedding private keys in Morpheus server runtime.
- Designing only for ideal users with strong wallets, stable internet, and easy
  access to legal enforcement.

## Research Basis

The payment design draws from the following research directions:

- Incentive-compatible escrow for decentralized commerce:
  [arXiv:2008.10326](https://arxiv.org/abs/2008.10326)
- Dual-deposit escrow for buyer/seller cooperation:
  [arXiv:1806.08379](https://arxiv.org/abs/1806.08379)
- Constant-load blockchain data marketplace design:
  [arXiv:2003.11424](https://arxiv.org/abs/2003.11424)
- Privacy-preserving peer-to-peer marketplace settlement:
  [arXiv:1905.07940](https://arxiv.org/abs/1905.07940)
- Low-cost, scalable IoT data marketplace architecture:
  [arXiv:2210.04733](https://arxiv.org/abs/2210.04733)
- Ethereum marketplace prototyping and private-chain validation:
  [arXiv:2401.00141](https://arxiv.org/abs/2401.00141)
- Ethereum smart-contract vulnerability analysis:
  [arXiv:1908.08605](https://arxiv.org/abs/1908.08605)

The practical conclusion is simple: Morpheus should use smart contracts for
custody and settlement, not as a replacement for product state, evidence,
operator policy, or human-readable dispute processes.
