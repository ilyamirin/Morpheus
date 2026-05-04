# Federated Marketplace Protocol

Reference validator and conformance suite for `io.marketplace` v0.1, a strict federated digital marketplace protocol over Matrix.

## Current Scope

This package validates protocol events and state transitions. It does not run a Matrix Application Service yet.

For order-room replay validation, use `validateOrderEventSequence`. It combines the transition graph with payload checks for `customer.bound`, `order.created` terms, payment intent/capture/refund references, entitlement references, and dispute references. `OrderStateMachine` is intentionally only the capture-policy-agnostic transition graph and is not sufficient by itself for strict protocol acceptance.

## Commands

```bash
npm install
npm run check
```

## Documents

- Spec: `docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md`
- Plan: `docs/superpowers/plans/2026-05-04-federated-marketplace-reference-validator.md`
