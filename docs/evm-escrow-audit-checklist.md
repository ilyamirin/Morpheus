# EVM Escrow External Audit Checklist

Morpheus must not enable production funds for the Vyper escrow contract until an
external audit report exists for the exact reviewed source, commit, compiler
settings, and deployed bytecode.

## Required Audit Scope

- `contracts/src/MorpheusEscrow.vy`
- `contracts/src/MockERC20.vy` only as a local/test helper, not as a production
  token implementation.
- Deployment script behavior in `contracts/script/deploy.py`.
- Event schema compatibility with the Morpheus watcher:
  `EscrowDeposited`, `EscrowReleased`, `EscrowRefunded`, and
  `EscrowPartialRefunded`.
- Admin authority, allowed token policy, terminal state transitions, partial
  refund arithmetic, ERC-20 transfer behavior, and replay/dedup assumptions.

## Required Report Fields

- Auditor identity and review date.
- Reviewed git commit, Vyper version, Moccasin/Titanoboa version, and bytecode
  hash or deployed contract address.
- Explicit list of in-scope and out-of-scope files.
- Findings with severity.
- Remediation status for each finding.
- Accepted risks signed off by the operator before any production funds.

## Production Gate

Set `MORPHEUS_EVM_AUDIT_REPORT` to the external report path and run:

```bash
make audit-evm-escrow
```

The checker is intentionally strict and fails when no external report is
provided. A local checklist, internal review, or passing unit test suite is not a
substitute for the external audit gate.
