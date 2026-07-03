# Foundry Helpers

Foundry is supporting tooling for the Vyper escrow workspace.

- `anvil --chain-id 31337` runs a local EVM JSON-RPC node.
- `cast call` and `cast send` are used for local smoke checks.
- Vyper/Moccasin remain the source of truth for compiling and testing contracts.

## Smoke Commands

```sh
anvil --chain-id 31337
cd contracts
mox run script/deploy.py --network local
cast code "$(jq -r .escrow_contract deployments/local.json)" --rpc-url http://127.0.0.1:8545
```

The `cast code` command should return non-empty bytecode for the deployed escrow contract.
