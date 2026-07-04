import assert from "node:assert/strict";
import {
  buildExplorerLink,
  evmLifecycleState,
  evmPaymentStatusRows,
  normalizeAddress,
  roleAddressMismatch,
  watcherStatusLabel
} from "./evmPaymentLifecycle.js";

const confirmation = {
  chain_id: 84532,
  token: "0x0000000000000000000000000000000000000002",
  token_currency: "USDC",
  token_decimals: 6,
  amount_units: "25000000",
  escrow_contract: "0x0000000000000000000000000000000000000001",
  order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111",
  buyer_evm_address: "0x0000000000000000000000000000000000000004",
  seller_evm_address: "0x0000000000000000000000000000000000000003",
  arbiter_evm_address: "0x0000000000000000000000000000000000000005",
  fee_hint: { confirmations: 5 }
};

assert.equal(normalizeAddress(" 0xABCDEF "), "0xabcdef");
assert.equal(normalizeAddress(""), "");

assert.equal(
  roleAddressMismatch("buyer", "0x0000000000000000000000000000000000000004", confirmation),
  ""
);
assert.match(
  roleAddressMismatch("seller", "0x0000000000000000000000000000000000000004", confirmation),
  /Expected seller wallet 0x0000000000000000000000000000000000000003/
);

assert.equal(
  buildExplorerLink({ explorer_base_url: "https://sepolia.basescan.org" }, "tx", "0xabc"),
  "https://sepolia.basescan.org/tx/0xabc"
);
assert.equal(buildExplorerLink({}, "tx", "0xabc"), "");

assert.deepEqual(
  evmLifecycleState({
    order: { status: "payment_intent_created", payment: { body: { confirmation } } },
    pendingAction: { kind: "deposit", txHash: "0xdep" },
    watcher: { last_scan: null, last_error: null }
  }),
  {
    state: "deposit_submitted",
    tone: "pending",
    label: "Deposit submitted",
    detail: "Waiting for Morpheus watcher confirmation."
  }
);

assert.equal(
  evmLifecycleState({
    order: { status: "payment_authorized", payment: { body: { confirmation } } },
    pendingAction: null,
    watcher: { last_scan: { status: "ok" }, last_error: null }
  }).state,
  "escrow_funded"
);

assert.equal(
  evmLifecycleState({
    order: { status: "payment_captured", payment: { body: { confirmation } } },
    pendingAction: null,
    watcher: { last_scan: { status: "ok" }, last_error: null }
  }).state,
  "captured"
);

assert.equal(
  evmLifecycleState({
    order: { status: "payment_refunded", payment: { body: { confirmation } } },
    pendingAction: null,
    watcher: { last_scan: { status: "ok" }, last_error: null }
  }).state,
  "refunded"
);

assert.equal(
  evmLifecycleState({
    order: { status: "payment_intent_created", payment: { body: { confirmation } } },
    pendingAction: null,
    watcher: { last_error: { message: "RPC timeout" } }
  }).state,
  "watcher_lagging"
);

assert.equal(
  watcherStatusLabel({ last_scan: { status: "ok", to_block: 123 }, last_error: null }),
  "Watcher ok through block 123"
);
assert.equal(
  watcherStatusLabel({ last_scan: null, last_error: { message: "RPC timeout" } }),
  "Watcher error: RPC timeout"
);

const rows = evmPaymentStatusRows({
  confirmation,
  watcher: { last_scan: { status: "ok", to_block: 123 }, last_error: null },
  network: { explorer_base_url: "https://sepolia.basescan.org" }
});
assert(rows.some((row) => row.label === "Escrow contract" && row.href.endsWith("/address/0x0000000000000000000000000000000000000001")));
assert(rows.some((row) => row.label === "Order hash" && row.value === confirmation.order_hash));
