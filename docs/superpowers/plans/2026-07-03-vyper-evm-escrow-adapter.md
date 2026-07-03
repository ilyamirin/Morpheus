# Vyper EVM Escrow Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `evm_escrow` payment adapter using a Vyper escrow contract, Moccasin/Titanoboa contract tests, Foundry Anvil/Cast tooling, and a Rust watcher that maps finalized escrow logs into Morpheus payment events.

**Architecture:** Keep Morpheus/Matrix as the marketplace lifecycle source of truth and the EVM contract as the token custody source of truth. Add an isolated `contracts/` workspace for Vyper artifacts, extend Morpheus config and store with adapter-specific state, then add a server-side adapter/watcher that verifies EVM logs before publishing existing `io.marketplace.payment.*` events.

**Tech Stack:** Rust, Axum, SQLx, Tokio, Vyper, Moccasin, Titanoboa, Foundry Anvil/Cast, ERC-20, JSON-RPC.

---

## File Structure

Create:

- `contracts/moccasin.toml`: Vyper contract workspace config.
- `contracts/src/MorpheusEscrow.vy`: ERC-20 escrow contract.
- `contracts/src/MockERC20.vy`: local test token.
- `contracts/tests/test_escrow.py`: contract behavior tests.
- `contracts/tests/test_invariants.py`: state transition and terminal-state tests.
- `contracts/script/deploy.py`: local/testnet deployment script that writes deployment JSON.
- `contracts/foundry/foundry.toml`: Anvil/Cast helper config.
- `contracts/foundry/README.md`: local contract tooling instructions.
- `contracts/deployments/.gitkeep`: keeps deployment directory present.
- `crates/morpheus-server/src/evm_escrow.rs`: order hash, intent metadata, log decoding, watcher orchestration.
- `crates/morpheus-server/tests/evm_escrow_adapter.rs`: Rust unit/route tests for adapter behavior.
- `docs/protocol-evm-escrow.md`: operator-facing protocol note for the adapter.

Modify:

- `Cargo.toml`: add workspace dependencies needed for EVM hashing/hex handling if not already present.
- `crates/morpheus-config/src/lib.rs`: add optional `[payments.evm_escrow]` config.
- `crates/morpheus-store/src/lib.rs`: add EVM log checkpoint and idempotency store methods.
- `migrations/sqlite/0001_initial.sql`: add EVM watcher tables.
- `migrations/postgres/0001_initial.sql`: add EVM watcher tables.
- `crates/morpheus-api/src/lib.rs`: add adapter-specific request/response DTOs only where HTTP boundaries need them.
- `crates/morpheus-server/src/lib.rs`: wire module, routes, and watcher hooks.
- `crates/morpheus-server/ui/assets/app.js`: hide mock payment controls for `evm_escrow`, show wallet payment state.
- `config/local.toml`: add disabled example config for `evm_escrow`.
- `README.md`: mention local contract workspace and high-level commands.

Do not change `morpheus-core` protocol validation unless a current event body shape rejects the adapter evidence. Prefer putting adapter details inside existing `confirmation` and `evidence` objects.

---

### Task 1: Add Vyper Contract Workspace Skeleton

**Files:**
- Create: `contracts/moccasin.toml`
- Create: `contracts/src/MorpheusEscrow.vy`
- Create: `contracts/src/MockERC20.vy`
- Create: `contracts/tests/test_escrow.py`
- Create: `contracts/deployments/.gitkeep`
- Create: `contracts/foundry/foundry.toml`
- Create: `contracts/foundry/README.md`

- [ ] **Step 1: Write the first failing contract test**

Create `contracts/tests/test_escrow.py` with:

```python
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
    contract = boa.load("contracts/src/MockERC20.vy", "Mock USDC", "mUSDC", 6)
    contract.mint(BUYER, 1_000_000, sender=ADMIN)
    return contract


@pytest.fixture
def escrow(token):
    contract = boa.load("contracts/src/MorpheusEscrow.vy", ADMIN)
    contract.set_allowed_token(token.address, True, sender=ADMIN)
    contract.set_seller_operator(OPERATOR, True, sender=ADMIN)
    contract.set_arbiter(ARBITER, True, sender=ADMIN)
    return contract


def test_deposit_records_escrow_and_transfers_tokens(token, escrow):
    token.approve(escrow.address, 250_000, sender=BUYER)

    escrow.deposit(ORDER_HASH, token.address, 250_000, SELLER, BUYER, ARBITER, sender=BUYER)

    assert escrow.escrow_status(ORDER_HASH) == 1
    assert token.balanceOf(escrow.address) == 250_000
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cd contracts
mox test tests/test_escrow.py -k deposit_records -q
```

Expected: FAIL because `contracts/moccasin.toml`, `MorpheusEscrow.vy`, and `MockERC20.vy` do not exist yet.

- [ ] **Step 3: Add Moccasin config**

Create `contracts/moccasin.toml`:

```toml
[project]
src = "src"
out = "abi"

[networks.local]
url = "http://127.0.0.1:8545"
chain_id = 31337
```

- [ ] **Step 4: Add minimal MockERC20**

Create `contracts/src/MockERC20.vy`:

```python
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
```

- [ ] **Step 5: Add minimal MorpheusEscrow**

Create `contracts/src/MorpheusEscrow.vy`:

```python
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

    self.escrows[order_hash] = Escrow({
        status: 1,
        token: token,
        amount: amount,
        seller: seller,
        buyer: buyer,
        arbiter: arbiter,
        deposited_at: block.timestamp,
    })
    assert extcall ERC20(token).transferFrom(buyer, self, amount), "TRANSFER_FROM"
    log EscrowDeposited(order_hash, buyer, seller, token, amount)
```

