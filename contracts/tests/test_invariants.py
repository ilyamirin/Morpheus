import boa
import pytest

from tests.test_escrow import ADMIN, ARBITER, BUYER, OPERATOR, ORDER_HASH, SELLER


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
    token.approve(contract.address, 250_000, sender=BUYER)
    contract.deposit(ORDER_HASH, token.address, 250_000, SELLER, BUYER, ARBITER, sender=BUYER)
    return contract


def test_terminal_release_cannot_refund(escrow):
    escrow.release(ORDER_HASH, sender=OPERATOR)
    with boa.reverts("NOT_DEPOSITED"):
        escrow.refund(ORDER_HASH, sender=ARBITER)


def test_terminal_refund_cannot_release(escrow):
    escrow.refund(ORDER_HASH, sender=ARBITER)
    with boa.reverts("NOT_DEPOSITED"):
        escrow.release(ORDER_HASH, sender=OPERATOR)
