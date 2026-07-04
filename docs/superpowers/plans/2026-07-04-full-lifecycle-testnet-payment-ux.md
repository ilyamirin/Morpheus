# Full Lifecycle Testnet Payment UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a browser-based full lifecycle EVM escrow rehearsal flow for public testnet usage: buyer deposit, seller release, arbiter refund, arbiter partial refund, watcher status, explorer links, and explicit wallet/error states.

**Architecture:** Keep low-level contract calls in `evmWallet.js`, add a focused lifecycle helper module for state/explorer/status decisions, and keep `app.js` responsible for rendering and binding UI actions. The browser must never treat a submitted wallet transaction hash as final payment success; final state is shown only from projected Morpheus payment/order state or watcher/status evidence.

**Tech Stack:** Vanilla browser JavaScript, viem wallet client, Vite UI build, Playwright browser smoke test, Node assert-based UI unit tests, Rust backend watcher/status tests, existing EVM escrow runbook.

---

## File Structure

- Create `crates/morpheus-server/ui/src/evmPaymentLifecycle.js`
  - Pure helpers for lifecycle state, role checks, explorer links, watcher status labels, and safe evidence summaries.
- Create `crates/morpheus-server/ui/src/evmPaymentLifecycle.test.mjs`
  - Node assert tests for the helper module.
- Modify `crates/morpheus-server/ui/src/evmWallet.js`
  - Add role validation before signing and classify wallet errors without changing contract call encoding.
- Modify `crates/morpheus-server/ui/src/evmWallet.test.mjs`
  - Cover role validation and error classification.
- Modify `crates/morpheus-server/ui/src/app.js`
  - Render the lifecycle status panel, explorer links, role-specific actions, watcher state, and explicit refund confirmation.
- Modify `crates/morpheus-server/ui/src/input.css`
  - Add compact payment status styling that works in buyer, seller, and admin themes.
- Modify `scripts/e2e/ui-wallet-flow.mjs`
  - Expand the mocked browser flow to cover deposit, release, full refund, partial refund, watcher lag, chain mismatch, and wallet rejection.
- Modify `package.json`
  - Make `npm run test:ui-wallet` run both wallet and lifecycle unit tests.
- Modify `docs/evm-escrow-production-runbook.md`
  - Add manual full lifecycle public testnet browser rehearsal steps.

## Scope Boundary

Do not add production real-money enablement, relayers, gas sponsorship, multi-arbiter governance, automatic timers, or non-EVM payment rails in this plan.

---

### Task 1: Add Pure Lifecycle Helpers

**Files:**
- Create: `crates/morpheus-server/ui/src/evmPaymentLifecycle.js`
- Create: `crates/morpheus-server/ui/src/evmPaymentLifecycle.test.mjs`
- Modify: `package.json`

- [ ] **Step 1: Write the failing lifecycle helper tests**

Create `crates/morpheus-server/ui/src/evmPaymentLifecycle.test.mjs`:

```js
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
```

- [ ] **Step 2: Run the lifecycle helper test to verify it fails**

Run:

```bash
node crates/morpheus-server/ui/src/evmPaymentLifecycle.test.mjs
```

Expected: FAIL with `Cannot find module` for `evmPaymentLifecycle.js`.

- [ ] **Step 3: Implement the lifecycle helper module**

Create `crates/morpheus-server/ui/src/evmPaymentLifecycle.js`:

```js
export function normalizeAddress(value) {
  return String(value || "").trim().toLowerCase();
}

export function confirmationFromOrder(order) {
  return order?.payment?.body?.confirmation
    || order?.payment?.confirmation
    || order?.body?.payment_confirmation
    || order?.body?.confirmation
    || null;
}

export function roleAddress(role, confirmation) {
  const key = `${role}_evm_address`;
  return normalizeAddress(confirmation?.[key]);
}

export function roleAddressMismatch(role, account, confirmation) {
  const expected = roleAddress(role, confirmation);
  const actual = normalizeAddress(account);
  if (!expected || !actual || expected === actual) return "";
  return `Expected ${role} wallet ${expected}, connected ${actual}`;
}

export function buildExplorerLink(network, kind, value) {
  const base = String(network?.explorer_base_url || "").replace(/\/+$/, "");
  const id = String(value || "").trim();
  if (!base || !id) return "";
  if (kind === "tx") return `${base}/tx/${id}`;
  if (kind === "address") return `${base}/address/${id}`;
  if (kind === "token") return `${base}/token/${id}`;
  return "";
}

export function watcherStatusLabel(watcher) {
  if (watcher?.last_error?.message) return `Watcher error: ${watcher.last_error.message}`;
  if (watcher?.last_scan?.to_block !== undefined && watcher?.last_scan?.to_block !== null) {
    return `Watcher ok through block ${watcher.last_scan.to_block}`;
  }
  return "Watcher has not reported a finalized scan yet";
}

export function evmLifecycleState({ order, pendingAction, watcher }) {
  const status = String(order?.status || order?.payment?.status || "").toLowerCase();
  if (watcher?.last_error?.message && !pendingAction) {
    return {
      state: "watcher_lagging",
      tone: "warning",
      label: "Watcher needs attention",
      detail: watcherStatusLabel(watcher)
    };
  }
  if (pendingAction?.kind === "deposit") {
    return {
      state: "deposit_submitted",
      tone: "pending",
      label: "Deposit submitted",
      detail: "Waiting for Morpheus watcher confirmation."
    };
  }
  if (pendingAction?.kind === "release") {
    return {
      state: "release_submitted",
      tone: "pending",
      label: "Release submitted",
      detail: "Waiting for Morpheus watcher confirmation."
    };
  }
  if (pendingAction?.kind === "refund") {
    return {
      state: "refund_submitted",
      tone: "pending",
      label: "Refund submitted",
      detail: "Waiting for Morpheus watcher confirmation."
    };
  }
  if (pendingAction?.kind === "partial_refund") {
    return {
      state: "partial_refund_submitted",
      tone: "pending",
      label: "Partial refund submitted",
      detail: "Waiting for Morpheus watcher confirmation."
    };
  }
  if (status === "payment_captured") {
    return { state: "captured", tone: "success", label: "Payment captured", detail: "Escrow release was verified by Morpheus." };
  }
  if (status === "payment_refunded") {
    return { state: "refunded", tone: "success", label: "Payment refunded", detail: "Escrow refund was verified by Morpheus." };
  }
  if (status === "payment_authorized") {
    return { state: "escrow_funded", tone: "success", label: "Escrow funded", detail: "Deposit was verified by Morpheus." };
  }
  if (status === "payment_intent_created") {
    return { state: "intent_ready", tone: "neutral", label: "Payment intent ready", detail: "Buyer can approve and deposit testnet tokens." };
  }
  return { state: "intent_ready", tone: "neutral", label: "Waiting for payment intent", detail: "Escrow payment intent is not available yet." };
}

export function evmPaymentStatusRows({ confirmation, watcher, network, txHash }) {
  if (!confirmation) return [];
  const rows = [
    { label: "Chain", value: String(confirmation.chain_id || "") },
    {
      label: "Escrow contract",
      value: confirmation.escrow_contract || "",
      href: buildExplorerLink(network, "address", confirmation.escrow_contract)
    },
    {
      label: "Token",
      value: confirmation.token || "",
      href: buildExplorerLink(network, "token", confirmation.token)
    },
    { label: "Amount units", value: String(confirmation.amount_units || "") },
    { label: "Order hash", value: confirmation.order_hash || "" },
    { label: "Confirmations", value: String(confirmation?.fee_hint?.confirmations || "") },
    { label: "Watcher", value: watcherStatusLabel(watcher) }
  ];
  if (txHash) rows.push({ label: "Pending tx", value: txHash, href: buildExplorerLink(network, "tx", txHash) });
  return rows.filter((row) => row.value);
}
```

- [ ] **Step 4: Add the lifecycle test to the UI wallet test script**

Modify `package.json`:

```json
{
  "scripts": {
    "build:ui": "vite build --config vite.config.mjs",
    "test:ui-wallet": "node crates/morpheus-server/ui/src/evmWallet.test.mjs && node crates/morpheus-server/ui/src/evmPaymentLifecycle.test.mjs",
    "test:ui-wallet-flow": "node scripts/e2e/ui-wallet-flow.mjs",
    "build:ui-css": "tailwindcss -c tailwind.config.js -i crates/morpheus-server/ui/src/input.css -o crates/morpheus-server/ui/assets/app.css --minify"
  }
}
```

Keep existing dependencies unchanged.

- [ ] **Step 5: Run lifecycle tests**

Run:

```bash
npm run test:ui-wallet
```

Expected: PASS with no assertion failures.

- [ ] **Step 6: Commit Task 1**

```bash
git add package.json crates/morpheus-server/ui/src/evmPaymentLifecycle.js crates/morpheus-server/ui/src/evmPaymentLifecycle.test.mjs
git commit -m "Add EVM payment lifecycle UI helpers"
```