- [ ] **Step 6: Add Foundry helper config**

Create `contracts/foundry/foundry.toml`:

```toml
[profile.default]
src = "../src"
out = "../abi"
libs = []
evm_version = "cancun"
```

Create `contracts/foundry/README.md`:

```markdown
# Foundry Helpers

Foundry is supporting tooling for the Vyper escrow workspace.

- `anvil --chain-id 31337` runs a local EVM JSON-RPC node.
- `cast call` and `cast send` are used for local smoke checks.
- Vyper/Moccasin remain the source of truth for compiling and testing contracts.
```

Create empty file `contracts/deployments/.gitkeep`.

- [ ] **Step 7: Run test to verify it passes**

Run:

```bash
cd contracts
mox test tests/test_escrow.py -k deposit_records -q
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add contracts
git commit -m "Add Vyper escrow contract workspace"
```

---

### Task 2: Complete Vyper Escrow State Transitions

**Files:**
- Modify: `contracts/src/MorpheusEscrow.vy`
- Modify: `contracts/tests/test_escrow.py`
- Create: `contracts/tests/test_invariants.py`

- [ ] **Step 1: Add failing release/refund tests**

Append to `contracts/tests/test_escrow.py`:

```python
def _deposit(token, escrow):
    token.approve(escrow.address, 250_000, sender=BUYER)
    escrow.deposit(ORDER_HASH, token.address, 250_000, SELLER, BUYER, ARBITER, sender=BUYER)


def test_release_transfers_tokens_to_seller(token, escrow):
    _deposit(token, escrow)

    escrow.release(ORDER_HASH, sender=OPERATOR)

    assert escrow.escrow_status(ORDER_HASH) == 2
    assert token.balanceOf(SELLER) == 250_000


def test_refund_transfers_tokens_to_buyer(token, escrow):
    _deposit(token, escrow)

    escrow.refund(ORDER_HASH, sender=ARBITER)

    assert escrow.escrow_status(ORDER_HASH) == 3
    assert token.balanceOf(BUYER) == 1_000_000


def test_partial_refund_splits_tokens(token, escrow):
    _deposit(token, escrow)

    escrow.partial_refund(ORDER_HASH, 100_000, sender=ARBITER)

    assert escrow.escrow_status(ORDER_HASH) == 4
    assert token.balanceOf(BUYER) == 850_000
    assert token.balanceOf(SELLER) == 150_000
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd contracts
mox test tests/test_escrow.py -k "release or refund" -q
```

Expected: FAIL because `release`, `refund`, and `partial_refund` are not implemented.

- [ ] **Step 3: Implement terminal transitions**

Append this event and functions to `contracts/src/MorpheusEscrow.vy`:

```python
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

@external
@nonreentrant
def release(order_hash: bytes32):
    assert not self.paused, "PAUSED"
    assert self.seller_operators[msg.sender], "NOT_OPERATOR"
    escrow: Escrow = self.escrows[order_hash]
    assert escrow.status == 1, "NOT_DEPOSITED"

    self.escrows[order_hash].status = 2
    assert extcall ERC20(escrow.token).transfer(escrow.seller, escrow.amount), "TRANSFER"
    log EscrowReleased(order_hash, escrow.seller, escrow.token, escrow.amount)

@external
@nonreentrant
def refund(order_hash: bytes32):
    assert not self.paused, "PAUSED"
    escrow: Escrow = self.escrows[order_hash]
    assert self.arbiters[msg.sender] or msg.sender == escrow.arbiter, "NOT_ARBITER"
    assert escrow.status == 1, "NOT_DEPOSITED"

    self.escrows[order_hash].status = 3
    assert extcall ERC20(escrow.token).transfer(escrow.buyer, escrow.amount), "TRANSFER"
    log EscrowRefunded(order_hash, escrow.buyer, escrow.token, escrow.amount)

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
    log EscrowPartiallyRefunded(order_hash, escrow.buyer, escrow.seller, escrow.token, buyer_amount, seller_amount)
```

- [ ] **Step 4: Add terminal-state invariant tests**

Create `contracts/tests/test_invariants.py`:

```python
import boa
import pytest

from tests.test_escrow import ADMIN, ARBITER, BUYER, OPERATOR, ORDER_HASH, SELLER


@pytest.fixture
def token():
    contract = boa.load("contracts/src/MockERC20.vy", "Mock USDC", "mUSDC", 6)
    contract.mint(BUYER, 1_000_000, sender=ADMIN)
    return contract


@pytest.fixture
def escrow(token):
    contract = boa.load("contracts/src/MorpheusEscrow.vy", ADMIN)
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
```

- [ ] **Step 5: Run contract tests**

Run:

```bash
cd contracts
mox test -q
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add contracts/src/MorpheusEscrow.vy contracts/tests
git commit -m "Implement Vyper escrow transitions"
```

---

### Task 3: Add Deployment Script And Foundry Smoke Path

**Files:**
- Create: `contracts/script/deploy.py`
- Modify: `contracts/foundry/README.md`

- [ ] **Step 1: Write deployment script**

Create `contracts/script/deploy.py`:

```python
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
```

- [ ] **Step 2: Run deployment against local Anvil**

Start Anvil in a separate terminal:

```bash
anvil --chain-id 31337
```

Run:

```bash
cd contracts
mox run script/deploy.py --network local
```

Expected: `contracts/deployments/local.json` contains `chain_id`, `mock_erc20`, `escrow_contract`, `default_token`, and `deploy_block`.

- [ ] **Step 3: Add smoke instructions**

Append to `contracts/foundry/README.md`:

```text
## Smoke Commands

anvil --chain-id 31337
cd contracts
mox run script/deploy.py --network local
cast code "$(jq -r .escrow_contract deployments/local.json)" --rpc-url http://127.0.0.1:8545

The `cast code` command should return non-empty bytecode for the deployed escrow contract.
```

- [ ] **Step 4: Commit**

```bash
git add contracts/script/deploy.py contracts/foundry/README.md contracts/deployments/.gitkeep
git commit -m "Add Vyper escrow deployment tooling"
```

---

### Task 4: Add EVM Escrow Config

**Files:**
- Modify: `crates/morpheus-config/src/lib.rs`
- Modify: `config/local.toml`
- Test: `crates/morpheus-config/src/lib.rs`

- [ ] **Step 1: Add failing config test**

Append inside `#[cfg(test)] mod tests` in `crates/morpheus-config/src/lib.rs`:

```rust
    #[test]
    fn validates_evm_escrow_config_when_enabled() {
        let mut config = valid_config();
        config.instance.payment_adapters = vec!["mock".into(), "evm_escrow".into()];
        config.payments = Some(PaymentsConfig {
            evm_escrow: Some(EvmEscrowConfig {
                enabled: true,
                chain_id: 31337,
                rpc_url_env: "MORPHEUS_EVM_RPC_URL".into(),
                escrow_contract: "0x0000000000000000000000000000000000000001".into(),
                default_token: "0x0000000000000000000000000000000000000002".into(),
                confirmations: 1,
                poll_interval_secs: 2,
                deployments_path: Some("contracts/deployments/local.json".into()),
                tokens: vec![EvmEscrowTokenConfig {
                    symbol: "USDC".into(),
                    contract: "0x0000000000000000000000000000000000000002".into(),
                    decimals: 6,
                    currency: "USDC".into(),
                }],
            }),
        });

        assert!(validate_config(&config).is_ok());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-config validates_evm_escrow_config_when_enabled
```

Expected: FAIL because `MorpheusConfig.payments`, `PaymentsConfig`, `EvmEscrowConfig`, and `EvmEscrowTokenConfig` are undefined.

- [ ] **Step 3: Add config structs and validation**

Modify `MorpheusConfig`:

```rust
pub struct MorpheusConfig {
    pub instance: InstanceConfig,
    pub appservice: AppServiceConfig,
    pub database: DatabaseConfig,
    pub admin: AdminConfig,
    pub auth: AuthConfig,
    pub allowlist: Option<AllowlistConfig>,
    pub payments: Option<PaymentsConfig>,
}
```

Add structs:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentsConfig {
    pub evm_escrow: Option<EvmEscrowConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowConfig {
    pub enabled: bool,
    pub chain_id: u64,
    pub rpc_url_env: String,
    pub escrow_contract: String,
    pub default_token: String,
    pub confirmations: u64,
    pub poll_interval_secs: u64,
    pub deployments_path: Option<String>,
    pub tokens: Vec<EvmEscrowTokenConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowTokenConfig {
    pub symbol: String,
    pub contract: String,
    pub decimals: u8,
    pub currency: String,
}
```

Add helper:

```rust
fn is_evm_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].chars().all(|ch| ch.is_ascii_hexdigit())
}
```

Add to `validate_config`:

```rust
    if let Some(payments) = &config.payments
        && let Some(evm) = &payments.evm_escrow
        && evm.enabled
    {
        anyhow::ensure!(
            config
                .instance
                .payment_adapters
                .iter()
                .any(|adapter| adapter == "evm_escrow"),
            "evm_escrow payment config requires instance.payment_adapters to include evm_escrow"
        );
        anyhow::ensure!(evm.chain_id > 0, "evm_escrow chain_id is required");
        anyhow::ensure!(!evm.rpc_url_env.is_empty(), "evm_escrow rpc_url_env is required");
        anyhow::ensure!(is_evm_address(&evm.escrow_contract), "evm_escrow escrow_contract must be an EVM address");
        anyhow::ensure!(is_evm_address(&evm.default_token), "evm_escrow default_token must be an EVM address");
        anyhow::ensure!(evm.confirmations > 0, "evm_escrow confirmations must be positive");
        anyhow::ensure!(evm.poll_interval_secs > 0, "evm_escrow poll_interval_secs must be positive");
        anyhow::ensure!(!evm.tokens.is_empty(), "evm_escrow tokens must not be empty");
        anyhow::ensure!(
            evm.tokens.iter().any(|token| token.contract == evm.default_token),
            "evm_escrow default_token must be listed in tokens"
        );
        for token in &evm.tokens {
            anyhow::ensure!(!token.symbol.is_empty(), "evm_escrow token symbol is required");
            anyhow::ensure!(is_evm_address(&token.contract), "evm_escrow token contract must be an EVM address");
            anyhow::ensure!(token.decimals <= 36, "evm_escrow token decimals must be <= 36");
            anyhow::ensure!(!token.currency.is_empty(), "evm_escrow token currency is required");
        }
    }
```

Update `valid_config()` to include `payments: None`.

- [ ] **Step 4: Add disabled local example**

Append to `config/local.toml`:

```toml
[payments.evm_escrow]
enabled = false
chain_id = 31337
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
escrow_contract = "0x0000000000000000000000000000000000000001"
default_token = "0x0000000000000000000000000000000000000002"
confirmations = 1
poll_interval_secs = 2
deployments_path = "contracts/deployments/local.json"

