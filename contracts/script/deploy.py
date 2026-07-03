import json
import os
from pathlib import Path

import boa


DEPLOYER = os.environ.get("MORPHEUS_EVM_DEPLOYER")
CHAIN_ID = int(os.environ.get("MORPHEUS_EVM_CHAIN_ID", "31337"))
OUT = Path(os.environ.get("MORPHEUS_EVM_DEPLOYMENT_OUT", "deployments/local.json"))


def configure_deployer():
    if not DEPLOYER:
        return
    from eth_account import Account

    boa.env.add_account(Account.from_key(DEPLOYER))


def latest_block_number() -> int:
    try:
        block = boa.env.get_block("latest")
        if hasattr(block, "number"):
            return int(block.number)
        if isinstance(block, dict) and "number" in block:
            return int(block["number"])
    except Exception:
        pass
    try:
        return int(boa.env.evm.patch.block_number)
    except Exception:
        return 0


def main():
    configure_deployer()
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
        "deploy_block": latest_block_number(),
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload, indent=2, sort_keys=True))


def moccasin_main():
    main()


if __name__ == "__main__":
    main()