---

### Task 2: Add Wallet Role Validation And Error Classification

**Files:**
- Modify: `crates/morpheus-server/ui/src/evmWallet.js`
- Modify: `crates/morpheus-server/ui/src/evmWallet.test.mjs`

- [ ] **Step 1: Write failing wallet validation tests**

Append to `crates/morpheus-server/ui/src/evmWallet.test.mjs`:

```js
import {
  classifyWalletError,
  requireRoleWallet
} from "./evmWallet.js";

assert.equal(
  requireRoleWallet("buyer", "0x0000000000000000000000000000000000000004", order.payment.body.confirmation),
  "0x0000000000000000000000000000000000000004"
);
assert.throws(
  () => requireRoleWallet("seller", "0x0000000000000000000000000000000000000004", order.payment.body.confirmation),
  /Expected seller wallet 0x0000000000000000000000000000000000000003/
);

assert.deepEqual(classifyWalletError(new Error("User rejected the request.")), {
  code: "wallet_rejected",
  message: "User rejected the request."
});
assert.deepEqual(classifyWalletError(new Error("wallet_switchEthereumChain failed")), {
  code: "chain_mismatch",
  message: "wallet_switchEthereumChain failed"
});
assert.deepEqual(classifyWalletError(new Error("RPC timeout")), {
  code: "wallet_error",
  message: "RPC timeout"
});
```

- [ ] **Step 2: Run the wallet unit test to verify it fails**

Run:

```bash
node crates/morpheus-server/ui/src/evmWallet.test.mjs
```

Expected: FAIL because `classifyWalletError` and `requireRoleWallet` are not exported.

- [ ] **Step 3: Implement role validation and error classification**

Modify the top of `crates/morpheus-server/ui/src/evmWallet.js`:

```js
import { createWalletClient, custom } from "viem";
import { roleAddressMismatch } from "./evmPaymentLifecycle.js";
```

Add these exports after `requireEthereum`:

```js
export function requireRoleWallet(role, account, confirmation) {
  const mismatch = roleAddressMismatch(role, account, confirmation);
  if (mismatch) throw new Error(mismatch);
  return account;
}

export function classifyWalletError(error) {
  const message = String(error?.message || error || "Wallet request failed");
  if (/reject|denied|cancel/i.test(message)) return { code: "wallet_rejected", message };
  if (/switchEthereumChain|wrong chain|chain/i.test(message)) return { code: "chain_mismatch", message };
  return { code: "wallet_error", message };
}
```

Update signing functions:

```js
export async function requestEvmEscrowDeposit(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  requireRoleWallet("buyer", account, confirmation);
  await switchWalletChain(ethereum, confirmation.chain_id);
  const calls = buildDepositCalls(order, account);
  const approveTxHash = await wallet.writeContract({ ...calls.approve, account });
  const depositTxHash = await wallet.writeContract({ ...calls.deposit, account });
  return {
    account,
    approve_tx_hash: approveTxHash,
    deposit_tx_hash: depositTxHash,
    status: "submitted_waiting_for_watcher"
  };
}

export async function requestEvmEscrowRelease(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  requireRoleWallet("seller", account, confirmation);
  await switchWalletChain(ethereum, confirmation.chain_id);
  const release = buildReleaseCall(order);
  const releaseTxHash = await wallet.writeContract({ ...release, account });
  return {
    account,
    release_tx_hash: releaseTxHash,
    status: "submitted_waiting_for_watcher"
  };
}

export async function requestEvmEscrowRefund(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  requireRoleWallet("arbiter", account, confirmation);
  await switchWalletChain(ethereum, confirmation.chain_id);
  const refund = buildRefundCall(order);
  const refundTxHash = await wallet.writeContract({ ...refund, account });
  return {
    account,
    refund_tx_hash: refundTxHash,
    status: "submitted_waiting_for_watcher"
  };
}

export async function requestEvmEscrowPartialRefund(order, buyerAmount, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  requireRoleWallet("arbiter", account, confirmation);
  await switchWalletChain(ethereum, confirmation.chain_id);
  const partialRefund = buildPartialRefundCall(order, buyerAmount);
  const partialRefundTxHash = await wallet.writeContract({ ...partialRefund, account });
  return {
    account,
    partial_refund_tx_hash: partialRefundTxHash,
    status: "submitted_waiting_for_watcher"
  };
}
```

- [ ] **Step 4: Run wallet tests**

Run:

```bash
npm run test:ui-wallet
```

Expected: PASS.

- [ ] **Step 5: Commit Task 2**

```bash
git add crates/morpheus-server/ui/src/evmWallet.js crates/morpheus-server/ui/src/evmWallet.test.mjs
git commit -m "Validate EVM escrow wallet roles"
```

---

### Task 3: Render Payment Status Panel And Explorer Links

**Files:**
- Modify: `crates/morpheus-server/ui/src/app.js`
- Modify: `crates/morpheus-server/ui/src/input.css`

- [ ] **Step 1: Add imports and pending EVM UI state**

Modify the import block in `crates/morpheus-server/ui/src/app.js`:

```js
import {
  buildExplorerLink,
  evmLifecycleState,
  evmPaymentStatusRows
} from "./evmPaymentLifecycle.js";
import {
  evmEscrowConfirmation,
  escrowPolicyHint,
  classifyWalletError,
  requestEvmEscrowDeposit,
  requestEvmEscrowRelease,
  requestEvmEscrowRefund,
  requestEvmEscrowPartialRefund
} from "./evmWallet.js";
```

Extend the `state` object:

```js
const state = {
  sellers: [],
  products: [],
  offers: [],
  orders: [],
  selectedOffer: null,
  pendingOrders: [],
  pendingListings: [],
  evm: {
    watcher: null,
    pendingActions: {}
  },
  admin: { healthOk: false, readyOk: false, incidents: [], pendingMaintenance: null }
};
```

- [ ] **Step 2: Add helper functions for network config and pending tx extraction**

Add after `isEvmEscrowOrder(order)`:

```js
function evmNetworkConfig(confirmation = {}) {
  const configured = UI_CONFIG.evm_escrow || UI_CONFIG.evm || {};
  return {
    explorer_base_url: configured.explorer_base_url || confirmation.explorer_base_url || ""
  };
}

function pendingEvmAction(orderId) {
  return state.evm.pendingActions[orderId] || null;
}

function pendingEvmTxHash(action) {
  return action?.deposit_tx_hash
    || action?.release_tx_hash
    || action?.refund_tx_hash
    || action?.partial_refund_tx_hash
    || "";
}

function rememberPendingEvmAction(order, kind, result) {
  const orderId = order?.order_id || "";
  if (!orderId) return;
  state.evm.pendingActions[orderId] = {
    kind,
    ...result,
    updated_at_unix_ms: Date.now()
  };
}

function clearPendingEvmAction(order) {
  const orderId = order?.order_id || "";
  if (orderId) delete state.evm.pendingActions[orderId];
}
```

- [ ] **Step 3: Add the lifecycle panel renderer**

Add after `evmEscrowSellerReleaseAction(order)`:

```js
function evmEscrowStatusPanel(order, role) {
  if (!isEvmEscrowOrder(order)) return "";
  const confirmation = evmEscrowConfirmation(order);
  if (!confirmation) {
    return `<div class="evm-status-panel"><strong>Escrow intent pending</strong><span class="muted-text">Seller must create the EVM payment intent before wallet actions are available.</span></div>`;
  }
  const pending = pendingEvmAction(order.order_id);
  const lifecycle = evmLifecycleState({ order, pendingAction: pending, watcher: state.evm.watcher });
  const txHash = pendingEvmTxHash(pending);
  const rows = evmPaymentStatusRows({
    confirmation,
    watcher: state.evm.watcher,
    network: evmNetworkConfig(confirmation),
    txHash
  });
  const rowMarkup = rows.map((row) => {
    const value = row.href
      ? `<a class="mono" href="${esc(row.href)}" target="_blank" rel="noreferrer">${esc(row.value)}</a>`
      : `<span class="mono">${esc(row.value)}</span>`;
    return `<div class="evm-status-row"><span>${esc(row.label)}</span>${value}</div>`;
  }).join("");
  const roleLabel = role ? `<span class="badge muted-badge">${esc(role)}</span>` : "";
  return `<section class="evm-status-panel" data-evm-lifecycle-state="${esc(lifecycle.state)}">
    <div class="evm-status-head">
      <div><strong>${esc(lifecycle.label)}</strong><p>${esc(lifecycle.detail)}</p></div>
      ${roleLabel}
    </div>
    <div class="evm-status-grid">${rowMarkup}</div>
  </section>`;
}
```

- [ ] **Step 4: Add the panel to order cards**

In `renderOrders`, replace card body construction:

```js
const lifecyclePanel = evmEscrowStatusPanel(order, columns === 5 ? "seller" : "buyer");
return `<article class="order-card"><div class="section-head compact-head"><div><p class="eyebrow">${esc(actor)}</p><h3>${esc(title)}</h3><p class="mono">${esc(offer)}</p></div>${statusBadge(order.status)}</div>${orderTimeline(order)}${lifecyclePanel}${walletAction}${sellerActions}</article>`;
```

- [ ] **Step 5: Fetch watcher status for admin and reuse it for panels**

Add a helper:

```js
async function refreshEvmWatcherStatus({ silent = true } = {}) {
  const result = await api("/admin/evm-escrow/status", {
    tokenRole: "admin",
    action: "GET /admin/evm-escrow/status",
    silent,
    result: false
  });
  if (result.ok && result.body && result.body.enabled) {
    state.evm.watcher = {
      last_scan: result.body.runtime?.last_scan || result.body.last_scan || null,
      last_error: result.body.runtime?.last_error || result.body.last_error || null
    };
  }
  return result;
}
```

Call it from `refreshAdmin` before rendering admin summary:

```js
await refreshEvmWatcherStatus({ silent: true });
```

If `refreshAdmin` is not `async`, convert it to `async function refreshAdmin({ silent = true } = {})`.

- [ ] **Step 6: Add compact CSS**

Append to `crates/morpheus-server/ui/src/input.css`:

```css
.evm-status-panel {
  display: grid;
  gap: 10px;
  margin: 12px 16px;
  padding: 12px;
  border: 1px solid #d9e1ec;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.68);
}

.admin-theme .evm-status-panel {
  border-color: #27364b;
  background: rgba(9, 19, 33, 0.72);
}

.evm-status-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.evm-status-head p {
  margin: 4px 0 0;
  color: #6f7f92;
  font-size: 12px;
}

.admin-theme .evm-status-head p {
  color: #8ea0b7;
}

.evm-status-grid {
  display: grid;
  gap: 6px;
}

.evm-status-row {
  display: grid;
  grid-template-columns: minmax(96px, 150px) minmax(0, 1fr);
  gap: 10px;
  align-items: baseline;
  font-size: 12px;
}

.evm-status-row > span:first-child {
  color: #6f7f92;
  font-weight: 760;
}

.admin-theme .evm-status-row > span:first-child {
  color: #8ea0b7;
}

.evm-status-row .mono {
  overflow-wrap: anywhere;
}
```

- [ ] **Step 7: Build CSS and run UI tests**

Run:

```bash
npm run build:ui-css
npm run test:ui-wallet
```

Expected: both commands PASS.

- [ ] **Step 8: Commit Task 3**

```bash
git add crates/morpheus-server/ui/src/app.js crates/morpheus-server/ui/src/input.css crates/morpheus-server/ui/assets/app.css
git commit -m "Render EVM escrow lifecycle status"
```

---

### Task 4: Make Wallet Actions Explicit And Non-Final

**Files:**
- Modify: `crates/morpheus-server/ui/src/app.js`

- [ ] **Step 1: Add wallet result handling helpers**

Add after `rememberPendingEvmAction`:

```js
function showWalletSubmitted(actionLabel, order, kind, result) {
  rememberPendingEvmAction(order, kind, result);
  showResult(actionLabel, "submitted_waiting_for_watcher", result);
  toast("Transaction submitted", "success", "Waiting for Morpheus watcher confirmation.");
  if (document.body.classList.contains("buyer-theme")) renderOrders("buyer-orders-rows", "buyer-order-count", 3);
  if (document.body.classList.contains("seller-theme")) renderOrders("seller-orders-rows", "seller-order-count", 5);
}

function showWalletFailure(actionLabel, error) {
  const classified = classifyWalletError(error);
  showResult(actionLabel, classified.code, { error: classified.message });
  toast(classified.code === "wallet_rejected" ? "Wallet request rejected" : "Wallet action failed", "error", classified.message);
}

function confirmRefundSigning(mode, buyerAmountUnits) {
  if (mode === "partial") {
    return window.confirm(`Submit partial refund for buyer amount units ${buyerAmountUnits}? Morpheus will wait for watcher evidence before marking it final.`);
  }
  return window.confirm("Submit full escrow refund? Morpheus will wait for watcher evidence before marking it final.");
}
```

- [ ] **Step 2: Update buyer deposit event handler**

Replace the deposit handler body:

