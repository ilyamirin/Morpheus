#pragma version ^0.4.1

interface ERC20:
    def transfer(to: address, amount: uint256) -> bool: nonpayable
    def transferFrom(owner: address, to: address, amount: uint256) -> bool: nonpayable

struct Escrow:
    status: uint8
    token: address
    amount: uint256
    seller: address
    buyer: address
    arbiter: address
    deposited_at: uint256

admin: public(address)
paused: public(bool)
seller_operators: public(HashMap[address, bool])
arbiters: public(HashMap[address, bool])
allowed_tokens: public(HashMap[address, bool])
escrows: HashMap[bytes32, Escrow]

event EscrowDeposited:
    order_hash: indexed(bytes32)
    buyer: indexed(address)
    seller: indexed(address)
    token: address
    amount: uint256

event EscrowReleased:
    order_hash: indexed(bytes32)
    seller: indexed(address)
    token: address
    amount: uint256

event EscrowRefunded:
    order_hash: indexed(bytes32)
    buyer: indexed(address)
    token: address
    amount: uint256

event EscrowPartiallyRefunded:
    order_hash: indexed(bytes32)
    buyer: indexed(address)
    seller: indexed(address)
    token: address
    buyer_amount: uint256
    seller_amount: uint256

@deploy
def __init__(_admin: address):
    assert _admin != empty(address), "ZERO_ADMIN"
    self.admin = _admin

@internal
def _only_admin():
    assert msg.sender == self.admin, "NOT_ADMIN"

@external
def set_allowed_token(token: address, allowed: bool):
    self._only_admin()
    assert token != empty(address), "ZERO_TOKEN"
    self.allowed_tokens[token] = allowed

@external
def set_seller_operator(operator: address, allowed: bool):
    self._only_admin()
    assert operator != empty(address), "ZERO_OPERATOR"
    self.seller_operators[operator] = allowed

@external
def set_arbiter(arbiter: address, allowed: bool):
    self._only_admin()
    assert arbiter != empty(address), "ZERO_ARBITER"
    self.arbiters[arbiter] = allowed

@view
@external
def escrow_status(order_hash: bytes32) -> uint8:
    return self.escrows[order_hash].status

@external
@nonreentrant
def deposit(order_hash: bytes32, token: address, amount: uint256, seller: address, buyer: address, arbiter: address):
    assert not self.paused, "PAUSED"
    assert order_hash != empty(bytes32), "ZERO_ORDER"
    assert self.allowed_tokens[token], "TOKEN"
    assert amount > 0, "AMOUNT"
    assert seller != empty(address), "ZERO_SELLER"
    assert buyer != empty(address), "ZERO_BUYER"
    assert arbiter != empty(address), "ZERO_ARBITER"
    assert msg.sender == buyer, "NOT_BUYER"
    assert self.escrows[order_hash].status == 0, "DUPLICATE"

    self.escrows[order_hash] = Escrow(
        status=1,
        token=token,
        amount=amount,
        seller=seller,
        buyer=buyer,
        arbiter=arbiter,
        deposited_at=block.timestamp,
    )
    assert extcall ERC20(token).transferFrom(buyer, self, amount), "TRANSFER_FROM"
    log EscrowDeposited(order_hash=order_hash, buyer=buyer, seller=seller, token=token, amount=amount)

@external
@nonreentrant
def release(order_hash: bytes32):
    assert not self.paused, "PAUSED"
    assert self.seller_operators[msg.sender], "NOT_OPERATOR"
    escrow: Escrow = self.escrows[order_hash]
    assert escrow.status == 1, "NOT_DEPOSITED"

    self.escrows[order_hash].status = 2
    assert extcall ERC20(escrow.token).transfer(escrow.seller, escrow.amount), "TRANSFER"
    log EscrowReleased(order_hash=order_hash, seller=escrow.seller, token=escrow.token, amount=escrow.amount)

@external
@nonreentrant
def refund(order_hash: bytes32):
    assert not self.paused, "PAUSED"
    escrow: Escrow = self.escrows[order_hash]
    assert self.arbiters[msg.sender] or msg.sender == escrow.arbiter, "NOT_ARBITER"
    assert escrow.status == 1, "NOT_DEPOSITED"

    self.escrows[order_hash].status = 3
    assert extcall ERC20(escrow.token).transfer(escrow.buyer, escrow.amount), "TRANSFER"
    log EscrowRefunded(order_hash=order_hash, buyer=escrow.buyer, token=escrow.token, amount=escrow.amount)

@external
@nonreentrant
def partial_refund(order_hash: bytes32, buyer_amount: uint256):
    assert not self.paused, "PAUSED"
    escrow: Escrow = self.escrows[order_hash]
    assert self.arbiters[msg.sender] or msg.sender == escrow.arbiter, "NOT_ARBITER"
    assert escrow.status == 1, "NOT_DEPOSITED"
    assert buyer_amount > 0, "ZERO_REFUND"
    assert buyer_amount < escrow.amount, "REFUND_TOO_LARGE"

    seller_amount: uint256 = escrow.amount - buyer_amount
    self.escrows[order_hash].status = 4
    assert extcall ERC20(escrow.token).transfer(escrow.buyer, buyer_amount), "BUYER_TRANSFER"
    assert extcall ERC20(escrow.token).transfer(escrow.seller, seller_amount), "SELLER_TRANSFER"
    log EscrowPartiallyRefunded(
        order_hash=order_hash,
        buyer=escrow.buyer,
        seller=escrow.seller,
        token=escrow.token,
        buyer_amount=buyer_amount,
        seller_amount=seller_amount,
    )
