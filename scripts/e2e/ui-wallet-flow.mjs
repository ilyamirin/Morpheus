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
    body: JSON.stringify({ orders: [evmOrder("payment_intent_created")] })
  }));
  await page.route("**/api/v1/seller/orders", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ orders: [evmOrder("payment_authorized")] })
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
  await page.route("**/api/v1/buyer/orders/*", (route) => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ order: evmOrder("payment_intent_created") })
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
    await routeApi(page);
    await routeHtml(page, "buyer");
    await page.addInitScript(() => {
      window.__morpheusWalletRequests = [];
      window.ethereum = {
        request: async (payload) => {
          window.__morpheusWalletRequests.push(payload);
          if (payload.method === "wallet_switchEthereumChain") return null;
          if (payload.method === "eth_requestAccounts") {
            return ["0x0000000000000000000000000000000000000004"];
          }
          return null;
        }
      };
    });
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/buyer.html`);
    await page.waitForSelector("[data-evm-escrow-deposit]", { state: "attached" });
    await page.locator("[data-evm-escrow-deposit]").dispatchEvent("click", { bubbles: true });
    await page.waitForSelector("#result-panel", { state: "attached" });
    const walletRequestMethods = await page.evaluate(() => window.__morpheusWalletRequests.map((request) => request.method));
    assert(walletRequestMethods.includes("wallet_switchEthereumChain"));
    assert.equal(await page.locator("[data-evm-escrow-deposit]").count(), 1);
    assert.match(await page.locator("#buyer-order-cards").innerText(), /Deposit window: 15 min/);
    assert.match(await page.locator("#result-panel").innerText(), /EVM escrow deposit/);

    await routeHtml(page, "seller");
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/seller.html`);
    await page.waitForSelector("[data-evm-escrow-release]", { state: "attached" });
    assert.equal(await page.locator("[data-evm-escrow-release]").count(), 1);

    await routeHtml(page, "admin");
    await page.goto(`${baseUrl}/crates/morpheus-server/ui/admin.html`);
    await page.waitForSelector("[data-form='evm-arbiter-refund']");
    assert.equal(await page.locator("[data-refund-mode='full']").count(), 1);
    assert.equal(await page.locator("[data-refund-mode='partial']").count(), 1);
  } finally {
    await browser.close();
    await server.close();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
