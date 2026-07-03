# EVM Escrow Full Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the `evm_escrow` adapter as a code-complete MVP with embedded log watching, real wallet transactions through `viem`, wallet-driven release/refund, local Anvil E2E, and production guardrails.

**Architecture:** Keep Morpheus/Matrix as the marketplace lifecycle source of truth and the EVM escrow contract as the custody source of truth. Users submit custody-changing transactions through wallets; `morpheus-server` verifies finalized logs through JSON-RPC before publishing existing payment events. The static UI gains a minimal `viem` build step while the served artifact remains a committed static asset.

**Tech Stack:** Rust, Axum, Tokio, SQLx, reqwest JSON-RPC, Vyper, Moccasin/Titanoboa, Foundry Anvil/Cast, TypeScript/JavaScript, Vite, viem.

---

## File Structure

Create:

- `crates/morpheus-server/src/evm_rpc.rs`: small EVM JSON-RPC client and strict DTO parsing.
- `crates/morpheus-server/src/evm_watcher.rs`: embedded watcher loop, scan logic, expected-payment lookup, publishing.
- `crates/morpheus-server/tests/evm_rpc.rs`: JSON-RPC parsing and error tests.
- `crates/morpheus-server/tests/evm_escrow_watcher.rs`: fake RPC/fake publisher watcher tests.
- `crates/morpheus-server/ui/src/app.js`: source copy of current UI logic, converted for bundling.
- `crates/morpheus-server/ui/src/evmWallet.js`: viem wallet helpers and contract ABIs.
- `crates/morpheus-server/ui/src/evmWallet.test.mjs`: Node-level calldata/action tests with mocked wallet client.
- `vite.config.mjs`: minimal Vite config emitting `crates/morpheus-server/ui/assets/app.bundle.js`.
- `package.json`: UI build/test scripts and `viem` dependency.
- `scripts/e2e/evm-escrow-flow.py`: E2E orchestration helper for order/intent HTTP calls and Cast transaction submission.

Modify:

- `Cargo.toml`: add `alloy-primitives` for Keccak/event topic hashing.
- `crates/morpheus-server/Cargo.toml`: depend on `alloy-primitives`.
- `crates/morpheus-server/src/lib.rs`: route watcher status, serve bundle, expose watcher launch helper.
- `crates/morpheus-server/src/main.rs`: start embedded watcher when `evm_escrow` is enabled.
- `crates/morpheus-config/src/lib.rs`: add optional bounded scan fields with validation.
- `crates/morpheus-store/src/lib.rs`: add rejected/pending watcher log state for retry-safe checkpointing.
- `migrations/sqlite/0001_initial.sql`: add watcher status/rejected state tables.
- `migrations/postgres/0001_initial.sql`: add watcher status/rejected state tables.
- `crates/morpheus-server/ui/admin.html`: show watcher status and arbiter refund controls as the arbiter demo surface.
- `crates/morpheus-server/ui/seller.html`: ensure seller wallet settings and release action surface.
- `crates/morpheus-server/ui/buyer.html`: ensure buyer wallet deposit action surface uses bundle.
- `crates/morpheus-server/ui/assets/app.js`: keep as legacy unbundled source for source-level tests; serve `app.bundle.js` from HTML.
- `crates/morpheus-server/ui/assets/app.css`: add wallet pending/action state styles.
- `scripts/e2e/run-evm-escrow.sh`: run full server/watcher flow, not only contract tests.
- `Makefile`: add `ui-build` and include it in `check`.
- `docs/protocol-evm-escrow.md`: document watcher, wallet role model, and release gates.

---

### Task 1: Add Bounded Watcher Configuration

**Files:**
- Modify: `crates/morpheus-config/src/lib.rs`
- Modify: `config/local.toml`
- Test: `crates/morpheus-config/src/lib.rs`

- [ ] **Step 1: Write failing config tests**

Add tests near the existing `evm_escrow` config tests:

```rust
#[test]
fn validates_evm_escrow_scan_bounds_when_enabled() {
    let mut evm = valid_evm_escrow_config();
    evm.max_scan_blocks = Some(250);
    evm.start_block = Some(12);
    let config = config_with_evm_escrow(evm);

    validate_config(&config).unwrap();
}

#[test]
fn rejects_zero_evm_escrow_scan_bound() {
    let mut evm = valid_evm_escrow_config();
    evm.max_scan_blocks = Some(0);
    let config = config_with_evm_escrow(evm);

    assert_error(
        validate_config(&config),
        "evm_escrow max_scan_blocks must be positive",
    );
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p morpheus-config evm_escrow_scan
```

Expected: FAIL because `EvmEscrowConfig` does not have `max_scan_blocks` or `start_block`.

- [ ] **Step 3: Add config fields**

Add fields to `EvmEscrowConfig`:

```rust
#[serde(default)]
pub start_block: Option<u64>,
#[serde(default)]
pub max_scan_blocks: Option<u64>,
```

Update `valid_evm_escrow_config()` with:

```rust
start_block: Some(0),
max_scan_blocks: Some(100),
```

- [ ] **Step 4: Add validation**

Inside enabled `evm_escrow` validation:

```rust
if let Some(max_scan_blocks) = evm.max_scan_blocks {
    anyhow::ensure!(
        max_scan_blocks > 0,
        "evm_escrow max_scan_blocks must be positive"
    );
}
```

- [ ] **Step 5: Update local config**

Add to `config/local.toml`:

```toml
start_block = 0
max_scan_blocks = 100
```

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt --check
cargo test -p morpheus-config evm_escrow
```

Commit:

```bash
git add crates/morpheus-config/src/lib.rs config/local.toml
git commit -m "Add EVM escrow watcher scan config"
```

---

### Task 2: Add Strict EVM JSON-RPC Client

**Files:**
- Create: `crates/morpheus-server/src/evm_rpc.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/evm_rpc.rs`

- [ ] **Step 1: Write failing JSON-RPC parsing tests**

Create `crates/morpheus-server/tests/evm_rpc.rs`:

```rust
use morpheus_server::evm_rpc::{parse_hex_quantity, rpc_log_from_value, rpc_receipt_from_value};
use serde_json::json;

#[test]
fn parses_hex_quantities_strictly() {
    assert_eq!(parse_hex_quantity("0x0").unwrap(), 0);
    assert_eq!(parse_hex_quantity("0x2a").unwrap(), 42);
    assert!(parse_hex_quantity("42").is_err());
    assert!(parse_hex_quantity("0x").is_err());
    assert!(parse_hex_quantity("0xzz").is_err());
}

#[test]
fn parses_rpc_log_with_required_fields() {
    let log = rpc_log_from_value(json!({
        "address": "0x0000000000000000000000000000000000000001",
        "blockHash": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "blockNumber": "0x10",
        "transactionHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "logIndex": "0x2",
        "topics": [
            "0x1111111111111111111111111111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222222222222222222222222222"
        ],
        "data": "0x"
    }))
    .unwrap();

    assert_eq!(log.block_number, 16);
    assert_eq!(log.log_index, 2);
    assert_eq!(log.topics.len(), 2);
}

