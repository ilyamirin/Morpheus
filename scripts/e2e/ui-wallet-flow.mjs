#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";
import { createServer } from "vite";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const confirmation = {
  method: "evm_escrow_deposit",
  adapter: "evm_escrow",
  chain_id: 31337,
  token: "0x0000000000000000000000000000000000000002",
  token_decimals: 6,
  amount_units: "25000000",
  escrow_contract: "0x0000000000000000000000000000000000000001",
  order_hash: "0x1111111111111111111111111111111111111111111111111111111111111111",
  buyer_evm_address: "0x0000000000000000000000000000000000000004",
  seller_evm_address: "0x0000000000000000000000000000000000000003",
  arbiter_evm_address: "0x0000000000000000000000000000000000000005",
  policy: {
    deposit_timeout_secs: 900,
    buyer_review_timeout_secs: 3600
  },
  fee_hint: {
    estimated_fee_units: "125000000000000",
    fee_token_symbol: "ETH"
  }
};

const lifecycle = {
  buyerStatus: "payment_intent_created",
  sellerStatus: "payment_authorized",
  adminOrderStatus: "payment_authorized",
  watcher: { last_scan: { status: "ok", to_block: 42 }, last_error: null },
  walletAccount: "0x0000000000000000000000000000000000000004",
  walletReject: false,
  chainSwitchReject: false
};

const evmOrder = (status) => ({
  order_id: "ord:local.example:01JEVMORDER",
  offer_id: "offer:local.example:01JOFFER",
  customer_id: "customer:local.example:01JCUST",
  seller_id: "seller:local.example:01JSELLER",
  room_id: "!order:local.example",
  status,
  body: {
    order_id: "ord:local.example:01JEVMORDER",
    offer_id: "offer:local.example:01JOFFER",
    customer_id: "customer:local.example:01JCUST",
    seller_id: "seller:local.example:01JSELLER",
    payment_adapter: "evm_escrow",
    price: { amount: "25.00", currency: "USDC" }
  },
  payment: {
    status,
    body: {
      adapter: "evm_escrow",
      amount: "25.00",
      currency: "USDC",
      confirmation
    }
  }
});

async function routeApi(page) {
  await page.route("**/api/v1/catalog/sellers", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ items: [] })
  }));
  await page.route("**/api/v1/catalog/products", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ items: [] })
  }));
  await page.route("**/api/v1/catalog/offers", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ items: [] })
  }));
  await page.route("**/api/v1/buyer/orders", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ orders: [evmOrder(lifecycle.buyerStatus)] })
  }));
  await page.route("**/api/v1/seller/orders", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ orders: [evmOrder(lifecycle.sellerStatus)] })
  }));
  await page.route("**/healthz", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ status: "ok" })
  }));
  await page.route("**/readyz", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ status: "ready" })
  }));
  await page.route("**/admin/config", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ instance_id: "local.example" })
  }));
  await page.route("**/admin/allowlist", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ entries: [] })
  }));
  await page.route("**/admin/projections/summary", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ catalog: { sellers: 0, products: 0, offers: 0 }, orders: 1, payments: 1 })
  }));
  await page.route("**/admin/events", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ events: [] })
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
      watcher: lifecycle.watcher
    })
  }));
  await page.route("**/api/v1/buyer/orders/*", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ order: evmOrder(lifecycle.buyerStatus) })
  }));
}

async function routeHtml(page, role) {
  const htmlPath = resolve(ROOT, `crates/morpheus-server/ui/${role}.html`);
  const html = await readFile(htmlPath, "utf8");
  const moduleHtml = html.replace(
    /<script src="\/ui\/assets\/app\.bundle\.js" defer><\/script>/,
    '<script type="module" src="/crates/morpheus-server/ui/src/app.js"></script>'
  );
  await page.route(`**/crates/morpheus-server/ui/${role}.html`, (route) => route.fulfill({
    contentType: "text/html",
    body: moduleHtml
  }));
}

