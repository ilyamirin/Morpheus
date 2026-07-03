import boa
import pytest


BUYER = boa.env.generate_address("buyer")
SELLER = boa.env.generate_address("seller")
ARBITER = boa.env.generate_address("arbiter")
OPERATOR = boa.env.generate_address("operator")
ADMIN = boa.env.generate_address("admin")
ORDER_HASH = b"\x11" * 32


@pytest.fixture
def token():
    contract = boa.load("src/MockERC20.vy", "Mock USDC", "mUSDC", 6)
    contract.mint(BUYER, 1_000_000, sender=ADMIN)
    return contract


@pytest.fixture
def escrow(token):
    contract = boa.load("src/MorpheusEscrow.vy", ADMIN)
    contract.set_allowed_token(token.address, True, sender=ADMIN)
    contract.set_seller_operator(OPERATOR, True, sender=ADMIN)
    contract.set_arbiter(ARBITER, True, sender=ADMIN)
    return contract


def test_deposit_records_escrow_and_transfers_tokens(token, escrow):
    token.approve(escrow.address, 250_000, sender=BUYER)

    escrow.deposit(ORDER_HASH, token.address, 250_000, SELLER, BUYER, ARBITER, sender=BUYER)

    assert escrow.escrow_status(ORDER_HASH) == 1
    assert token.balanceOf(escrow.address) == 250_000