#[test]
fn parses_successful_receipt_status() {
    let receipt = rpc_receipt_from_value(json!({
        "transactionHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "blockHash": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "blockNumber": "0x10",
        "status": "0x1"
    }))
    .unwrap();

    assert!(receipt.success);
    assert_eq!(receipt.block_number, 16);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p morpheus-server --test evm_rpc
```

Expected: FAIL because `evm_rpc` module does not exist.

- [ ] **Step 3: Add module export**

In `crates/morpheus-server/src/lib.rs`:

```rust
pub mod evm_rpc;
```

- [ ] **Step 4: Implement DTOs and parsers**

Create `crates/morpheus-server/src/evm_rpc.rs`:

```rust
use morpheus_protocol::{ValidationCode, ValidationError};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcLog {
    pub address: String,
    pub block_hash: String,
    pub block_number: i64,
    pub transaction_hash: String,
    pub log_index: i64,
    pub topics: Vec<String>,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcReceipt {
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: i64,
    pub success: bool,
}

pub fn parse_hex_quantity(value: &str) -> Result<i64, ValidationError> {
    let hex = value.strip_prefix("0x").ok_or_else(|| rpc_error("hex quantity missing 0x prefix"))?;
    if hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(rpc_error("invalid hex quantity"));
    }
    i64::from_str_radix(hex, 16).map_err(|err| rpc_error(format!("hex quantity out of range: {err}")))
}

pub fn rpc_log_from_value(value: Value) -> Result<RpcLog, ValidationError> {
    let field = |name: &str| -> Result<String, ValidationError> {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| rpc_error(format!("rpc log missing {name}")))
    };
    let topics = value
        .get("topics")
        .and_then(Value::as_array)
        .ok_or_else(|| rpc_error("rpc log missing topics"))?
        .iter()
        .map(|topic| {
            topic
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| rpc_error("rpc log topic must be string"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RpcLog {
        address: field("address")?,
        block_hash: field("blockHash")?,
        block_number: parse_hex_quantity(&field("blockNumber")?)?,
        transaction_hash: field("transactionHash")?,
        log_index: parse_hex_quantity(&field("logIndex")?)?,
        topics,
        data: field("data")?,
    })
}

pub fn rpc_receipt_from_value(value: Value) -> Result<RpcReceipt, ValidationError> {
    let field = |name: &str| -> Result<String, ValidationError> {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| rpc_error(format!("rpc receipt missing {name}")))
    };
    Ok(RpcReceipt {
        transaction_hash: field("transactionHash")?,
        block_hash: field("blockHash")?,
        block_number: parse_hex_quantity(&field("blockNumber")?)?,
        success: parse_hex_quantity(&field("status")?)? == 1,
    })
}

pub fn rpc_error(message: impl Into<String>) -> ValidationError {
    ValidationError::new(ValidationCode::PolicyViolation, message.into())
}
```

- [ ] **Step 5: Add async client**

Append:

```rust
#[derive(Debug, Clone)]
pub struct EvmRpcClient {
    url: String,
    client: reqwest::Client,
}

impl EvmRpcClient {
    pub fn new(url: String) -> Self {
        Self { url, client: reqwest::Client::new() }
    }

    pub async fn block_number(&self) -> Result<i64, ValidationError> {
        let value = self.call("eth_blockNumber", serde_json::json!([])).await?;
        value.as_str().ok_or_else(|| rpc_error("eth_blockNumber result must be string")).and_then(parse_hex_quantity)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, ValidationError> {
        let response = self
            .client
            .post(&self.url)
            .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .send()
            .await
            .map_err(|err| rpc_error(format!("evm rpc {method} request failed: {err}")))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| rpc_error(format!("evm rpc {method} response was not json: {err}")))?;
        if !status.is_success() {
            return Err(rpc_error(format!("evm rpc {method} returned http {status}: {body}")));
        }
        if let Some(error) = body.get("error") {
            return Err(rpc_error(format!("evm rpc {method} returned error: {error}")));
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| rpc_error(format!("evm rpc {method} missing result")))
    }
}
```

This task intentionally stops at `block_number`; Task 3 adds `get_logs` and `transaction_receipt` after `RpcLog` and `RpcReceipt` are covered by parser tests.

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt --check
cargo test -p morpheus-server --test evm_rpc
```

Commit:

```bash
git add crates/morpheus-server/src/lib.rs crates/morpheus-server/src/evm_rpc.rs crates/morpheus-server/tests/evm_rpc.rs
git commit -m "Add EVM JSON-RPC client parsing"
```

---

### Task 3: Decode Raw Escrow Logs

**Files:**
- Modify: `crates/morpheus-server/src/evm_escrow.rs`
- Modify: `crates/morpheus-server/src/evm_rpc.rs`
- Test: `crates/morpheus-server/tests/evm_escrow_adapter.rs`
- Test: `crates/morpheus-server/tests/evm_rpc.rs`

- [ ] **Step 1: Add failing event topic and decode tests**

Append to `crates/morpheus-server/tests/evm_escrow_adapter.rs`:

```rust
use morpheus_server::evm_escrow::{decode_rpc_log, escrow_event_topics};
use morpheus_server::evm_rpc::RpcLog;

#[test]
fn decodes_deposited_rpc_log() {
    let topics = escrow_event_topics();
    let log = RpcLog {
        address: "0x0000000000000000000000000000000000000001".into(),
        block_hash: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        block_number: 10,
        transaction_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        log_index: 0,
        topics: vec![
            topics.deposited.clone(),
            "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
            "0x0000000000000000000000000000000000000000000000000000000000000004".into(),
            "0x0000000000000000000000000000000000000000000000000000000000000003".into(),
        ],
        data: concat!(
            "0x",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "00000000000000000000000000000000000000000000000000000000017d7840",
        )
        .into(),
    };

    let decoded = decode_rpc_log(31337, &log).unwrap();

    assert_eq!(decoded.event_name, "EscrowDeposited");
    assert_eq!(decoded.order_hash, "0x1111111111111111111111111111111111111111111111111111111111111111");
    assert_eq!(decoded.token, "0x0000000000000000000000000000000000000002");
    assert_eq!(decoded.amount, "25000000");
    assert_eq!(decoded.buyer.as_deref(), Some("0x0000000000000000000000000000000000000004"));
    assert_eq!(decoded.seller.as_deref(), Some("0x0000000000000000000000000000000000000003"));
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p morpheus-server decodes_deposited_rpc_log
```

Expected: FAIL because `decode_rpc_log` and `escrow_event_topics` do not exist.

- [ ] **Step 3: Add event topic constants**

Add `alloy-primitives` to workspace dependencies before writing the topic helper:

```toml
alloy-primitives = "1"
```

Add it to `crates/morpheus-server/Cargo.toml`:

```toml
alloy-primitives.workspace = true
```

In `evm_escrow.rs` add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscrowEventTopics {
    pub deposited: String,
    pub released: String,
    pub refunded: String,
    pub partially_refunded: String,
}

impl EscrowEventTopics {
    pub fn all(&self) -> Vec<String> {
        vec![
            self.deposited.clone(),
            self.released.clone(),
            self.refunded.clone(),
            self.partially_refunded.clone(),
        ]
    }
}

pub fn escrow_event_topics() -> EscrowEventTopics {
    EscrowEventTopics {
        deposited: event_topic("EscrowDeposited(bytes32,address,address,address,uint256)"),
        released: event_topic("EscrowReleased(bytes32,address,address,uint256)"),
        refunded: event_topic("EscrowRefunded(bytes32,address,address,uint256)"),
        partially_refunded: event_topic("EscrowPartiallyRefunded(bytes32,address,address,address,uint256,uint256)"),
    }
}

fn event_topic(signature: &str) -> String {
    format!("{:#x}", alloy_primitives::keccak256(signature.as_bytes()))
}
```

- [ ] **Step 4: Implement log decoding helpers**

Add helpers:

```rust
pub fn decode_rpc_log(chain_id: i64, log: &crate::evm_rpc::RpcLog) -> Result<DecodedEscrowLog, ValidationError> {
    let topics = escrow_event_topics();
    let topic0 = log.topics.first().ok_or_else(|| evm_decode_error("evm escrow log missing topic0"))?;
    let words = data_words(&log.data)?;
    let order_hash = topic_bytes32(required_topic(log, 1)?)?;

    if topic0 == &topics.deposited {
        return Ok(DecodedEscrowLog {
            event_name: "EscrowDeposited".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: word_uint(required_word(&words, 1)?)?,
            buyer: Some(topic_address(required_topic(log, 2)?)?),
            seller: Some(topic_address(required_topic(log, 3)?)?),
            buyer_amount: None,
            seller_amount: None,
        });
    }

    if topic0 == &topics.released {
        return Ok(DecodedEscrowLog {
            event_name: "EscrowReleased".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: word_uint(required_word(&words, 1)?)?,
            buyer: None,
            seller: Some(topic_address(required_topic(log, 2)?)?),
            buyer_amount: None,
            seller_amount: None,
        });
    }

    if topic0 == &topics.refunded {
        return Ok(DecodedEscrowLog {
            event_name: "EscrowRefunded".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: word_uint(required_word(&words, 1)?)?,
            buyer: Some(topic_address(required_topic(log, 2)?)?),
            seller: None,
            buyer_amount: None,
            seller_amount: None,
        });
    }

    if topic0 == &topics.partially_refunded {
        let buyer_amount = word_uint(required_word(&words, 1)?)?;
        let seller_amount = word_uint(required_word(&words, 2)?)?;
        return Ok(DecodedEscrowLog {
            event_name: "EscrowPartiallyRefunded".into(),
            order_hash,
            tx_hash: log.transaction_hash.clone(),
            log_index: log.log_index,
            block_number: log.block_number,
            block_hash: log.block_hash.clone(),
            chain_id,
            escrow_contract: log.address.to_lowercase(),
            token: word_address(required_word(&words, 0)?)?,
            amount: sum_uint_strings(&buyer_amount, &seller_amount)?,
            buyer: Some(topic_address(required_topic(log, 2)?)?),
            seller: Some(topic_address(required_topic(log, 3)?)?),
            buyer_amount: Some(buyer_amount),
            seller_amount: Some(seller_amount),
        });
    }

    Err(evm_decode_error(format!("unknown evm escrow topic {topic0}")))
}
```

Add concrete helpers:

```rust
fn topic_bytes32(topic: &str) -> Result<String, ValidationError>;
fn topic_address(topic: &str) -> Result<String, ValidationError>;
fn data_words(data: &str) -> Result<Vec<String>, ValidationError>;
fn word_address(word: &str) -> Result<String, ValidationError>;
fn word_uint(word: &str) -> Result<String, ValidationError>;
fn sum_uint_strings(left: &str, right: &str) -> Result<String, ValidationError>;
fn required_topic(log: &crate::evm_rpc::RpcLog, index: usize) -> Result<&str, ValidationError>;
fn required_word(words: &[String], index: usize) -> Result<&str, ValidationError>;
```

`word_uint` must return a decimal string by parsing the 32-byte hex word as `alloy_primitives::U256`; `sum_uint_strings` must parse both decimal strings as `U256`, add them with checked semantics, and return a decimal string. Do not use `u128` because ERC-20 amounts are `uint256`.

- [ ] **Step 5: Add receipt/getLogs client methods**

In `evm_rpc.rs`, add:

```rust
pub async fn get_logs(&self, from_block: i64, to_block: i64, address: &str, topics: &[String]) -> Result<Vec<RpcLog>, ValidationError>;
pub async fn transaction_receipt(&self, tx_hash: &str) -> Result<Option<RpcReceipt>, ValidationError>;
```

Use JSON-RPC params:

```json
[{
  "fromBlock": "0x1",
  "toBlock": "0x64",
  "address": "0x0000000000000000000000000000000000000001",
  "topics": [["0x1111111111111111111111111111111111111111111111111111111111111111"]]
}]
```

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt --check
cargo test -p morpheus-server --test evm_rpc
cargo test -p morpheus-server decodes_deposited_rpc_log
cargo test -p morpheus-server evm_escrow_adapter
```

Commit:

```bash
git add Cargo.toml crates/morpheus-server/Cargo.toml crates/morpheus-server/src/evm_rpc.rs crates/morpheus-server/src/evm_escrow.rs crates/morpheus-server/tests/evm_rpc.rs crates/morpheus-server/tests/evm_escrow_adapter.rs
git commit -m "Decode EVM escrow RPC logs"
```

---

### Task 4: Build Watcher Core With Fake RPC

**Files:**
- Create: `crates/morpheus-server/src/evm_watcher.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/evm_escrow_watcher.rs`

- [ ] **Step 1: Write failing watcher test for deposit -> authorized**

Create `crates/morpheus-server/tests/evm_escrow_watcher.rs`:

```rust
use async_trait::async_trait;
use morpheus_config::{EvmEscrowConfig, EvmEscrowTokenConfig};
use morpheus_server::evm_rpc::{RpcLog, RpcReceipt};
use morpheus_server::evm_watcher::{EvmLogSource, WatcherPublisher, WatcherScanConfig, scan_once};
use morpheus_store::{EventStore, InMemoryEventStore};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeLogSource {
    head: i64,
    logs: Vec<RpcLog>,
    receipts: Vec<RpcReceipt>,
}

#[async_trait]
impl EvmLogSource for FakeLogSource {
    async fn block_number(&self) -> Result<i64, morpheus_protocol::ValidationError> {
        Ok(self.head)
    }

    async fn get_logs(&self, _from: i64, _to: i64, _address: &str, _topics: &[String]) -> Result<Vec<RpcLog>, morpheus_protocol::ValidationError> {
        Ok(self.logs.clone())
    }

    async fn transaction_receipt(&self, tx_hash: &str) -> Result<Option<RpcReceipt>, morpheus_protocol::ValidationError> {
        Ok(self.receipts.iter().find(|receipt| receipt.transaction_hash == tx_hash).cloned())
    }
}

#[derive(Clone, Default)]
struct FakeWatcherPublisher {
    events: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl WatcherPublisher for FakeWatcherPublisher {
    async fn publish_payment_event(&self, room_id: &str, event_type: &str, body: Value) -> Result<Value, morpheus_protocol::ValidationError> {
        let event = json!({"room_id": room_id, "type": event_type, "content": {"body": body}, "event_id": "$watcher"});
        self.events.lock().unwrap().push(event.clone());
        Ok(event)
    }
}

#[tokio::test]
async fn watcher_publishes_authorized_for_verified_deposit_log() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource { head: 20, logs: vec![deposit_rpc_log()], receipts: vec![success_receipt()] };
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config()).await.unwrap();

    assert_eq!(result.accepted, 1);
    assert_eq!(publisher.events.lock().unwrap()[0]["type"], "io.marketplace.payment.authorized");
}
```

Implement `seed_evm_order_and_payment`, `deposit_rpc_log`, `success_receipt`, and `watcher_config` in the test using existing test fixture values from `http_api.rs` and `evm_escrow_adapter.rs`.

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p morpheus-server --test evm_escrow_watcher watcher_publishes_authorized_for_verified_deposit_log
```

Expected: FAIL because watcher module does not exist.

- [ ] **Step 3: Add watcher traits and scan result**

Create `evm_watcher.rs`:

```rust
use async_trait::async_trait;
use morpheus_config::EvmEscrowConfig;
use morpheus_protocol::ValidationError;
use morpheus_store::EventStore;
use serde_json::Value;

use crate::evm_rpc::{RpcLog, RpcReceipt};

#[async_trait]
pub trait EvmLogSource: Clone + Send + Sync + 'static {
    async fn block_number(&self) -> Result<i64, ValidationError>;
    async fn get_logs(&self, from_block: i64, to_block: i64, address: &str, topics: &[String]) -> Result<Vec<RpcLog>, ValidationError>;
    async fn transaction_receipt(&self, tx_hash: &str) -> Result<Option<RpcReceipt>, ValidationError>;
}

