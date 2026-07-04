# Foundry Helpers

Foundry is supporting tooling for the Vyper escrow workspace.

- `anvil --chain-id 31337` runs a local EVM JSON-RPC node.
- `cast call` and `cast send` are used for local smoke checks.
- Vyper/Moccasin remain the source of truth for compiling and testing contracts.

Required tools for the full local flow:

- Foundry: `anvil`, `cast`
- Moccasin: `mox`
- Node/npm for the viem UI bundle
- Docker Compose for local Postgres

## Smoke Commands

```sh
anvil --chain-id 31337
cd contracts
mox run script/deploy.py --network local
cast code "$(jq -r .escrow_contract deployments/local.json)" --rpc-url http://127.0.0.1:8545
```

The `cast code` command should return non-empty bytecode for the deployed escrow contract.

## Full E2E

```sh
make e2e-evm-escrow
```

The E2E runner starts Anvil, runs Moccasin tests, deploys the Vyper contracts,
starts Postgres, launches `morpheus-server`, submits the Morpheus order/payment
flow, sends `mint`, `approve`, `deposit`, and `release` transactions with Cast,
and waits for the embedded watcher to project authorized and captured payment
states.