async function installWalletMock(page) {
  await page.addInitScript((initialWalletState) => {
    window.__morpheusWalletRequests = [];
    window.__morpheusWalletWrites = [];
    window.__morpheusWalletState = {
      account: initialWalletState.account,
      reject: initialWalletState.reject,
      chainReject: initialWalletState.chainReject
    };
    window.__morpheusWalletTxCounter = 0;
    window.ethereum = {
      request: async (payload) => {
        window.__morpheusWalletRequests.push(payload);
        if (window.__morpheusWalletState.reject) throw new Error("User rejected the request.");
        if (payload.method === "wallet_switchEthereumChain") {
          if (window.__morpheusWalletState.chainReject) throw new Error("wallet_switchEthereumChain failed");
          return null;
        }
        if (payload.method === "eth_requestAccounts" || payload.method === "eth_accounts") {
          return [window.__morpheusWalletState.account];
        }
        if (payload.method === "eth_chainId") return "0x7a69";
        if (payload.method === "eth_sendTransaction" || payload.method === "wallet_sendTransaction") {
          window.__morpheusWalletTxCounter += 1;
          const txHash = `0x${String(window.__morpheusWalletTxCounter).padStart(64, "0")}`;
          window.__morpheusWalletWrites.push({ payload, txHash });
          return txHash;
        }
        return null;
      }
    };
  }, {
    account: lifecycle.walletAccount,
    reject: lifecycle.walletReject,
    chainReject: lifecycle.chainSwitchReject
  });
}

async function setWalletState(page, state) {
  await page.evaluate((updates) => {
    Object.assign(window.__morpheusWalletState, updates);
  }, state);
}

async function walletRequestMethods(page) {
  return page.evaluate(() => window.__morpheusWalletRequests.map((request) => request.method));
}

async function walletWriteCount(page) {
  return page.evaluate(() => window.__morpheusWalletWrites.length);
}

async function waitForResult(page, expectedText) {
  try {
    await page.waitForFunction((text) => {
      return document.querySelector("#result-panel")?.innerText.includes(text);
    }, expectedText);
  } catch (error) {
    const panelText = await page.locator("#result-panel").innerText().catch(() => "<missing result panel>");
    const methods = await page.evaluate(() => window.__morpheusWalletRequests?.map((request) => request.method) || []).catch(() => []);
    const writes = await page.evaluate(() => window.__morpheusWalletWrites?.length || 0).catch(() => 0);
    throw new Error(`${error.message}\nExpected result text: ${expectedText}\nResult panel: ${panelText}\nWallet methods: ${methods.join(", ")}\nWallet writes: ${writes}`);
  }
  return page.locator("#result-panel").innerText();
}