#[async_trait]
pub trait WatcherPublisher: Clone + Send + Sync + 'static {
    async fn publish_payment_event(&self, room_id: &str, event_type: &str, body: Value) -> Result<Value, ValidationError>;
}

#[derive(Debug, Clone)]
pub struct WatcherScanConfig {
    pub evm: EvmEscrowConfig,
    pub instance_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatcherScanResult {
    pub scanned: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub from_block: i64,
    pub to_block: i64,
}
```

- [ ] **Step 4: Implement expected payment lookup**

Add:

```rust
async fn expected_payment_by_order_hash<S: EventStore>(
    store: &S,
    order_hash: &str,
) -> Result<Option<ExpectedPaymentContext>, ValidationError>;

struct ExpectedPaymentContext {
    order_id: String,
    room_id: String,
    payment_id: String,
    currency: String,
    expected: crate::evm_escrow::ExpectedEscrowPayment,
}
```

Build this from `store.orders()` and `store.payments()`, using `payment.body.confirmation`.

- [ ] **Step 5: Implement `scan_once`**

Add `scan_once`:

```rust
pub async fn scan_once<S, R, P>(
    store: &S,
    source: &R,
    publisher: &P,
    config: WatcherScanConfig,
) -> Result<WatcherScanResult, ValidationError>
where
    S: EventStore,
    R: EvmLogSource,
    P: WatcherPublisher,
{
    let chain_id = config.evm.chain_id as i64;
    let checkpoint = store
        .evm_escrow_checkpoint(chain_id, &config.evm.escrow_contract)
        .await?
        .unwrap_or(config.evm.start_block.unwrap_or(0) as i64);
    let head = source.block_number().await?;
    let safe_to = head - config.evm.confirmations as i64;
    if safe_to <= checkpoint {
        return Ok(WatcherScanResult { from_block: checkpoint + 1, to_block: safe_to, ..WatcherScanResult::default() });
    }
    let max_scan = config.evm.max_scan_blocks.unwrap_or(100) as i64;
    let from_block = checkpoint + 1;
    let to_block = std::cmp::min(safe_to, from_block + max_scan - 1);
    let topic_values = crate::evm_escrow::escrow_event_topics().all();
    let logs = source.get_logs(from_block, to_block, &config.evm.escrow_contract, &topic_values).await?;
    let mut result = WatcherScanResult { scanned: logs.len(), from_block, to_block, ..WatcherScanResult::default() };
    for rpc_log in logs {
        process_rpc_log(store, source, publisher, &config, &mut result, rpc_log).await?;
    }
    store.set_evm_escrow_checkpoint(chain_id, &config.evm.escrow_contract, to_block).await?;
    Ok(result)
}
```

Do not advance checkpoint if publishing a verified event fails.

- [ ] **Step 6: Add tests for failure cases**

Add concrete tests:

```rust
#[tokio::test]
async fn watcher_waits_for_confirmations() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource { head: 10, logs: vec![deposit_rpc_log_at_block(10)], receipts: vec![success_receipt_at_block(10)] };
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config_with_confirmations(2)).await.unwrap();

    assert_eq!(result.scanned, 0);
    assert_eq!(result.accepted, 0);
    assert!(publisher.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn watcher_rejects_failed_receipt() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource { head: 20, logs: vec![deposit_rpc_log()], receipts: vec![failed_receipt()] };
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config()).await.unwrap();

    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected, 1);
    assert!(publisher.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn watcher_rejects_amount_mismatch_without_publish() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment_with_amount(&store, "26000000").await;
    let source = FakeLogSource { head: 20, logs: vec![deposit_rpc_log()], receipts: vec![success_receipt()] };
    let publisher = FakeWatcherPublisher::default();

    let result = scan_once(&store, &source, &publisher, watcher_config()).await.unwrap();

    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected, 1);
    assert!(publisher.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn watcher_deduplicates_processed_logs() {
    let store = InMemoryEventStore::default();
    seed_evm_order_and_payment(&store).await;
    let source = FakeLogSource { head: 20, logs: vec![deposit_rpc_log()], receipts: vec![success_receipt()] };
    let publisher = FakeWatcherPublisher::default();

    scan_once(&store, &source, &publisher, watcher_config()).await.unwrap();
    let duplicate = scan_once(&store, &source, &publisher, watcher_config()).await.unwrap();

    assert_eq!(duplicate.duplicates, 1);
    assert_eq!(publisher.events.lock().unwrap().len(), 1);
}
```

- [ ] **Step 7: Verify and commit**

Run:

```bash
cargo fmt --check
cargo test -p morpheus-server --test evm_escrow_watcher
cargo test -p morpheus-server evm_escrow_adapter
```

Commit:

```bash
git add crates/morpheus-server/src/lib.rs crates/morpheus-server/src/evm_watcher.rs crates/morpheus-server/tests/evm_escrow_watcher.rs
git commit -m "Add EVM escrow watcher scan core"
```

---

### Task 5: Wire Embedded Watcher And Admin Status

**Files:**
- Modify: `crates/morpheus-server/src/main.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Modify: `crates/morpheus-server/src/evm_rpc.rs`
- Modify: `crates/morpheus-server/src/evm_watcher.rs`
- Test: `crates/morpheus-server/tests/http_api.rs`

- [ ] **Step 1: Add failing admin status test**

Add to `http_api.rs`:

```rust
#[tokio::test]
async fn admin_evm_escrow_status_reports_checkpoint_and_config() {
    let store = InMemoryEventStore::default();
    store
        .set_evm_escrow_checkpoint(31337, "0x0000000000000000000000000000000000000001", 12)
        .await
        .unwrap();
    let (status, body) = send_admin_request(
        store,
        "GET",
        "/admin/evm-escrow/status",
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], true);
    assert_eq!(body["chain_id"], 31337);
    assert_eq!(body["checkpoint"]["latest_scanned_block"], 12);
    assert_eq!(body["watcher"]["mode"], "embedded");
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p morpheus-server admin_evm_escrow_status_reports_checkpoint_and_config
```

Expected: FAIL with 404.

- [ ] **Step 3: Add admin route**

In router:

```rust
.route("/admin/evm-escrow/status", get(admin_evm_escrow_status::<S, P>))
```

Implement handler:

```rust
async fn admin_evm_escrow_status<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_admin(&headers, &state.config.admin_token) {
        return response;
    }
    let Some(evm) = state.config.evm_escrow.as_ref().filter(|evm| evm.enabled) else {
        return Json(json!({"enabled": false})).into_response();
    };
    let checkpoint = state
        .store
        .evm_escrow_checkpoint(evm.chain_id as i64, &evm.escrow_contract)
        .await;
    // Return stable JSON with config, checkpoint, watcher mode, and last known counts.
}
```

- [ ] **Step 4: Implement production watcher startup helper**

In `evm_watcher.rs`:

```rust
pub fn spawn_evm_escrow_watcher<S, P>(
    store: S,
    publisher: P,
    server_config: crate::ServerConfig,
    rpc_url: String,
)
where
    S: EventStore,
    P: crate::MatrixPublisher,
{
    tokio::spawn(async move {
        let source = crate::evm_rpc::EvmRpcClient::new(rpc_url);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            server_config.evm_escrow.as_ref().map(|evm| evm.poll_interval_secs).unwrap_or(5),
        ));
        loop {
            interval.tick().await;
            let Some(evm) = server_config.evm_escrow.clone() else { continue };
            let _ = scan_once(
                &store,
                &source,
                &publisher,
                WatcherScanConfig { evm, instance_id: server_config.instance_id.clone() },
            ).await;
        }
    });
}
```

Implement `WatcherPublisher` for any `P: MatrixPublisher` in the same module. Add a public helper in `lib.rs` named `watcher_payment_event(room_id, event_type, body)` that returns the same Matrix event envelope shape used by existing seller payment endpoints, then call `publisher.publish(vec![event]).await`.

- [ ] **Step 5: Wire startup in main**

In `main.rs`, after `store` and `publisher` are created and before `build_router_with_publisher` consumes clones:

```rust
if let Some(evm) = config.payments.as_ref().and_then(|payments| payments.evm_escrow.as_ref()).filter(|evm| evm.enabled) {
    let rpc_url = env::var(&evm.rpc_url_env)
        .with_context(|| format!("missing EVM RPC URL env {}", evm.rpc_url_env))?;
    morpheus_server::evm_watcher::spawn_evm_escrow_watcher(
        store.clone(),
        publisher.clone(),
        server_config.clone(),
        rpc_url,
    );
}
```

Refactor `ServerConfig` construction into a local variable so it can be cloned.

- [ ] **Step 6: Replace replay stub with one bounded scan**

Update `/admin/evm-escrow/replay` to construct `EvmRpcClient` from `rpc_url_env`, call `scan_once` once, and return the real `WatcherScanResult`. Remove the previous `json_rpc_log_scanning_not_implemented` response.

- [ ] **Step 7: Verify and commit**

Run:

```bash
cargo fmt --check
cargo test -p morpheus-server admin_evm_escrow_status_reports_checkpoint_and_config
cargo test -p morpheus-server --test evm_escrow_watcher
cargo test --workspace
```

Commit:

```bash
git add crates/morpheus-server/src/main.rs crates/morpheus-server/src/lib.rs crates/morpheus-server/src/evm_rpc.rs crates/morpheus-server/src/evm_watcher.rs crates/morpheus-server/tests/http_api.rs
git commit -m "Wire embedded EVM escrow watcher"
```

---

### Task 6: Add UI Build Pipeline With Viem

**Files:**
- Create: `package.json`
- Create: `vite.config.mjs`
- Create: `crates/morpheus-server/ui/src/app.js`
- Create: `crates/morpheus-server/ui/src/evmWallet.js`
- Modify: `crates/morpheus-server/src/lib.rs`
- Modify: `crates/morpheus-server/ui/buyer.html`
- Modify: `crates/morpheus-server/ui/seller.html`
- Modify: `crates/morpheus-server/ui/admin.html`
- Modify: `.gitignore`
- Test: `crates/morpheus-server/tests/http_api.rs`

- [ ] **Step 1: Add failing bundle asset test**

Add to `http_api.rs`:

```rust
#[tokio::test]
async fn ui_bundle_asset_returns_javascript_without_auth() {
    let (status, content_type, body) = send_ui_body_request("/ui/assets/app.bundle.js").await;

    assert_eq!(status, StatusCode::OK);
    assert!(content_type.as_deref().is_some_and(|value| value.contains("javascript")));
    assert_contains_all(&body, &["viem", "writeContract", "evm_escrow"]);
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p morpheus-server ui_bundle_asset_returns_javascript_without_auth
```

Expected: FAIL with 404.

- [ ] **Step 3: Add package and Vite config**

Create `package.json`:

```json
{
  "private": true,
  "type": "module",
  "scripts": {
    "build:ui": "vite build --config vite.config.mjs",
    "test:ui-wallet": "node crates/morpheus-server/ui/src/evmWallet.test.mjs"
  },
  "dependencies": {
    "viem": "^2.33.0"
  },
  "devDependencies": {
    "vite": "^7.0.0"
  }
}
```

Create `vite.config.mjs`:

```js
import { defineConfig } from "vite";

export default defineConfig({
  build: {
    emptyOutDir: false,
    lib: {
      entry: "crates/morpheus-server/ui/src/app.js",
      name: "MorpheusUi",
      formats: ["iife"],
      fileName: () => "app.bundle.js"
    },
    outDir: "crates/morpheus-server/ui/assets"
  }
});
```

- [ ] **Step 4: Move current UI JS to source**

Copy current `crates/morpheus-server/ui/assets/app.js` into `crates/morpheus-server/ui/src/app.js`.

At the top of `ui/src/app.js`, import wallet helpers:

```js
import {
  requestEvmEscrowDeposit,
  requestEvmEscrowRelease,
  requestEvmEscrowRefund,
  requestEvmEscrowPartialRefund
} from "./evmWallet.js";
```

Remove the old local `requestEvmEscrowDeposit` function from `app.js` after Task 7 moves behavior into `evmWallet.js`.

- [ ] **Step 5: Add temporary wallet helper module**

Create `evmWallet.js`:

```js
export async function requestEvmEscrowDeposit(order) {
  throw new Error("EVM wallet deposit requires Task 7");
}

export async function requestEvmEscrowRelease(order) {
  throw new Error("EVM wallet release requires Task 8");
}

export async function requestEvmEscrowRefund(order) {
  throw new Error("EVM wallet refund requires Task 9");
}

export async function requestEvmEscrowPartialRefund(order, buyerAmount) {
  throw new Error("EVM wallet partial refund requires Task 9");
}
```

- [ ] **Step 6: Serve bundle**

In `lib.rs`, add:

```rust
.route("/ui/assets/app.bundle.js", get(ui_app_bundle_js))
```

and:

```rust
async fn ui_app_bundle_js() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../ui/assets/app.bundle.js"),
    )
}
```

- [ ] **Step 7: Update HTML script tags**

Change `buyer.html`, `seller.html`, and `admin.html`:

```html
<script src="/ui/assets/app.bundle.js" defer></script>
```

- [ ] **Step 8: Build and verify**

Run:

```bash
npm install
npm run build:ui
cargo fmt --check
cargo test -p morpheus-server ui_bundle_asset_returns_javascript_without_auth
cargo test -p morpheus-server ui_html_routes_return_ok_without_auth
```

Expected: UI route tests pass and bundle exists.

- [ ] **Step 9: Commit**

```bash
git add package.json package-lock.json vite.config.mjs crates/morpheus-server/ui/src/app.js crates/morpheus-server/ui/src/evmWallet.js crates/morpheus-server/ui/assets/app.bundle.js crates/morpheus-server/ui/buyer.html crates/morpheus-server/ui/seller.html crates/morpheus-server/ui/admin.html crates/morpheus-server/src/lib.rs crates/morpheus-server/tests/http_api.rs
git commit -m "Add viem UI build pipeline"
```

---

### Task 7: Implement Buyer Approve And Deposit Wallet Flow

**Files:**
- Modify: `crates/morpheus-server/ui/src/evmWallet.js`
- Create: `crates/morpheus-server/ui/src/evmWallet.test.mjs`
- Modify: `crates/morpheus-server/ui/src/app.js`
- Modify: `crates/morpheus-server/ui/assets/app.css`
- Build: `crates/morpheus-server/ui/assets/app.bundle.js`

- [ ] **Step 1: Write failing wallet helper tests**

Create `evmWallet.test.mjs`:

```js
import assert from "node:assert/strict";
import { evmEscrowConfirmation, buildDepositCalls } from "./evmWallet.js";

const order = {
  payment: {
    body: {
      confirmation: {
        chain_id: 31337,
        token: "0x0000000000000000000000000000000000000002",
        amount_units: "25000000",
        escrow_contract: "0x0000000000000000000000000000000000000001",
        order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111",
        buyer_evm_address: "0x0000000000000000000000000000000000000004",
        seller_evm_address: "0x0000000000000000000000000000000000000003",
        arbiter_evm_address: "0x0000000000000000000000000000000000000005"
      }
    }
  }
};

assert.equal(evmEscrowConfirmation(order).order_hash, order.payment.body.confirmation.order_hash);

const calls = buildDepositCalls(order, "0x0000000000000000000000000000000000000004");
assert.equal(calls.approve.address, order.payment.body.confirmation.token);
assert.equal(calls.deposit.address, order.payment.body.confirmation.escrow_contract);
assert.equal(calls.deposit.functionName, "deposit");
assert.equal(calls.deposit.args[0], order.payment.body.confirmation.order_hash);
assert.equal(calls.deposit.args[2], 25000000n);
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
npm run test:ui-wallet
```

Expected: FAIL because helpers are not exported or implemented.

- [ ] **Step 3: Implement ABIs and helper exports**

In `evmWallet.js`:

```js
import { createWalletClient, custom } from "viem";

export const erc20Abi = [
  {
    type: "function",
    name: "approve",
    stateMutability: "nonpayable",
    inputs: [{ name: "spender", type: "address" }, { name: "amount", type: "uint256" }],
    outputs: [{ name: "", type: "bool" }]
  }
];

export const escrowAbi = [
  {
    type: "function",
    name: "deposit",
    stateMutability: "nonpayable",
    inputs: [
      { name: "order_hash", type: "bytes32" },
      { name: "token", type: "address" },
      { name: "amount", type: "uint256" },
      { name: "seller", type: "address" },
      { name: "buyer", type: "address" },
      { name: "arbiter", type: "address" }
    ],
    outputs: []
  }
];
```

Add:

```js
export function evmEscrowConfirmation(order) {
  return order?.payment?.body?.confirmation
    || order?.payment?.confirmation
    || order?.body?.payment_confirmation
    || order?.body?.confirmation
    || null;
}

export function buildDepositCalls(order, account) {
  const confirmation = requireConfirmation(order);
  const buyer = confirmation.buyer_evm_address || account;
  return {
    approve: {
      address: confirmation.token,
      abi: erc20Abi,
      functionName: "approve",
      args: [confirmation.escrow_contract, BigInt(confirmation.amount_units)]
    },
    deposit: {
      address: confirmation.escrow_contract,
      abi: escrowAbi,
      functionName: "deposit",
      args: [
        confirmation.order_hash,
        confirmation.token,
        BigInt(confirmation.amount_units),
        confirmation.seller_evm_address,
        buyer,
        confirmation.arbiter_evm_address
      ]
    }
  };
}

export async function requestEvmEscrowDeposit(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const calls = buildDepositCalls(order, account);
  const approveTxHash = await wallet.writeContract({ ...calls.approve, account });
  const depositTxHash = await wallet.writeContract({ ...calls.deposit, account });
  return { account, approve_tx_hash: approveTxHash, deposit_tx_hash: depositTxHash, status: "submitted_waiting_for_watcher" };
}
```

`requestEvmEscrowDeposit` must:

1. validate confirmation;
2. request account;
3. switch chain;
4. `writeContract(approve)`;
5. `writeContract(deposit)`;
6. return `{ account, approve_tx_hash, deposit_tx_hash, status: "submitted_waiting_for_watcher" }`.

Add shared helper functions in the same file:

```js
export function requireConfirmation(order) {
  const confirmation = evmEscrowConfirmation(order);
  if (!confirmation) throw new Error("EVM escrow confirmation is not available for this order");
  return confirmation;
}

export function requireEthereum(ethereum) {
  if (!ethereum) throw new Error("EVM wallet is not available");
  return ethereum;
}

export async function switchWalletChain(ethereum, chainId) {
  const numeric = Number(chainId);
  if (!Number.isFinite(numeric) || numeric <= 0) throw new Error("EVM chain id is not available for this order");
  await ethereum.request({
    method: "wallet_switchEthereumChain",
    params: [{ chainId: `0x${numeric.toString(16)}` }]
  });
}
```

- [ ] **Step 4: Update app source to use wallet helper**

In `ui/src/app.js`, remove local plan-only helper functions and use imported `requestEvmEscrowDeposit`.

On success:

```js
showResult("EVM escrow deposit", "submitted_waiting_for_watcher", result);
toast("Transaction submitted", "success", "Waiting for Morpheus watcher confirmation.");
```

- [ ] **Step 5: Build and verify**

Run:

```bash
npm run test:ui-wallet
npm run build:ui
cargo test -p morpheus-server app_js_contains_evm_escrow_hooks
cargo test -p morpheus-server ui_bundle_asset_returns_javascript_without_auth
```

- [ ] **Step 6: Commit**

```bash
git add crates/morpheus-server/ui/src/evmWallet.js crates/morpheus-server/ui/src/evmWallet.test.mjs crates/morpheus-server/ui/src/app.js crates/morpheus-server/ui/assets/app.bundle.js crates/morpheus-server/ui/assets/app.css
git commit -m "Add buyer EVM escrow wallet deposit"
```

---

### Task 8: Implement Seller Release Wallet Flow

**Files:**
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/http_api.rs`
- Modify: `crates/morpheus-server/ui/src/evmWallet.js`
- Modify: `crates/morpheus-server/ui/src/evmWallet.test.mjs`
- Modify: `crates/morpheus-server/ui/src/app.js`
- Modify: `crates/morpheus-server/ui/assets/app.css`
- Build: `crates/morpheus-server/ui/assets/app.bundle.js`

- [ ] **Step 1: Add failing release helper test**

Append:

```js
import { buildReleaseCall } from "./evmWallet.js";

const release = buildReleaseCall(order);
assert.equal(release.address, order.payment.body.confirmation.escrow_contract);
assert.equal(release.functionName, "release");
assert.deepEqual(release.args, [order.payment.body.confirmation.order_hash]);
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
npm run test:ui-wallet
```

Expected: FAIL because `buildReleaseCall` is missing.

- [ ] **Step 3: Add release ABI and request helper**

Add release function to `escrowAbi`:

```js
{
  type: "function",
  name: "release",
  stateMutability: "nonpayable",
  inputs: [{ name: "order_hash", type: "bytes32" }],
  outputs: []
}
```

Add:

```js
export function buildReleaseCall(order) {
  const confirmation = requireConfirmation(order);
  return {
    address: confirmation.escrow_contract,
    abi: escrowAbi,
    functionName: "release",
    args: [confirmation.order_hash]
  };
}

export async function requestEvmEscrowRelease(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const release = buildReleaseCall(order);
  const releaseTxHash = await wallet.writeContract({ ...release, account });
  return { account, release_tx_hash: releaseTxHash, status: "submitted_waiting_for_watcher" };
}
```

- [ ] **Step 4: Render seller release action**

In `ui/src/app.js`, add seller wallet action for `evm_escrow` orders when payment is authorized/captured-ready:

```js
function evmEscrowSellerReleaseAction(order) {
  if (!isEvmEscrowOrder(order)) return "";
  const confirmation = evmEscrowConfirmation(order);
  const status = String(order.status || "");
  if (!confirmation || !/payment_authorized|payment_captured|entitlement_granted|entitlement_completed/.test(status)) return "";
  return `<button class="btn btn-small btn-primary" type="button" data-evm-escrow-release data-order-id="${esc(order.order_id || "")}">Release escrow</button>`;
}
```

Bind click handler:

```js
const evmRelease = event.target.closest("[data-evm-escrow-release]");
if (evmRelease) {
  const order = state.orders.find((item) => item.order_id === evmRelease.dataset.orderId);
  requestEvmEscrowRelease(order)
    .then((result) => showResult("EVM escrow release", "submitted_waiting_for_watcher", result))
    .catch((error) => showResult("EVM escrow release", "wallet_unavailable", { error: error.message }));
  return;
}
```

- [ ] **Step 5: Build and verify**

Run:

```bash
npm run test:ui-wallet
npm run build:ui
cargo test -p morpheus-server ui_javascript_renders_status_aware_seller_order_actions
cargo test -p morpheus-server ui_bundle_asset_returns_javascript_without_auth
```

- [ ] **Step 6: Commit**

```bash
git add crates/morpheus-server/ui/src/evmWallet.js crates/morpheus-server/ui/src/evmWallet.test.mjs crates/morpheus-server/ui/src/app.js crates/morpheus-server/ui/assets/app.bundle.js crates/morpheus-server/ui/assets/app.css
git commit -m "Add seller EVM escrow release wallet action"
```

---

### Task 9: Implement Arbiter Refund Wallet Flow

**Files:**
- Modify: `crates/morpheus-server/ui/src/evmWallet.js`
- Modify: `crates/morpheus-server/ui/src/evmWallet.test.mjs`
- Modify: `crates/morpheus-server/ui/src/app.js`
- Modify: `crates/morpheus-server/ui/admin.html`
- Modify: `crates/morpheus-server/ui/assets/app.css`
- Build: `crates/morpheus-server/ui/assets/app.bundle.js`

- [ ] **Step 1: Add failing admin order detail test**

Add to `http_api.rs`:

```rust
#[tokio::test]
async fn admin_order_show_returns_payment_confirmation_for_arbiter_tools() {
    let store = InMemoryEventStore::default();
    let order_id = "ord:shop.example:01JARBEVM";
    insert_evm_order(&store, order_id, "seller:shop.example:01JSELLER").await;
    store
        .upsert_payment(
            "pay:shop.example:01JARBPAY",
            order_id,
            "authorized",
            json!({
                "order_id": order_id,
                "payment_id": "pay:shop.example:01JARBPAY",
                "adapter": "evm_escrow",
                "currency": "USDC",
                "confirmation": {
                    "chain_id": 31337,
                    "token": "0x0000000000000000000000000000000000000002",
                    "amount_units": "25000000",
                    "escrow_contract": "0x0000000000000000000000000000000000000001",
                    "order_hash": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "buyer_evm_address": "0x0000000000000000000000000000000000000004",
                    "seller_evm_address": "0x0000000000000000000000000000000000000003",
                    "arbiter_evm_address": "0x0000000000000000000000000000000000000005"
                }
            }),
        )
        .await
        .unwrap();

    let (status, body) = send_admin_request(
        store,
        "GET",
        &format!("/admin/orders/{order_id}"),
        Some("Bearer admin-token"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["order"]["payment"]["body"]["confirmation"]["order_hash"], "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p morpheus-server admin_order_show_returns_payment_confirmation_for_arbiter_tools
```

Expected: FAIL with 404.

- [ ] **Step 3: Add admin order detail route**

Add route:

```rust
.route("/admin/orders/{order_id}", get(admin_order_show::<S, P>))
```

Implement handler by reusing the enriched order shape from `list_orders`:

```rust
async fn admin_order_show<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_admin(&headers, &state.config.admin_token) {
        return response;
    }
    match enriched_order(&state.store, &order_id).await {
        Ok(Some(order)) => Json(json!({ "order": order })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"}))).into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}
```

If `list_orders` currently builds enrichment inline, extract:

```rust
async fn enriched_order<S: EventStore>(store: &S, order_id: &str) -> Result<Option<Value>, ValidationError>;
```

- [ ] **Step 4: Add failing refund helper tests**

Append:

```js
import { buildRefundCall, buildPartialRefundCall } from "./evmWallet.js";

const refund = buildRefundCall(order);
assert.equal(refund.functionName, "refund");
assert.deepEqual(refund.args, [order.payment.body.confirmation.order_hash]);

const partial = buildPartialRefundCall(order, "10000000");
assert.equal(partial.functionName, "partial_refund");
assert.deepEqual(partial.args, [order.payment.body.confirmation.order_hash, 10000000n]);
```

- [ ] **Step 5: Run test and verify failure**

Run:

```bash
npm run test:ui-wallet
```

Expected: FAIL because refund helpers are missing.

- [ ] **Step 6: Add refund ABI and helpers**

Add to `escrowAbi`:

```js
{
  type: "function",
  name: "refund",
  stateMutability: "nonpayable",
  inputs: [{ name: "order_hash", type: "bytes32" }],
  outputs: []
},
{
  type: "function",
  name: "partial_refund",
  stateMutability: "nonpayable",
  inputs: [{ name: "order_hash", type: "bytes32" }, { name: "buyer_amount", type: "uint256" }],
  outputs: []
}
```

Add:

```js
export function buildRefundCall(order) {
  const confirmation = requireConfirmation(order);
  return {
    address: confirmation.escrow_contract,
    abi: escrowAbi,
    functionName: "refund",
    args: [confirmation.order_hash]
  };
}

export function buildPartialRefundCall(order, buyerAmountUnits) {
  const confirmation = requireConfirmation(order);
  return {
    address: confirmation.escrow_contract,
    abi: escrowAbi,
    functionName: "partial_refund",
    args: [confirmation.order_hash, BigInt(buyerAmountUnits)]
  };
}

export async function requestEvmEscrowRefund(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const refund = buildRefundCall(order);
  const refundTxHash = await wallet.writeContract({ ...refund, account });
  return { account, refund_tx_hash: refundTxHash, status: "submitted_waiting_for_watcher" };
}

export async function requestEvmEscrowPartialRefund(order, buyerAmountUnits, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const partialRefund = buildPartialRefundCall(order, buyerAmountUnits);
  const partialRefundTxHash = await wallet.writeContract({ ...partialRefund, account });
  return { account, partial_refund_tx_hash: partialRefundTxHash, status: "submitted_waiting_for_watcher" };
}
```

- [ ] **Step 7: Add arbiter controls to admin advanced UI**

In `admin.html`, add a compact panel under advanced/debug:

```html
<section class="flow-panel" id="evm-arbiter-tools">
  <h3>EVM escrow arbiter</h3>
  <form class="stack-form" data-form="evm-arbiter-refund">
    <label class="field"><span class="label">Order id</span><input class="input" name="order_id"></label>
    <label class="field"><span class="label">Buyer amount units</span><input class="input" name="buyer_amount_units" placeholder="leave empty for full refund"></label>
    <div class="button-row stretch">
      <button class="btn btn-danger" type="submit" data-refund-mode="full">Refund</button>
      <button class="btn" type="submit" data-refund-mode="partial">Partial refund</button>
    </div>
  </form>
</section>
```

This is an admin-hosted arbiter demo surface; the EVM authority still comes from the connected arbiter wallet.

- [ ] **Step 8: Bind arbiter actions**

In `ui/src/app.js`, admin page should fetch the target order through the new admin route:

```js
async function fetchAdminOrder(orderId) {
  const result = await api(`/admin/orders/${encodeURIComponent(orderId)}`, {
    tokenRole: "admin",
    action: "GET /admin/orders/{order_id}"
  });
  if (!result.ok || !result.body || !result.body.order) {
    throw new Error("Order with EVM payment confirmation is not available");
  }
  return result.body.order;
}
```

On submit:

```js
const order = await fetchAdminOrder(data.order_id);
if (mode === "full") {
  await requestEvmEscrowRefund(order);
} else {
  await requestEvmEscrowPartialRefund(order, data.buyer_amount_units);
}
```

- [ ] **Step 9: Build and verify**

Run:

```bash
npm run test:ui-wallet
npm run build:ui
cargo test -p morpheus-server admin_ui_uses_auto_refresh_instead_of_per_metric_refresh_buttons
cargo test -p morpheus-server admin_order_show_returns_payment_confirmation_for_arbiter_tools
cargo test -p morpheus-server ui_bundle_asset_returns_javascript_without_auth
```

- [ ] **Step 10: Commit**

```bash
git add crates/morpheus-server/src/lib.rs crates/morpheus-server/tests/http_api.rs crates/morpheus-server/ui/src/evmWallet.js crates/morpheus-server/ui/src/evmWallet.test.mjs crates/morpheus-server/ui/src/app.js crates/morpheus-server/ui/admin.html crates/morpheus-server/ui/assets/app.bundle.js crates/morpheus-server/ui/assets/app.css
git commit -m "Add arbiter EVM escrow refund wallet actions"
```

---

### Task 10: Complete Local Anvil E2E Flow

**Files:**
- Modify: `scripts/e2e/run-evm-escrow.sh`
- Create: `scripts/e2e/evm-escrow-flow.py`
- Create: `config/e2e/evm-escrow.toml`
- Test: `make e2e-evm-escrow`

- [ ] **Step 1: Add E2E config**

Create `config/e2e/evm-escrow.toml` by copying `config/local.toml` and changing:

```toml
[instance]
payment_adapters = ["mock", "evm_escrow"]

[database]
url = "postgres://morpheus:morpheus@localhost:5432/morpheus"

[payments.evm_escrow]
enabled = true
chain_id = 31337
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
confirmations = 1
poll_interval_secs = 1
deployments_path = "contracts/deployments/local.json"
start_block = 0
max_scan_blocks = 100
```

The E2E script must patch `escrow_contract`, `default_token`, and token contract in a temporary `.local/e2e/evm-escrow.toml` after `contracts/deployments/local.json` exists. Keep production config explicit; do not make the production config loader override addresses from deployment JSON.

- [ ] **Step 2: Add flow script**

Create `scripts/e2e/evm-escrow-flow.py` that:

1. reads `contracts/deployments/local.json`;
2. updates a temporary `.local/e2e/evm-escrow.toml`;
3. starts `morpheus-server`;
4. creates/accepts EVM order through HTTP;
5. creates payment intent;
6. uses `cast send` to mint/approve/deposit/release with Anvil accounts;
7. polls Morpheus orders until statuses change.

Use deterministic Anvil keys. Example Cast calls:

```bash
cast send "$TOKEN" "mint(address,uint256)" "$BUYER" 25000000 --private-key "$ADMIN_KEY" --rpc-url "$RPC_URL"
cast send "$TOKEN" "approve(address,uint256)" "$ESCROW" 25000000 --private-key "$BUYER_KEY" --rpc-url "$RPC_URL"
cast send "$ESCROW" "deposit(bytes32,address,uint256,address,address,address)" "$ORDER_HASH" "$TOKEN" 25000000 "$SELLER" "$BUYER" "$ARBITER" --private-key "$BUYER_KEY" --rpc-url "$RPC_URL"
cast send "$ESCROW" "release(bytes32)" "$ORDER_HASH" --private-key "$SELLER_OPERATOR_KEY" --rpc-url "$RPC_URL"
```

- [ ] **Step 3: Update e2e runner**

`run-evm-escrow.sh` should:

```bash
require_command anvil
require_command cast
require_command mox
require_command cargo
require_command curl

# start Anvil
# run mox test
# deploy contracts
# start docker compose postgres if it is not already running
# run cargo run -p morpheus-cli -- db migrate
# start morpheus-server with enabled EVM config
# run evm-escrow-flow helper
```

Use the existing root `docker compose up -d postgres` service for the E2E database. The script must not skip the server/watcher portion.

- [ ] **Step 4: Run and debug**

Run:

```bash
make e2e-evm-escrow
```

Expected when dependencies are installed: Vyper tests pass, contracts deploy, watcher observes deposit/release, final order reaches captured/completed projection.

If local dependencies are missing, expected failure must be explicit:

```text
anvil is required
mox is required
cast is required
```

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e/run-evm-escrow.sh scripts/e2e/evm-escrow-flow.py config/e2e/evm-escrow.toml
git commit -m "Complete local EVM escrow e2e flow"
```

---

### Task 11: Update Operator Docs And Guardrails

**Files:**
- Modify: `docs/protocol-evm-escrow.md`
- Modify: `README.md`
- Modify: `contracts/foundry/README.md`
- Test: source checks in `crates/morpheus-server/tests/http_api.rs` if admin text changes

- [ ] **Step 1: Update protocol note**

Add sections:

```markdown
## Watcher Operation

The embedded watcher starts only when `[payments.evm_escrow].enabled = true`.
It reads RPC URL from `rpc_url_env`, scans bounded block ranges, waits configured confirmations, verifies receipts, and publishes payment events only from matching finalized logs.

## Wallet Roles

Buyer wallet submits `approve` and `deposit`.
Seller wallet submits `release`.
Arbiter wallet submits `refund` and `partial_refund`.
Morpheus never treats a submitted transaction hash as final payment state.
```

- [ ] **Step 2: Add production guardrails**

Document:

```markdown
- no mainnet funds before external contract audit;
- use monitored RPC providers;
- configure network-specific confirmations;
- use conservative deposit limits;
- keep wallet/private key material outside Matrix events and committed config;
- keep pause/admin runbook ready before testnet or production funds.
```

- [ ] **Step 3: Update Foundry README**

Add full local E2E:

```markdown
make e2e-evm-escrow
```

and required tools:

```markdown
- Foundry: `anvil`, `cast`
- Moccasin: `mox`
- Node/npm for the viem UI bundle
```

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo fmt --check
cargo test --workspace
```

Commit:

```bash
git add docs/protocol-evm-escrow.md contracts/foundry/README.md README.md
git commit -m "Document full EVM escrow operation"
```

---

## Final Verification

- [ ] Run Rust checks:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

- [ ] Run UI checks:

```bash
npm run test:ui-wallet
npm run build:ui
```

- [ ] Run contract checks:

```bash
cd contracts
mox test -q
```

- [ ] Run full local E2E when Foundry/Moccasin/Node dependencies are installed:

```bash
make e2e-evm-escrow
```

- [ ] Review git state:

```bash
git status --short
git log --oneline -16
```

Expected: working tree clean, one commit per task, with the known external release gates still documented.

---

## Spec Coverage Self-Review

- Real JSON-RPC watcher: Tasks 2-5.
- Receipt/finality verification: Tasks 4-5.
- Payment intent lookup by `order_hash`: Task 4.
- Wallet-driven `approve/deposit`: Tasks 6-7.
- Wallet-driven `release`: Task 8.
- Wallet-driven `refund/partial_refund`: Task 9.
- No backend signer: enforced by Tasks 7-9 and documented in Task 11.
- Local Anvil E2E: Task 10.
- Production guardrails and release gates: Tasks 1, 5, and 11.