```js
const order = state.orders.find((item) => item.order_id === evmDeposit.dataset.orderId);
requestEvmEscrowDeposit(order)
  .then((result) => showWalletSubmitted("EVM escrow deposit", order, "deposit", result))
  .catch((error) => showWalletFailure("EVM escrow deposit", error));
return;
```

- [ ] **Step 3: Update seller release event handler**

Replace the release handler body:

```js
const order = state.orders.find((item) => item.order_id === evmRelease.dataset.orderId);
requestEvmEscrowRelease(order)
  .then((result) => showWalletSubmitted("EVM escrow release", order, "release", result))
  .catch((error) => showWalletFailure("EVM escrow release", error));
return;
```

- [ ] **Step 4: Update admin refund submit handler**

Replace the refund submit handler body after validation:

```js
const order = await fetchAdminOrder(data.order_id || DEMO.orderId);
if (!confirmRefundSigning(mode, data.buyer_amount_units)) {
  showResult(action, "wallet_rejected", { error: "Refund signing was cancelled before wallet submission." });
  return;
}
const result = mode === "partial"
  ? await requestEvmEscrowPartialRefund(order, data.buyer_amount_units)
  : await requestEvmEscrowRefund(order);
showWalletSubmitted(action, order, mode === "partial" ? "partial_refund" : "refund", result);
```

Keep the catch block but replace its body:

```js
showWalletFailure(action, error);
```

- [ ] **Step 5: Run UI unit tests**

Run:

```bash
npm run test:ui-wallet
```

Expected: PASS.

- [ ] **Step 6: Commit Task 4**

```bash
git add crates/morpheus-server/ui/src/app.js
git commit -m "Track pending EVM wallet actions"
```

---

### Task 5: Expand Browser Lifecycle Smoke Test

**Files:**
- Modify: `scripts/e2e/ui-wallet-flow.mjs`

- [ ] **Step 1: Replace static order routing with mutable lifecycle state**

In `scripts/e2e/ui-wallet-flow.mjs`, add mutable test state after `confirmation`:

```js
const lifecycle = {
  buyerStatus: "payment_intent_created",
  sellerStatus: "payment_authorized",
  adminOrderStatus: "payment_authorized",
  watcher: { last_scan: { status: "ok", to_block: 42 }, last_error: null },
  walletAccount: "0x0000000000000000000000000000000000000004",
  walletReject: false,
  chainSwitchReject: false
};
```

Update route handlers:

```js
await page.route("**/api/v1/buyer/orders", (route) => route.fulfill({
  contentType: "application/json",
  body: JSON.stringify({ orders: [evmOrder(lifecycle.buyerStatus)] })
}));
await page.route("**/api/v1/seller/orders", (route) => route.fulfill({
  contentType: "application/json",
  body: JSON.stringify({ orders: [evmOrder(lifecycle.sellerStatus)] })
}));
await page.route("**/admin/orders/*", (route) => route.fulfill({
  contentType: "application/json",
  body: JSON.stringify({ order: evmOrder(lifecycle.adminOrderStatus) })
}));
await page.route("**/admin/evm-escrow/status", (route) => route.fulfill({
  contentType: "application/json",
  body: JSON.stringify({
    enabled: true,
    chain_id: confirmation.chain_id,
    escrow_contract: confirmation.escrow_contract,
    confirmations: 5,
    runtime: lifecycle.watcher
  })
}));
```

- [ ] **Step 2: Make the mocked wallet support role changes and errors**

Replace the `page.addInitScript` wallet mock:

```js
await page.addInitScript(() => {
  window.__morpheusWalletRequests = [];
  window.__morpheusWalletWrites = [];
  window.__morpheusWalletState = {
    account: "0x0000000000000000000000000000000000000004",
    reject: false,
    chainReject: false
  };
  window.ethereum = {
    request: async (payload) => {
      window.__morpheusWalletRequests.push(payload);
      if (window.__morpheusWalletState.reject) throw new Error("User rejected the request.");
      if (payload.method === "wallet_switchEthereumChain") {
        if (window.__morpheusWalletState.chainReject) throw new Error("wallet_switchEthereumChain failed");
        return null;
      }
      if (payload.method === "eth_requestAccounts") return [window.__morpheusWalletState.account];
      return null;
    }
  };
});
```

- [ ] **Step 3: Assert buyer deposit panel and non-final pending state**

After clicking `[data-evm-escrow-deposit]`, assert:

```js
assert.match(await page.locator("#result-panel").innerText(), /submitted_waiting_for_watcher/);
assert.match(await page.locator("#buyer-order-cards").innerText(), /Deposit submitted/);
assert.match(await page.locator("#buyer-order-cards").innerText(), /Waiting for Morpheus watcher confirmation/);
```