async function main() {
  const server = await createServer({
    root: ROOT,
    configFile: false,
    server: { host: "127.0.0.1", port: 0 }
  });
  await server.listen();
  const baseUrl = server.resolvedUrls.local[0].replace(/\/$/, "");
  const browser = await chromium.launch({ headless: true });

  try {
    const page = await browser.newPage();
    page.on("pageerror", (error) => {
      console.error(`page error: ${error.message}`);
    });
    await routeApi(page);
    await routeHtml(page, "buyer");
    await installWalletMock(page);
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/buyer.html`);
    await page.waitForSelector("[data-evm-escrow-deposit]", { state: "attached" });
    await page.locator("[data-evm-escrow-deposit]").dispatchEvent("click", { bubbles: true });
    assert.match(await waitForResult(page, "submitted_waiting_for_watcher"), /submitted_waiting_for_watcher/);
    const buyerWalletRequestMethods = await walletRequestMethods(page);
    assert(buyerWalletRequestMethods.includes("wallet_switchEthereumChain"));
    assert(buyerWalletRequestMethods.includes("eth_sendTransaction"));
    assert.equal(await walletWriteCount(page), 2);
    assert.equal(await page.locator("[data-evm-escrow-deposit]").count(), 1);
    assert.match(await page.locator("#buyer-order-cards").innerText(), /Deposit window: 15 min/);
    assert.match(await page.locator("#result-panel").innerText(), /EVM escrow deposit/);
    await page.locator('[data-action="buyer-orders"]').dispatchEvent("click", { bubbles: true });
    await page.waitForSelector("[data-evm-lifecycle-state='deposit_submitted']", { state: "attached" });
    assert.match(await page.locator("#buyer-order-cards").innerText(), /Deposit submitted/);
    assert.match(await page.locator("#buyer-order-cards").innerText(), /Waiting for Morpheus watcher confirmation/);

    lifecycle.buyerStatus = "payment_authorized";
    await page.reload();
    await page.waitForSelector("[data-evm-lifecycle-state='escrow_funded']", { state: "attached" });
    assert.match(await page.locator("#buyer-order-cards").innerText(), /Escrow funded/);

    // Buyer and seller pages do not fetch the admin-only watcher status endpoint,
    // so watcher_lagging is intentionally not asserted in those role flows.

    await routeHtml(page, "seller");
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/seller.html`);
    await page.waitForSelector("[data-evm-escrow-release]", { state: "attached" });
    await setWalletState(page, { account: "0x0000000000000000000000000000000000000003" });
    assert.equal(await page.locator("[data-evm-escrow-release]").count(), 1);
    await page.locator("[data-evm-escrow-release]").dispatchEvent("click", { bubbles: true });
    assert.match(await waitForResult(page, "submitted_waiting_for_watcher"), /submitted_waiting_for_watcher/);
    await page.locator('[data-action="seller-orders"]').dispatchEvent("click", { bubbles: true });
    await page.waitForSelector("[data-evm-lifecycle-state='release_submitted']", { state: "attached" });
    assert.match(await page.locator("#seller-orders-rows-cards").innerText(), /Release submitted/);
    assert.equal(await walletWriteCount(page), 1);

    await routeHtml(page, "admin");
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/admin.html`);
    await page.waitForSelector("[data-form='evm-arbiter-refund']");
    await setWalletState(page, { account: "0x0000000000000000000000000000000000000005" });
    await page.evaluate(() => {
      window.confirm = () => true;
    });
    assert.equal(await page.locator("[data-refund-mode='full']").count(), 1);
    assert.equal(await page.locator("[data-refund-mode='partial']").count(), 1);
    await page.locator("[data-refund-mode='full']").click();
    assert.match(await waitForResult(page, "submitted_waiting_for_watcher"), /EVM escrow refund/);
    assert.match(await page.locator("#result-panel").innerText(), /submitted_waiting_for_watcher/);
    assert.equal(await walletWriteCount(page), 1);

    await page.fill('[data-form="evm-arbiter-refund"] [name="buyer_amount_units"]', "10000000");
    await page.locator("[data-refund-mode='partial']").click();
    assert.match(await waitForResult(page, "EVM escrow partial refund"), /EVM escrow partial refund/);
    assert.match(await page.locator("#result-panel").innerText(), /submitted_waiting_for_watcher/);
    assert.equal(await walletWriteCount(page), 2);

    const writesBeforeCancel = await walletWriteCount(page);
    await page.evaluate(() => {
      window.confirm = () => false;
    });
    await page.locator("[data-refund-mode='full']").click();
    assert.match(await waitForResult(page, "wallet_rejected"), /wallet_rejected/);
    assert.equal(await walletWriteCount(page), writesBeforeCancel);

    lifecycle.buyerStatus = "payment_intent_created";
    await routeHtml(page, "buyer");
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/buyer.html`);
    await page.waitForSelector("[data-evm-escrow-deposit]", { state: "attached" });
    await setWalletState(page, {
      account: "0x0000000000000000000000000000000000000004",
      chainReject: true,
      reject: false
    });
    await page.locator("[data-evm-escrow-deposit]").dispatchEvent("click", { bubbles: true });
    assert.match(await waitForResult(page, "chain_mismatch"), /chain_mismatch/);

    await setWalletState(page, {
      chainReject: false,
      reject: true
    });
    await page.locator("[data-evm-escrow-deposit]").dispatchEvent("click", { bubbles: true });
    assert.match(await waitForResult(page, "wallet_rejected"), /wallet_rejected/);
  } finally {
    await browser.close();
    await server.close();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
