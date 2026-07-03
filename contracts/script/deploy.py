import json
import os
from pathlib import Path

import boa


DEPLOYER = os.environ.get("MORPHEUS_EVM_DEPLOYER")
CHAIN_ID = int(os.environ.get("MORPHEUS_EVM_CHAIN_ID", "31337"))
OUT = Path(os.environ.get("MORPHEUS_EVM_DEPLOYMENT_OUT", "deployments/local.json"))


def main():
    if DEPLOYER:
        boa.env.add_account(DEPLOYER)

    admin = boa.env.eoa
    token = boa.load("src/MockERC20.vy", "Mock USDC", "mUSDC", 6)
    escrow = boa.load("src/MorpheusEscrow.vy", admin)
    escrow.set_allowed_token(token.address, True)

    payload = {
        "chain_id": CHAIN_ID,
        "admin": admin,
        "mock_erc20": token.address,
        "escrow_contract": escrow.address,
        "default_token": token.address,
        "deploy_block": boa.env.evm.patch.block_number,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