Then simulate watcher-projected funded state:

```js
lifecycle.buyerStatus = "payment_authorized";
await page.reload();
await page.waitForSelector("[data-evm-lifecycle-state='escrow_funded']");
assert.match(await page.locator("#buyer-order-cards").innerText(), /Escrow funded/);
```

- [ ] **Step 4: Assert seller release flow**

Before opening seller:

```js
await page.evaluate(() => { window.__morpheusWalletState.account = "0x0000000000000000000000000000000000000003"; });
```

After clicking release:

```js
await page.locator("[data-evm-escrow-release]").dispatchEvent("click", { bubbles: true });
await page.waitForSelector("#result-panel", { state: "attached" });
assert.match(await page.locator("#result-panel").innerText(), /submitted_waiting_for_watcher/);
assert.match(await page.locator("#seller-order-cards").innerText(), /Release submitted/);
```

- [ ] **Step 5: Assert admin full refund and partial refund confirmations**

Before admin refund tests:

```js
await page.evaluate(() => {
  window.__morpheusWalletState.account = "0x0000000000000000000000000000000000000005";
  window.confirm = () => true;
});
```

Submit full refund:

```js
await page.locator("[data-refund-mode='full']").click();
await page.waitForSelector("#result-panel", { state: "attached" });
assert.match(await page.locator("#result-panel").innerText(), /EVM escrow refund/);
assert.match(await page.locator("#result-panel").innerText(), /submitted_waiting_for_watcher/);
```

Submit partial refund:

```js
await page.fill('[data-form="evm-arbiter-refund"] [name="buyer_amount_units"]', "10000000");
await page.locator("[data-refund-mode='partial']").click();
await page.waitForSelector("#result-panel", { state: "attached" });
assert.match(await page.locator("#result-panel").innerText(), /EVM escrow partial refund/);
assert.match(await page.locator("#result-panel").innerText(), /submitted_waiting_for_watcher/);
```

- [ ] **Step 6: Assert chain mismatch and wallet rejection states**

Add a second buyer page check:

```js
await routeHtml(page, "buyer");
await page.goto(`${baseUrl}/crates/morpheus-server/ui/buyer.html`);
await page.evaluate(() => {
  window.__morpheusWalletState.account = "0x0000000000000000000000000000000000000004";
  window.__morpheusWalletState.chainReject = true;
});
await page.waitForSelector("[data-evm-escrow-deposit]", { state: "attached" });
await page.locator("[data-evm-escrow-deposit]").dispatchEvent("click", { bubbles: true });
await page.waitForSelector("#result-panel", { state: "attached" });
assert.match(await page.locator("#result-panel").innerText(), /chain_mismatch/);

await page.evaluate(() => {
  window.__morpheusWalletState.chainReject = false;
  window.__morpheusWalletState.reject = true;
});
await page.locator("[data-evm-escrow-deposit]").dispatchEvent("click", { bubbles: true });
assert.match(await page.locator("#result-panel").innerText(), /wallet_rejected/);
```

- [ ] **Step 7: Run browser lifecycle test**

Run:

```bash
npm run test:ui-wallet-flow
```

Expected: PASS. If Vite local server binding is blocked by sandboxing, rerun with approval for the same command and record that reason in the final implementation notes.

- [ ] **Step 8: Commit Task 5**

```bash
git add scripts/e2e/ui-wallet-flow.mjs
git commit -m "Expand EVM escrow wallet lifecycle smoke"
```

---

### Task 6: Document Manual Public Testnet Rehearsal

**Files:**
- Modify: `docs/evm-escrow-production-runbook.md`

- [ ] **Step 1: Add manual browser rehearsal section**

Append after the existing `### Testnet Drill` section:

