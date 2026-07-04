#pragma version ^0.4.1

name: public(String[64])
symbol: public(String[16])
decimals: public(uint8)
totalSupply: public(uint256)
balanceOf: public(HashMap[address, uint256])
allowance: public(HashMap[address, HashMap[address, uint256]])

event Transfer:
    sender: indexed(address)
    receiver: indexed(address)
    value: uint256

event Approval:
    owner: indexed(address)
    spender: indexed(address)
    value: uint256

@deploy
def __init__(_name: String[64], _symbol: String[16], _decimals: uint8):
    self.name = _name
    self.symbol = _symbol
    self.decimals = _decimals

@external
def mint(to: address, amount: uint256):
    assert to != empty(address), "ZERO_TO"
    self.balanceOf[to] += amount
    self.totalSupply += amount
    log Transfer(empty(address), to, amount)

@external
def approve(spender: address, amount: uint256) -> bool:
    assert spender != empty(address), "ZERO_SPENDER"
    self.allowance[msg.sender][spender] = amount
    log Approval(msg.sender, spender, amount)
    return True

@external
def transfer(to: address, amount: uint256) -> bool:
    assert to != empty(address), "ZERO_TO"
    assert self.balanceOf[msg.sender] >= amount, "BALANCE"
    self.balanceOf[msg.sender] -= amount
    self.balanceOf[to] += amount
    log Transfer(msg.sender, to, amount)
    return True

@external
def transferFrom(owner: address, to: address, amount: uint256) -> bool:
    assert owner != empty(address), "ZERO_OWNER"
    assert to != empty(address), "ZERO_TO"
    assert self.balanceOf[owner] >= amount, "BALANCE"
    assert self.allowance[owner][msg.sender] >= amount, "ALLOWANCE"
    self.allowance[owner][msg.sender] -= amount
    self.balanceOf[owner] -= amount
    self.balanceOf[to] += amount
    log Transfer(owner, to, amount)
    return True