[[payments.evm_escrow.tokens]]
symbol = "USDC"
contract = "0x0000000000000000000000000000000000000002"
decimals = 6
currency = "USDC"
```

- [ ] **Step 5: Run config tests**

Run:

```bash
cargo test -p morpheus-config
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/morpheus-config/src/lib.rs config/local.toml
git commit -m "Add EVM escrow payment configuration"
```

---

### Task 5: Add Order Hash And Intent Metadata

**Files:**
- Create: `crates/morpheus-server/src/evm_escrow.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/evm_escrow_adapter.rs`

- [ ] **Step 1: Add failing order hash test**

Create `crates/morpheus-server/tests/evm_escrow_adapter.rs`:

```rust
use morpheus_server::evm_escrow::{EvmEscrowIntentInput, compute_order_hash};
use serde_json::json;

#[test]
fn order_hash_is_deterministic_for_locked_terms() {
    let input = EvmEscrowIntentInput {
        protocol: "io.marketplace".into(),
        protocol_version: "0.1".into(),
        instance_id: "shop.example".into(),
        order_id: "ord:shop.example:01JORDER".into(),
        customer_id: "customer:shop.example:01JCUSTOMER".into(),
        seller_id: "seller:shop.example:01JSELLER".into(),
        offer_id: "offer:shop.example:01JOFFER".into(),
        offer_revision: 1,
        price: json!({"amount": "25.00", "currency": "USDC"}),
        payment_adapter: "evm_escrow".into(),
        payment_capture_policy: "before_entitlement".into(),
        chain_id: 31337,
        token_contract: "0x0000000000000000000000000000000000000002".into(),
        amount_units: "25000000".into(),
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        seller_evm_address: "0x0000000000000000000000000000000000000003".into(),
        buyer_evm_address: "0x0000000000000000000000000000000000000004".into(),
        arbiter_actor: "arbiter:shop.example:01JARBITER".into(),
        arbiter_evm_address: "0x0000000000000000000000000000000000000005".into(),
    };

    let left = compute_order_hash(&input).unwrap();
    let right = compute_order_hash(&input).unwrap();

    assert_eq!(left, right);
    assert!(left.starts_with("0x"));
    assert_eq!(left.len(), 66);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-server order_hash_is_deterministic_for_locked_terms
```

Expected: FAIL because `morpheus_server::evm_escrow` does not exist.

- [ ] **Step 3: Add module export**

In `crates/morpheus-server/src/lib.rs`, add near the top-level modules:

```rust
pub mod evm_escrow;
```

- [ ] **Step 4: Implement hash helper**

Create `crates/morpheus-server/src/evm_escrow.rs`:

```rust
use morpheus_protocol::{ValidationCode, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowIntentInput {
    pub protocol: String,
    pub protocol_version: String,
    pub instance_id: String,
    pub order_id: String,
    pub customer_id: String,
    pub seller_id: String,
    pub offer_id: String,
    pub offer_revision: i64,
    pub price: Value,
    pub payment_adapter: String,
    pub payment_capture_policy: String,
    pub chain_id: u64,
    pub token_contract: String,
    pub amount_units: String,
    pub escrow_contract: String,
    pub seller_evm_address: String,
    pub buyer_evm_address: String,
    pub arbiter_actor: String,
    pub arbiter_evm_address: String,
}

pub fn compute_order_hash(input: &EvmEscrowIntentInput) -> Result<String, ValidationError> {
    let canonical = json!({
        "protocol": input.protocol,
        "protocol_version": input.protocol_version,
        "instance_id": input.instance_id,
        "order_id": input.order_id,
        "customer_id": input.customer_id,
        "seller_id": input.seller_id,
        "offer_id": input.offer_id,
        "offer_revision": input.offer_revision,
        "price": input.price,
        "payment_adapter": input.payment_adapter,
        "payment_capture_policy": input.payment_capture_policy,
        "chain_id": input.chain_id,
        "token_contract": input.token_contract.to_lowercase(),
        "amount_units": input.amount_units,
        "escrow_contract": input.escrow_contract.to_lowercase(),
        "seller_evm_address": input.seller_evm_address.to_lowercase(),
        "buyer_evm_address": input.buyer_evm_address.to_lowercase(),
        "arbiter_actor": input.arbiter_actor,
        "arbiter_evm_address": input.arbiter_evm_address.to_lowercase(),
    });
    let bytes = serde_json::to_vec(&canonical).map_err(|err| {
        ValidationError::new(
            ValidationCode::MalformedJson,
            format!("failed to serialize evm escrow order hash input: {err}"),
        )
    })?;
    let digest = Sha256::digest(bytes);
    Ok(format!("0x{}", hex::encode(digest)))
}
```

Add `sha2.workspace = true` and `hex.workspace = true` to `crates/morpheus-server/Cargo.toml`.

- [ ] **Step 5: Run test to verify it passes**

Run:

```bash
cargo test -p morpheus-server order_hash_is_deterministic_for_locked_terms
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/morpheus-server/src/lib.rs crates/morpheus-server/src/evm_escrow.rs crates/morpheus-server/tests/evm_escrow_adapter.rs crates/morpheus-server/Cargo.toml
git commit -m "Add EVM escrow order hash helper"
```

---

### Task 6: Add Watcher Persistence

**Files:**
- Modify: `migrations/sqlite/0001_initial.sql`
- Modify: `migrations/postgres/0001_initial.sql`
- Modify: `crates/morpheus-store/src/lib.rs`
- Test: `crates/morpheus-store/tests/store_behavior.rs`

- [ ] **Step 1: Add failing store behavior test**

Append to `crates/morpheus-store/tests/store_behavior.rs`:

```rust
#[tokio::test]
async fn store_deduplicates_evm_escrow_logs() {
    let store = morpheus_store::InMemoryEventStore::default();
    let log = morpheus_store::EvmEscrowLogRecord {
        chain_id: 31337,
        tx_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        log_index: 0,
        block_number: 10,
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
        event_name: "EscrowDeposited".into(),
        payload: serde_json::json!({"amount": "25000000"}),
        emitted_marketplace_event_id: None,
    };

    assert!(store.record_evm_escrow_log(log.clone()).await.unwrap());
    assert!(!store.record_evm_escrow_log(log).await.unwrap());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-store store_deduplicates_evm_escrow_logs
```

Expected: FAIL because `EvmEscrowLogRecord` and `record_evm_escrow_log` do not exist.

- [ ] **Step 3: Add migrations**

Append to both migrations, using `JSONB` for Postgres and `TEXT` for SQLite payload fields:

```sql
CREATE TABLE IF NOT EXISTS evm_escrow_logs (
  chain_id BIGINT NOT NULL,
  tx_hash TEXT NOT NULL,
  log_index BIGINT NOT NULL,
  block_number BIGINT NOT NULL,
  block_hash TEXT NOT NULL,
  escrow_contract TEXT NOT NULL,
  order_hash TEXT NOT NULL,
  event_name TEXT NOT NULL,
  payload JSONB NOT NULL,
  emitted_marketplace_event_id TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (chain_id, tx_hash, log_index)
);

CREATE TABLE IF NOT EXISTS evm_escrow_checkpoints (
  chain_id BIGINT NOT NULL,
  escrow_contract TEXT NOT NULL,
  latest_scanned_block BIGINT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (chain_id, escrow_contract)
);
```

For SQLite, use:

```sql
CREATE TABLE IF NOT EXISTS evm_escrow_logs (
  chain_id INTEGER NOT NULL,
  tx_hash TEXT NOT NULL,
  log_index INTEGER NOT NULL,
  block_number INTEGER NOT NULL,
  block_hash TEXT NOT NULL,
  escrow_contract TEXT NOT NULL,
  order_hash TEXT NOT NULL,
  event_name TEXT NOT NULL,
  payload TEXT NOT NULL,
  emitted_marketplace_event_id TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (chain_id, tx_hash, log_index)
);

CREATE TABLE IF NOT EXISTS evm_escrow_checkpoints (
  chain_id INTEGER NOT NULL,
  escrow_contract TEXT NOT NULL,
  latest_scanned_block INTEGER NOT NULL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (chain_id, escrow_contract)
);
```

- [ ] **Step 4: Extend store trait and in-memory store**

Add record:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvmEscrowLogRecord {
    pub chain_id: i64,
    pub tx_hash: String,
    pub log_index: i64,
    pub block_number: i64,
    pub block_hash: String,
    pub escrow_contract: String,
    pub order_hash: String,
    pub event_name: String,
    pub payload: Value,
    pub emitted_marketplace_event_id: Option<String>,
}
```

Add trait methods:

```rust
    async fn record_evm_escrow_log(
        &self,
        log: EvmEscrowLogRecord,
    ) -> Result<bool, ValidationError>;

    async fn evm_escrow_checkpoint(
        &self,
        chain_id: i64,
        escrow_contract: &str,
    ) -> Result<Option<i64>, ValidationError>;

    async fn set_evm_escrow_checkpoint(
        &self,
        chain_id: i64,
        escrow_contract: &str,
        latest_scanned_block: i64,
    ) -> Result<(), ValidationError>;
```

Implement these in `InMemoryEventStore` with a `HashMap<(i64, String, i64), EvmEscrowLogRecord>` and `HashMap<(i64, String), i64>`.

- [ ] **Step 5: Implement SQL stores**

For SQLite/Postgres, implement `record_evm_escrow_log` as insert-if-absent:

```sql
INSERT INTO evm_escrow_logs (...)
VALUES (...)
ON CONFLICT(chain_id, tx_hash, log_index) DO NOTHING
```

Return `true` if one row was inserted and `false` otherwise.

Implement checkpoint upsert with `ON CONFLICT(chain_id, escrow_contract) DO UPDATE`.

- [ ] **Step 6: Run store tests**

Run:

```bash
cargo test -p morpheus-store store_deduplicates_evm_escrow_logs
cargo test -p morpheus-store
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add migrations/sqlite/0001_initial.sql migrations/postgres/0001_initial.sql crates/morpheus-store/src/lib.rs crates/morpheus-store/tests/store_behavior.rs
git commit -m "Persist EVM escrow watcher state"
```

---

### Task 7: Add Log Decoding And Event Mapping

**Files:**
- Modify: `crates/morpheus-server/src/evm_escrow.rs`
- Test: `crates/morpheus-server/tests/evm_escrow_adapter.rs`

- [ ] **Step 1: Add failing log mapping test**

Append to `crates/morpheus-server/tests/evm_escrow_adapter.rs`:

```rust
use morpheus_server::evm_escrow::{DecodedEscrowLog, map_escrow_log_to_payment_event};

#[test]
fn deposited_log_maps_to_payment_authorized() {
    let log = DecodedEscrowLog {
        event_name: "EscrowDeposited".into(),
        order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
        tx_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        log_index: 0,
        block_number: 10,
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        chain_id: 31337,
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        token: "0x0000000000000000000000000000000000000002".into(),
        amount: "25000000".into(),
        buyer: Some("0x0000000000000000000000000000000000000004".into()),
        seller: Some("0x0000000000000000000000000000000000000003".into()),
        buyer_amount: None,
        seller_amount: None,
    };

    let mapped = map_escrow_log_to_payment_event("ord:shop.example:01JORDER", "pay:shop.example:01JPAY", &log).unwrap();

    assert_eq!(mapped.event_type, "io.marketplace.payment.authorized");
    assert_eq!(mapped.body["order_id"], "ord:shop.example:01JORDER");
    assert_eq!(mapped.body["payment_id"], "pay:shop.example:01JPAY");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-server deposited_log_maps_to_payment_authorized
```

Expected: FAIL because mapping types/functions do not exist.

- [ ] **Step 3: Implement mapping**

Add to `crates/morpheus-server/src/evm_escrow.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedEscrowLog {
    pub event_name: String,
    pub order_hash: String,
    pub tx_hash: String,
    pub log_index: i64,
    pub block_number: i64,
    pub block_hash: String,
    pub chain_id: i64,
    pub escrow_contract: String,
    pub token: String,
    pub amount: String,
    pub buyer: Option<String>,
    pub seller: Option<String>,
    pub buyer_amount: Option<String>,
    pub seller_amount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentEventDraft {
    pub event_type: String,
    pub body: Value,
}

pub fn map_escrow_log_to_payment_event(
    order_id: &str,
    payment_id: &str,
    log: &DecodedEscrowLog,
) -> Result<PaymentEventDraft, ValidationError> {
    let evidence = json!({
        "kind": "evm_escrow_log",
        "chain_id": log.chain_id,
        "escrow_contract": log.escrow_contract,
        "tx_hash": log.tx_hash,
        "log_index": log.log_index,
        "block_number": log.block_number,
        "block_hash": log.block_hash,
        "order_hash": log.order_hash,
        "event_name": log.event_name,
        "token": log.token,
        "amount": log.amount,
        "buyer": log.buyer,
        "seller": log.seller,
        "buyer_amount": log.buyer_amount,
        "seller_amount": log.seller_amount,
    });

    match log.event_name.as_str() {
        "EscrowDeposited" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.authorized".into(),
            body: json!({ "order_id": order_id, "payment_id": payment_id }),
        }),
        "EscrowReleased" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.captured".into(),
            body: json!({
                "order_id": order_id,
                "payment_id": payment_id,
                "adapter": "evm_escrow",
                "amount": log.amount,
                "currency": "USDC",
                "provider_ref": format!("evm:{}:{}:{}", log.chain_id, log.tx_hash, log.log_index),
                "evidence": evidence,
            }),
        }),
        "EscrowRefunded" | "EscrowPartiallyRefunded" => Ok(PaymentEventDraft {
            event_type: "io.marketplace.payment.refunded".into(),
            body: json!({
                "order_id": order_id,
                "payment_id": payment_id,
                "refund_id": format!("refund:local:{}", &log.tx_hash.trim_start_matches("0x")[..16]),
                "amount": log.buyer_amount.as_deref().unwrap_or(log.amount.as_str()),
                "currency": "USDC",
                "provider_ref": format!("evm:{}:{}:{}", log.chain_id, log.tx_hash, log.log_index),
                "evidence": evidence,
            }),
        }),
        _ => Err(ValidationError::new(
            ValidationCode::UnsupportedEventType,
            format!("unsupported evm escrow event {}", log.event_name),
        )),
    }
}
```

- [ ] **Step 4: Run mapping tests**

Run:

```bash
cargo test -p morpheus-server deposited_log_maps_to_payment_authorized
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/morpheus-server/src/evm_escrow.rs crates/morpheus-server/tests/evm_escrow_adapter.rs
git commit -m "Map EVM escrow logs to payment events"
```

---

### Task 8: Add HTTP Surface For EVM Payment Intent

**Files:**
- Modify: `crates/morpheus-api/src/lib.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/http_api.rs`

- [ ] **Step 1: Add failing route test**

Append to `crates/morpheus-server/tests/http_api.rs`:

```rust
#[tokio::test]
async fn seller_evm_payment_intent_returns_confirmation_metadata() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JEVMORDER";
    store
        .upsert_order(OrderProjectionRecord {
            order_id: order_id.into(),
            room_id: "!order:shop.example".into(),
            customer_id: "customer:shop.example:01JCUST".into(),
            seller_id: "seller:shop.example:01JSELLER".into(),
            offer_id: "offer:shop.example:01JOFFER".into(),
            status: "accepted".into(),
            body: json!({
                "order_id": order_id,
                "customer_id": "customer:shop.example:01JCUST",
                "seller_id": "seller:shop.example:01JSELLER",
                "offer_id": "offer:shop.example:01JOFFER",
                "offer_revision": 1,
                "price": {"amount": "25.00", "currency": "USDC"},
                "payment_adapter": "evm_escrow",
                "payment_capture_policy": "before_entitlement",
                "arbiter_actor": "arbiter:shop.example:01JARBITER"
            }),
        })
        .await
        .unwrap();
    let publisher = RecordingPublisher::default();
    let published_events = publisher.published_events.clone();
    let app = build_router_with_publisher(server_config(), store, publisher);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent"))
                .header("authorization", "Bearer seller-token")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "actor_id": "seller:shop.example:01JSELLER",
                        "payment_id": "pay:shop.example:01JPAYEVM",
                        "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                        "seller_evm_address": "0x0000000000000000000000000000000000000003",
                        "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    let events = published_events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "io.marketplace.payment.intent.created");
    assert_eq!(
        events[0]["content"]["body"]["confirmation"]["adapter"],
        "evm_escrow"
    )
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-server seller_evm_payment_intent_returns_confirmation_metadata
```

Expected: FAIL with 404 because route does not exist.

- [ ] **Step 3: Add DTO**

Add to `crates/morpheus-api/src/lib.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowPaymentIntentRequest {
    pub actor_id: String,
    pub payment_id: String,
    pub buyer_evm_address: String,
    pub seller_evm_address: String,
    pub arbiter_evm_address: String,
}
```

- [ ] **Step 4: Add route**

In router setup in `crates/morpheus-server/src/lib.rs`, add:

```rust
        .route(
            "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent",
            post(seller_evm_escrow_payment_intent::<S, P>),
        )
```

Implement handler next to `seller_payment_intent`. It should:

- authorize seller actor;
- load order projection;
- reject if order body `payment_adapter != "evm_escrow"`;
- compute order hash using `compute_order_hash`;
- publish `io.marketplace.payment.intent.created` with `adapter = "evm_escrow"`;
- include `confirmation` object with chain/token/contract/order hash.

Use the existing `order_event_response` helper rather than writing a new Matrix publish path.

- [ ] **Step 5: Run route test**

Run:

```bash
cargo test -p morpheus-server seller_evm_payment_intent_returns_confirmation_metadata
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/morpheus-api/src/lib.rs crates/morpheus-server/src/lib.rs crates/morpheus-server/tests/http_api.rs
git commit -m "Add EVM escrow payment intent route"
```

---

### Task 9: Add Watcher Orchestration Without Background Startup

**Files:**
- Modify: `crates/morpheus-server/src/evm_escrow.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/evm_escrow_adapter.rs`

- [ ] **Step 1: Add failing watcher verification test**

Append to `crates/morpheus-server/tests/evm_escrow_adapter.rs`:

```rust
use morpheus_server::evm_escrow::{ExpectedEscrowPayment, verify_decoded_log};

#[test]
fn watcher_rejects_amount_mismatch() {
    let expected = ExpectedEscrowPayment {
        order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
        chain_id: 31337,
        escrow_contract: "0x0000000000000000000000000000000000000001".into(),
        token: "0x0000000000000000000000000000000000000002".into(),
        amount: "25000000".into(),
        buyer: "0x0000000000000000000000000000000000000004".into(),
        seller: "0x0000000000000000000000000000000000000003".into(),
        arbiter: "0x0000000000000000000000000000000000000005".into(),
    };
    let mut log = deposited_log_fixture();
    log.amount = "24000000".into();

    let err = verify_decoded_log(&expected, &log).unwrap_err();
    assert_eq!(err.code, morpheus_protocol::ValidationCode::PaymentTermsMismatch);
}
```

Add `deposited_log_fixture()` in the test file returning the same `DecodedEscrowLog` shape used in Task 7.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-server watcher_rejects_amount_mismatch
```

Expected: FAIL because `ExpectedEscrowPayment` and `verify_decoded_log` do not exist.

- [ ] **Step 3: Implement verification**

Add to `crates/morpheus-server/src/evm_escrow.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedEscrowPayment {
    pub order_hash: String,
    pub chain_id: i64,
    pub escrow_contract: String,
    pub token: String,
    pub amount: String,
    pub buyer: String,
    pub seller: String,
    pub arbiter: String,
}

pub fn verify_decoded_log(
    expected: &ExpectedEscrowPayment,
    log: &DecodedEscrowLog,
) -> Result<(), ValidationError> {
    let matches = expected.order_hash.eq_ignore_ascii_case(&log.order_hash)
        && expected.chain_id == log.chain_id
        && expected.escrow_contract.eq_ignore_ascii_case(&log.escrow_contract)
        && expected.token.eq_ignore_ascii_case(&log.token)
        && expected.amount == log.amount
        && log
            .buyer
            .as_deref()
            .map(|buyer| expected.buyer.eq_ignore_ascii_case(buyer))
            .unwrap_or(true)
        && log
            .seller
            .as_deref()
            .map(|seller| expected.seller.eq_ignore_ascii_case(seller))
            .unwrap_or(true);
    if matches {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::PaymentTermsMismatch,
            "evm escrow log does not match payment intent",
        ))
    }
}
```

- [ ] **Step 4: Add manual admin replay route for watcher logs**

Add an admin-only route:

```rust
        .route(
            "/admin/evm-escrow/replay",
            post(admin_evm_escrow_replay::<S, P>),
        )
```

This route should trigger one bounded scan using configured `start_block/checkpoint -> latest_confirmed_block` and return JSON with counts:

```json
{"status":"ok","scanned":10,"accepted":1,"duplicates":0}
```

Keep continuous background polling out of this task; a bounded route is easier to test and debug first.

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test -p morpheus-server watcher_rejects_amount_mismatch
cargo test -p morpheus-server evm_escrow
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/morpheus-server/src/evm_escrow.rs crates/morpheus-server/src/lib.rs crates/morpheus-server/tests/evm_escrow_adapter.rs
git commit -m "Add EVM escrow watcher verification"
```

---

### Task 10: Add Buyer/Seller UI States

**Files:**
- Modify: `crates/morpheus-server/ui/assets/app.js`
- Modify: `crates/morpheus-server/ui/assets/app.css`
- Test: `crates/morpheus-server/tests/http_api.rs`

- [ ] **Step 1: Add UI hook test**

Add or extend existing UI route test in `crates/morpheus-server/tests/http_api.rs`:

```rust
#[tokio::test]
async fn buyer_ui_contains_evm_escrow_wallet_hooks() {
    let (status, _content_type, html) = send_ui_body_request("/ui/buyer").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("app.js"));
}

#[tokio::test]
async fn app_js_contains_evm_escrow_hooks() {
    let source = include_str!("../ui/assets/app.js");
    assert!(source.contains("evm_escrow"));
    assert!(source.contains("approve"));
    assert!(source.contains("deposit"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p morpheus-server app_js_contains_evm_escrow_hooks
```

Expected: FAIL because `app.js` does not include `evm_escrow` wallet hooks.

- [ ] **Step 3: Add minimal wallet state helpers**

In `crates/morpheus-server/ui/assets/app.js`, add helpers near payment handling:

```javascript
function isEvmEscrowOrder(order) {
  return pick(order, ["body", "payment_adapter"], "") === "evm_escrow";
}

function evmEscrowConfirmation(order) {
  return pick(order, ["payment", "body", "confirmation"], null)
    || pick(order, ["body", "payment_confirmation"], null)
    || null;
}

async function requestEvmEscrowDeposit(order) {
  const confirmation = evmEscrowConfirmation(order);
  if (!confirmation || !window.ethereum) {
    throw new Error("EVM wallet is not available for this order");
  }
  const [account] = await window.ethereum.request({ method: "eth_requestAccounts" });
  await window.ethereum.request({
    method: "wallet_switchEthereumChain",
    params: [{ chainId: `0x${Number(confirmation.chain_id).toString(16)}` }]
  });
  return { account, confirmation };
}
```

Wire `requestEvmEscrowDeposit` behind an `evm_escrow` payment button. Keep actual `approve/deposit` transaction construction in a small helper that can be replaced by a more complete ABI encoder during Task 12.

- [ ] **Step 4: Run UI hook test**

Run:

```bash
cargo test -p morpheus-server app_js_contains_evm_escrow_hooks
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/morpheus-server/ui/assets/app.js crates/morpheus-server/ui/assets/app.css crates/morpheus-server/tests/http_api.rs
git commit -m "Add EVM escrow UI hooks"
```

---

### Task 11: Add Documentation

**Files:**
- Create: `docs/protocol-evm-escrow.md`
- Modify: `README.md`

- [ ] **Step 1: Write operator note**

Create `docs/protocol-evm-escrow.md`:

```markdown
# EVM Escrow Payment Adapter

`evm_escrow` lets Morpheus orders use an ERC-20 escrow contract for token custody.

Morpheus records marketplace state in Matrix. The EVM chain records custody state. The bridge between them is `order_hash`, generated from locked order terms and adapter configuration.

Local development uses:

- Vyper contracts in `contracts/src`;
- Moccasin/Titanoboa tests in `contracts/tests`;
- Anvil as the local EVM JSON-RPC chain;
- Cast for manual inspection;
- `contracts/deployments/local.json` for local addresses.

The watcher accepts payment state only after reading contract logs from RPC, verifying receipt success, waiting configured confirmations, and matching token, amount, actors, chain id, contract address, and order hash.
```

- [ ] **Step 2: Update README**

Add to `README.md` under stack or documents:

```markdown
- [EVM Escrow Payment Adapter](docs/protocol-evm-escrow.md) describes the planned Vyper-based ERC-20 escrow adapter, local Anvil execution, and watcher verification model.
```

- [ ] **Step 3: Commit**

```bash
git add docs/protocol-evm-escrow.md README.md
git commit -m "Document EVM escrow adapter"
```

---

### Task 12: Add Local Anvil E2E

**Files:**
- Create: `scripts/e2e/run-evm-escrow.sh`
- Modify: `Makefile`
- Test: `scripts/e2e/run-evm-escrow.sh`

- [ ] **Step 1: Add e2e script**

Create `scripts/e2e/run-evm-escrow.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if ! command -v anvil >/dev/null 2>&1; then
  echo "anvil is required" >&2
  exit 1
fi

if ! command -v mox >/dev/null 2>&1; then
  echo "mox is required" >&2
  exit 1
fi

ANVIL_LOG="${TMPDIR:-/tmp}/morpheus-anvil.log"
anvil --chain-id 31337 >"$ANVIL_LOG" 2>&1 &
ANVIL_PID=$!
trap 'kill "$ANVIL_PID" >/dev/null 2>&1 || true' EXIT

sleep 2

(
  cd contracts
  mox test -q
  mox run script/deploy.py --network local
)

test -s contracts/deployments/local.json
cargo test -p morpheus-server evm_escrow
```

- [ ] **Step 2: Make script executable**

Run:

```bash
chmod +x scripts/e2e/run-evm-escrow.sh
```

- [ ] **Step 3: Add Make target**

Add to `Makefile`:

```make
.PHONY: e2e-evm-escrow
e2e-evm-escrow:
	./scripts/e2e/run-evm-escrow.sh
```

- [ ] **Step 4: Run e2e script**

Run:

```bash
make e2e-evm-escrow
```

Expected: Vyper contract tests pass, deployment JSON is written, and Rust `evm_escrow` tests pass.

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e/run-evm-escrow.sh Makefile
git commit -m "Add local EVM escrow e2e target"
```

---

## Final Verification

- [ ] Run Rust checks:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

- [ ] Run contract checks:

```bash
cd contracts
mox test -q
```

- [ ] Run local escrow e2e when Anvil/Moccasin are installed:

```bash
make e2e-evm-escrow
```

- [ ] Review git history:

```bash
git status --short
git log --oneline -12
```

Expected: working tree clean after all commits, with one commit per task.

---

## Spec Coverage Self-Review

- Vyper-first contract stack: covered by Tasks 1-3.
- Moccasin/Titanoboa tests: covered by Tasks 1-2.
- Foundry Anvil/Cast support: covered by Tasks 3 and 12.
- Local execution and deployment JSON: covered by Tasks 3 and 12.
- Production configuration shape: covered by Task 4 and docs in Task 11.
- Order hash binding: covered by Task 5.
- Watcher persistence: covered by Task 6.
- Log detection, finality, and event mapping: covered by Tasks 7 and 9.
- API/UI behavior: covered by Tasks 8 and 10.
- Documentation and rollout notes: covered by Task 11.