```markdown
### Manual Browser Testnet Rehearsal

Use this rehearsal after `make testnet-evm-escrow` passes and before any
real-money production pilot.

Required setup:

- Public testnet RPC in `MORPHEUS_EVM_RPC_URL`.
- Runtime config with `[payments.evm_escrow].enabled = true`.
- Testnet `chain_id`, `escrow_contract`, token contract, decimals, and
  confirmations pinned in config.
- Browser wallet funded with testnet native gas token.
- Buyer, seller, and arbiter wallet addresses configured in the UI settings.
- Optional `evm_escrow.explorer_base_url` in UI config for transaction links.

Rehearsal path:

1. Open the seller UI and accept a test order.
2. Create the EVM escrow payment intent.
3. Open the buyer UI with the buyer wallet selected.
4. Confirm the status panel shows the expected chain, token, escrow contract,
   amount, order hash, and watcher state.
5. Click **Approve and deposit**.
6. Confirm the UI shows submitted transaction state and does not mark escrow as
   funded yet.
7. Wait for watcher confirmation and refresh orders.
8. Confirm the UI shows escrow funded from Morpheus payment state.
9. Open the seller UI with the seller wallet selected.
10. Click **Release escrow**.
11. Confirm the UI shows release submitted, then payment captured only after
    watcher evidence.
12. Repeat the flow with a new order for full refund using the arbiter wallet.
13. Repeat the flow with a new order for partial refund and verify buyer/seller
    amounts before signing.

Failure drills:

- Switch the browser wallet to the wrong chain and verify the UI reports
  `chain_mismatch`.
- Connect the wrong role wallet and verify the UI blocks signing.
- Reject the wallet signature and verify the UI reports `wallet_rejected`.
- Stop or misconfigure the watcher and verify submitted transactions remain
  pending instead of becoming final payment state.

Evidence to keep:

- Order id and payment id.
- Chain id and escrow contract.
- Deposit, release, refund, and partial refund transaction hashes.
- `/admin/evm-escrow/status` response after each phase.
- Explorer links for signed transactions.
```

- [ ] **Step 2: Verify documentation renders as plain Markdown**

Run:

```bash
rg -n "Manual Browser Testnet Rehearsal|wallet_rejected|chain_mismatch" docs/evm-escrow-production-runbook.md
```

Expected: all three terms are found.

- [ ] **Step 3: Commit Task 6**

```bash
git add docs/evm-escrow-production-runbook.md
git commit -m "Document browser testnet payment rehearsal"
```

---

### Task 7: Full Verification

**Files:**
- No new files. This task verifies the integrated change set.

- [ ] **Step 1: Run UI unit tests**

```bash
npm run test:ui-wallet
```

Expected: PASS.

- [ ] **Step 2: Run browser wallet lifecycle smoke**

```bash
npm run test:ui-wallet-flow
```

Expected: PASS.

- [ ] **Step 3: Build UI**

```bash
npm run build:ui
```

Expected: PASS.

- [ ] **Step 4: Run focused Rust payment tests**

```bash
cargo test -p morpheus-server --test evm_escrow_adapter --test evm_escrow_watcher --test evm_rpc
```

Expected: PASS.

- [ ] **Step 5: Run local EVM escrow E2E**

```bash
make e2e-evm-escrow
```

Expected: PASS. Docker Compose orphan warnings are acceptable if the command exits 0.

- [ ] **Step 6: Check formatting whitespace**

```bash
git diff --check
```

Expected: no output and exit 0.

- [ ] **Step 7: Commit verification fixes only when a concrete file changed**

If Step 1 through Step 6 reveal a small deterministic fix, make the fix, rerun the failing command, then inspect the changed files:

```bash
git status --short
git diff --stat
```

Stage only files changed by that fix. For example, if the fix touches the lifecycle helper and browser smoke test:

```bash
git add crates/morpheus-server/ui/src/evmPaymentLifecycle.js scripts/e2e/ui-wallet-flow.mjs
git commit -m "Fix EVM escrow payment UX verification"
```

If all checks pass without changes, do not create an empty commit.

---

## Self-Review

Spec coverage:

- Buyer deposit flow: Task 3 renders status/actions, Task 4 tracks pending state, Task 5 tests deposit.
- Seller release flow: Task 3 renders release status, Task 4 tracks release pending, Task 5 tests release.
- Admin refund and partial refund: Task 4 adds explicit confirmation, Task 5 tests both modes.
- Shared status panel: Task 1 defines status rows, Task 3 renders them.
- Watcher state and non-final submitted tx: Task 1 state model, Task 3 panel, Task 4 pending action handling, Task 5 assertions.
- Explorer links: Task 1 helper, Task 3 renderer, Task 6 manual runbook.
- Manual testnet rehearsal: Task 6.
- Acceptance commands: Task 7.

Completeness scan:

- The plan has no unresolved future-work markers.
- Every code-changing step includes concrete code or an exact replacement block.
- Verification commands include expected outcomes.

Type consistency:

- `pendingAction.kind` values are `deposit`, `release`, `refund`, and `partial_refund`.
- Lifecycle states match the approved spec names.
- Helper names imported in `app.js` match exports from `evmPaymentLifecycle.js` and `evmWallet.js`.
