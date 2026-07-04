(function () {
  "use strict";

  const HASH_A = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const SELLER_TERMS_HASH = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
  const OFFER_TERMS_HASH = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
  const UI_CONFIG = readUiConfig();
  const LOCAL_INSTANCE = UI_CONFIG.instance_id || "local.example";
  const PAGE_NONCE = Date.now().toString(36).toUpperCase();
  const DEMO = {
    sellerId: protocolId("seller", LOCAL_INSTANCE, "SELLER01"),
    productId: protocolId("prod", LOCAL_INSTANCE, `PROD_${PAGE_NONCE}`),
    offerId: protocolId("offer", LOCAL_INSTANCE, `OFFER_${PAGE_NONCE}`),
    customerId: protocolId("customer", LOCAL_INSTANCE, "CUSTOMER01"),
    orderId: protocolId("ord", LOCAL_INSTANCE, `ORDER_${PAGE_NONCE}`),
    paymentId: protocolId("pay", LOCAL_INSTANCE, `PAY_${PAGE_NONCE}`),
    entitlementId: protocolId("ent", LOCAL_INSTANCE, `ENT_${PAGE_NONCE}`)
  };
  const PRODUCT_IMAGES = {
    books: "/ui/assets/products/books.png",
    cases: "/ui/assets/products/cases.png",
    sneakers: "/ui/assets/products/sneakers.png",
    clothing: "/ui/assets/products/clothing.png"
  };
  const SEEDED_PRODUCT_IMAGES = {
    "prod:books.example:BOOKSPROD0101": "/ui/assets/products/seed/booksprod0101.jpg",
    "prod:books.example:BOOKSPROD0102": "/ui/assets/products/seed/booksprod0102.jpg",
    "prod:books.example:BOOKSPROD0201": "/ui/assets/products/seed/booksprod0201.jpg",
    "prod:books.example:BOOKSPROD0202": "/ui/assets/products/seed/booksprod0202.jpg",
    "prod:books.example:BOOKSPROD0301": "/ui/assets/products/seed/booksprod0301.jpg",
    "prod:books.example:BOOKSPROD0302": "/ui/assets/products/seed/booksprod0302.jpg",
    "prod:books.example:BOOKSPROD0401": "/ui/assets/products/seed/booksprod0401.jpg",
    "prod:books.example:BOOKSPROD0402": "/ui/assets/products/seed/booksprod0402.jpg",
    "prod:books.example:BOOKSPROD0501": "/ui/assets/products/seed/booksprod0501.jpg",
    "prod:books.example:BOOKSPROD0502": "/ui/assets/products/seed/booksprod0502.jpg",
    "prod:cases.example:CASESPROD0101": "/ui/assets/products/seed/casesprod0101.jpg",
    "prod:cases.example:CASESPROD0102": "/ui/assets/products/seed/casesprod0102.jpg",
    "prod:cases.example:CASESPROD0201": "/ui/assets/products/seed/casesprod0201.jpg",
    "prod:cases.example:CASESPROD0202": "/ui/assets/products/seed/casesprod0202.jpg",
    "prod:cases.example:CASESPROD0301": "/ui/assets/products/seed/casesprod0301.jpg",
    "prod:cases.example:CASESPROD0302": "/ui/assets/products/seed/casesprod0302.jpg",
    "prod:cases.example:CASESPROD0401": "/ui/assets/products/seed/casesprod0401.jpg",
    "prod:cases.example:CASESPROD0402": "/ui/assets/products/seed/casesprod0402.jpg",
    "prod:fashion.example:FASHIONPROD0101": "/ui/assets/products/seed/fashionprod0101.jpg",
    "prod:fashion.example:FASHIONPROD0102": "/ui/assets/products/seed/fashionprod0102.jpg",
    "prod:fashion.example:FASHIONPROD0201": "/ui/assets/products/seed/fashionprod0201.jpg",
    "prod:fashion.example:FASHIONPROD0202": "/ui/assets/products/seed/fashionprod0202.jpg",
    "prod:fashion.example:FASHIONPROD0301": "/ui/assets/products/seed/fashionprod0301.jpg",
    "prod:fashion.example:FASHIONPROD0302": "/ui/assets/products/seed/fashionprod0302.jpg",
    "prod:fashion.example:FASHIONPROD0401": "/ui/assets/products/seed/fashionprod0401.jpg",
    "prod:fashion.example:FASHIONPROD0402": "/ui/assets/products/seed/fashionprod0402.jpg",
    "prod:fashion.example:FASHIONPROD0501": "/ui/assets/products/seed/fashionprod0501.jpg",
    "prod:fashion.example:FASHIONPROD0502": "/ui/assets/products/seed/fashionprod0502.jpg"
  };
  const state = {
    sellers: [],
    products: [],
    offers: [],
    orders: [],
    selectedOffer: null,
    pendingOrders: [],
    pendingListings: [],
    admin: { healthOk: false, readyOk: false, incidents: [], pendingMaintenance: null }
  };
  let resultPanel = null;

  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));
  const esc = (value) => String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
  const int = (value, fallback) => Number.isFinite(Number(value)) ? Number(value) : fallback;
  const pick = (value, path, fallback) => {
    let cursor = value;
    for (const key of path) {
      if (!cursor || typeof cursor !== "object" || !(key in cursor)) return fallback;
      cursor = cursor[key];
    }
    return cursor ?? fallback;
  };
  const displayId = (value, fallback = "not set") => String(value || fallback);
  const normalizeTitle = (value) => String(value || "")
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());

  function readUiConfig() {
    const node = document.getElementById("morpheus-ui-config");
    if (!node) return {};
    try {
      return JSON.parse(node.textContent || "{}");
    } catch (_error) {
      return {};
    }
  }

  function localId(seed) {
    const normalized = String(seed || "ID")
      .toUpperCase()
      .replace(/[^A-Z0-9]+/g, "_")
      .replace(/^_|_$/g, "") || "ID";
    return normalized.slice(0, 64).padEnd(3, "0");
  }

  function protocolId(kind, instance, seed) {
    return `${kind}:${instance || LOCAL_INSTANCE}:${localId(seed || kind)}`;
  }

  function freshDraftId(kind) {
    return protocolId(kind, LOCAL_INSTANCE, `${kind}_${Date.now().toString(36).toUpperCase()}`);
  }

  function objectInstance(id, fallback = LOCAL_INSTANCE) {
    const parts = String(id || "").split(":");
    return parts.length >= 2 && parts[1] ? parts[1] : fallback;
  }

  function orderScopedId(kind, orderId, fallback) {
    const parts = String(orderId || "").split(":");
    const seed = parts.length >= 3 ? parts[2] : fallback;
    return protocolId(kind, LOCAL_INSTANCE, seed || fallback);
  }

  function closestNamedItem(items, field, id) {
    if (!id) return null;
    return items.find((item) => item && item[field] === id) || null;
  }

  function initRoleTabs() {
    const tabs = $$(".role-tab");
    if (!tabs.length) return;
    const targetSections = tabs
      .map((tab) => {
        const hash = tab.getAttribute("href") || "";
        return hash.startsWith("#") ? document.getElementById(hash.slice(1)) : null;
      })
      .filter(Boolean);
    const activate = (tab, { scroll = false } = {}) => {
      const hash = tab.getAttribute("href") || "";
      const target = hash.startsWith("#") ? document.getElementById(hash.slice(1)) : null;
      tabs.forEach((item) => {
        const active = item === tab;
        item.classList.toggle("is-active", active);
        item.setAttribute("aria-current", active ? "page" : "false");
      });
      if (targetSections.length > 1) {
        targetSections.forEach((section) => {
          section.hidden = target ? section !== target : false;
        });
      }
      if (target && scroll) target.scrollIntoView({ behavior: "smooth", block: "start" });
    };
    tabs.forEach((tab) => {
      tab.addEventListener("click", (event) => {
        event.preventDefault();
        if (tab.hash) history.replaceState(null, "", tab.hash);
        activate(tab, { scroll: false });
      });
    });
    const initial = tabs.find((tab) => tab.hash && tab.hash === window.location.hash) || tabs[0];
    if (initial) activate(initial);
  }

  function setBuyerSettingsOpen(open) {
    const panel = $("[data-token-settings-panel]");
    const overlay = $("[data-token-settings-overlay]");
    const toggle = $("[data-token-settings-toggle]");
    if (!panel || !overlay) return;
    panel.hidden = !open;
    overlay.hidden = !open;
    document.body.classList.toggle("settings-open", open);
    if (toggle) toggle.setAttribute("aria-expanded", open ? "true" : "false");
  }

  function initBuyerSettings() {
    const toggle = $("[data-token-settings-toggle]");
    if (!toggle) return;
    toggle.addEventListener("click", () => setBuyerSettingsOpen(true));
    document.addEventListener("click", (event) => {
      if (event.target.closest("[data-token-settings-close]") || event.target.closest("[data-token-settings-overlay]")) {
        setBuyerSettingsOpen(false);
      }
      if (event.target.closest(".role-tab-debug")) {
        setBuyerSettingsOpen(false);
      }
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setBuyerSettingsOpen(false);
    });
  }

  function setSellerSettingsOpen(open) {
    const panel = $("[data-seller-settings-panel]");
    const overlay = $("[data-seller-settings-overlay]");
    const toggle = $("[data-seller-settings-toggle]");
    if (!panel || !overlay) return;
    panel.hidden = !open;
    overlay.hidden = !open;
    document.body.classList.toggle("settings-open", open);
    if (toggle) toggle.setAttribute("aria-expanded", open ? "true" : "false");
  }

  function initSellerSettings() {
    const toggle = $("[data-seller-settings-toggle]");
    if (!toggle) return;
    toggle.addEventListener("click", () => setSellerSettingsOpen(true));
    document.addEventListener("click", (event) => {
      if (event.target.closest("[data-seller-settings-close]") || event.target.closest("[data-seller-settings-overlay]")) {
        setSellerSettingsOpen(false);
      }
      if (event.target.closest(".role-tab-debug")) {
        setSellerSettingsOpen(false);
      }
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setSellerSettingsOpen(false);
    });
  }

  function setSellerQuickAddOpen(open) {
    const panel = $("[data-seller-quick-add-panel]");
    const overlay = $("[data-seller-quick-add-overlay]");
    const toggles = $$("[data-seller-quick-add-toggle]");
    if (!panel || !overlay) return;
    panel.hidden = !open;
    overlay.hidden = !open;
    document.body.classList.toggle("quick-add-open", open);
    toggles.forEach((toggle) => toggle.setAttribute("aria-expanded", open ? "true" : "false"));
  }

  function initSellerQuickAdd() {
    const toggles = $$("[data-seller-quick-add-toggle]");
    if (!toggles.length) return;
    toggles.forEach((toggle) => toggle.addEventListener("click", () => setSellerQuickAddOpen(true)));
    document.addEventListener("click", (event) => {
      if (event.target.closest("[data-seller-quick-add-close]") || event.target.closest("[data-seller-quick-add-overlay]")) {
        setSellerQuickAddOpen(false);
      }
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setSellerQuickAddOpen(false);
    });
  }

  function setAdminSettingsOpen(open) {
    const panel = $("[data-admin-settings-panel]");
    const overlay = $("[data-admin-settings-overlay]");
    const toggle = $("[data-admin-settings-toggle]");
    if (!panel || !overlay) return;
    if (open) setMaintenanceConfirm(false);
    panel.hidden = !open;
    overlay.hidden = !open;
    document.body.classList.toggle("settings-open", open);
    if (toggle) toggle.setAttribute("aria-expanded", open ? "true" : "false");
  }

  function initAdminSettings() {
    const toggle = $("[data-admin-settings-toggle]");
    if (!toggle) return;
    toggle.addEventListener("click", () => setAdminSettingsOpen(true));
    document.addEventListener("click", (event) => {
      if (event.target.closest("[data-admin-settings-close]") || event.target.closest("[data-admin-settings-overlay]")) {
        setAdminSettingsOpen(false);
      }
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setAdminSettingsOpen(false);
    });
  }

  function openAdminDebug() {
    const debugTab = $('.role-tab[href="#debug"]');
    if (debugTab) debugTab.click();
  }

  function setMaintenanceConfirm(open, detail = {}) {
    const panel = $("[data-maintenance-confirm-panel]");
    if (!panel) return;
    panel.hidden = !open;
    if (!open) {
      state.admin.pendingMaintenance = null;
      return;
    }
    state.admin.pendingMaintenance = detail;
    setText("maintenance-confirm-title", detail.title || "Confirm maintenance action");
    const detailNode = $("[data-maintenance-confirm-detail]");
    if (detailNode) detailNode.textContent = detail.message || "This operation can affect projections.";
    const titleNode = $("[data-maintenance-confirm-title]");
    if (titleNode) titleNode.textContent = detail.title || "Confirm maintenance action";
  }

  function initTokens() {
    $$("[data-token]").forEach((input) => {
      const role = input.dataset.token;
      const storageKey = `morpheus.ui.token.${role}`;
      const stored = localStorage.getItem(storageKey);
      if (stored) input.value = stored;
      const persist = () => {
        const value = input.value.trim();
        if (value) localStorage.setItem(storageKey, value);
        else localStorage.removeItem(storageKey);
      };
      input.addEventListener("input", persist);
      input.addEventListener("change", persist);
    });
    $$("[data-evm-address]").forEach((input) => {
      const name = input.dataset.evmAddress;
      const storageKey = `morpheus.ui.evm.${name}`;
      const stored = localStorage.getItem(storageKey);
      if (stored) input.value = stored;
      const persist = () => {
        const value = input.value.trim();
        if (value) localStorage.setItem(storageKey, value);
        else localStorage.removeItem(storageKey);
      };
      input.addEventListener("input", persist);
      input.addEventListener("change", persist);
    });
  }

  function token(role) {
    const input = $(`[data-token="${role}"]`);
    return (input && (input.value.trim() || input.placeholder)) || `${role}-token`;
  }

  function configuredEvmAddress(name) {
    const input = $(`[data-evm-address="${name}"]`);
    return input ? input.value.trim() : "";
  }

  function form(formEl) {
    return Object.fromEntries(new FormData(formEl).entries());
  }

  function setText(idOrKey, text) {
    const nodes = [];
    const byId = document.getElementById(idOrKey);
    if (byId) nodes.push(byId);
    $$(`[data-text="${idOrKey}"]`).forEach((node) => {
      if (!nodes.includes(node)) nodes.push(node);
    });
    nodes.forEach((node) => {
      node.textContent = text;
    });
  }

  function setSellerSyncStatus(text, accent = "accent-cyan") {
    const node = document.getElementById("seller-sync-status");
    if (!node) return;
    node.textContent = text;
    node.className = `status-pill ${accent}`;
  }

  function updateAdminOverallStatus() {
    const incidentCount = state.admin.incidents.length;
    const healthy = state.admin.healthOk && state.admin.readyOk && incidentCount === 0;
    const degraded = state.admin.healthOk && state.admin.readyOk && incidentCount > 0;
    const status = healthy ? "Healthy" : degraded ? "Degraded" : "Attention";
    const accent = healthy ? "accent-emerald" : degraded ? "accent-amber" : "accent-crimson";
    const pill = document.getElementById("admin-overall-status");
    if (pill) {
      pill.textContent = status;
      pill.className = `status-pill ${accent}`;
    }
    setText("admin-status-summary", healthy ? "Healthy" : degraded ? "Runtime ok, incidents open" : "Check runtime");
  }

  function showResult(action, status, response) {
    if (!resultPanel) return;
    resultPanel.textContent = JSON.stringify({
      action,
      status,
      received_at: new Date().toISOString(),
      response
    }, null, 2);
  }

  function toast(title, tone, detail) {
    const stack = document.getElementById("toast-stack");
    if (!stack) return;
    const item = document.createElement("div");
    item.className = `toast toast-${tone || "neutral"}`;
    item.innerHTML = `<strong>${esc(title)}</strong><span>${esc(detail)}</span>`;
    stack.prepend(item);
    window.setTimeout(() => item.remove(), 5200);
  }

  async function api(path, { method = "GET", tokenRole, body, action, silent = false, result = true } = {}) {
    const headers = {};
    const label = action || `${method} ${path}`;
    if (body !== undefined) headers["content-type"] = "application/json";
    if (tokenRole) headers.authorization = `Bearer ${token(tokenRole)}`;
    try {
      const response = await fetch(path, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const contentType = response.headers.get("content-type") || "";
      const text = await response.text();
      let responseBody = text;
      if (contentType.includes("application/json") && text) {
        try {
          responseBody = JSON.parse(text);
        } catch (error) {
          responseBody = { error: "Invalid JSON response", raw: text };
        }
      } else if (!text) {
        responseBody = null;
      }
      if (result) showResult(label, response.status, responseBody);
      if (!silent) toast(label, response.ok ? "success" : "error", `${response.status} ${response.statusText}`);
      return { ok: response.ok, status: response.status, body: responseBody };
    } catch (error) {
      const responseBody = { error: error.message, hint: "Server route may not be mounted yet." };
      if (result) showResult(label, "network-error", responseBody);
      if (!silent) toast(label, "error", error.message);
      return { ok: false, status: "network-error", body: responseBody };
    }
  }

  function silentGet(path, { tokenRole, action } = {}) {
    return api(path, { tokenRole, action, silent: true, result: false });
  }

  function currentSellerId() {
    const input = $('[data-form="seller-announce"] [name="seller_id"]');
    return (input && input.value.trim()) || DEMO.sellerId;
  }

  function sellerAnnounce(formEl) {
    const data = form(formEl);
    return {
      seller_id: data.seller_id || DEMO.sellerId,
      display_name: data.display_name || "Fixture Seller",
      legal_profile_ref: data.legal_profile_ref || `https://${LOCAL_INSTANCE}/legal`,
      terms_ref: data.terms_ref || `https://${LOCAL_INSTANCE}/terms`,
      terms_hash: HASH_A,
      supported_payment_adapters: ["mock"],
      supported_entitlement_types: ["external_entitlement"]
    };
  }

  function sellerProduct(formEl) {
    const data = form(formEl);
    const category = String(data.kind || "").trim();
    return {
      seller_id: currentSellerId(),
      product_id: data.product_id || DEMO.productId,
      revision: int(data.revision, 1),
      title: String(data.title || "").trim(),
      description: data.description || "Operator workspace with marketplace workflow controls.",
      kind: "digital_service",
      categories: [category, "marketplace"],
      tags: ["morpheus", "operator", "poc"],
      image_src: data.image_src || undefined,
      terms_hash: HASH_A
    };
  }

  function sellerOffer(formEl) {
    const data = form(formEl);
    return {
      seller_id: currentSellerId(),
      product_id: data.product_id || DEMO.productId,
      offer_id: data.offer_id || DEMO.offerId,
      revision: int(data.revision, 1),
      price: { amount: String(data.amount || "").trim(), currency: data.currency || "USD" },
      payment_capture_policy: data.payment_capture_policy || "before_entitlement",
      seller_terms_hash: SELLER_TERMS_HASH,
      offer_terms_hash: OFFER_TERMS_HASH,
      entitlement_type: "external_entitlement",
      availability_mode: "unlimited"
    };
  }

  function sellerOrder(step, orderId) {
    orderId = orderId ? decodeURIComponent(orderId) : orderId;
    const order = state.orders.find((item) => item.order_id === orderId) || {};
    const actorId = objectInstance(order.seller_id) === LOCAL_INSTANCE ? order.seller_id : currentSellerId();
    const amount = pick(order, ["body", "price", "amount"], "100.00");
    const currency = pick(order, ["body", "price", "currency"], "USD");
    const paymentId = orderId ? orderScopedId("pay", orderId, `PAY_${PAGE_NONCE}`) : DEMO.paymentId;
    const entitlementId = orderId ? orderScopedId("ent", orderId, `ENT_${PAGE_NONCE}`) : DEMO.entitlementId;
    const evidence = {
      kind: "seller-ui-poc",
      uri: `https://${LOCAL_INSTANCE}/evidence/seller-ui-poc`,
      sha256: OFFER_TERMS_HASH
    };
    if (step === "accept") {
      return {
        actor_id: actorId,
        offer_revision: 1,
        seller_terms_hash: SELLER_TERMS_HASH,
        offer_terms_hash: OFFER_TERMS_HASH,
        payment_capture_policy: "before_entitlement",
        arbitration_policy_version: "1"
      };
    }
    if (step === "payment-intent") {
      return {
        actor_id: actorId,
        payment_id: paymentId,
        adapter: "mock",
        amount,
        currency,
        capture_policy: "before_entitlement",
        idempotency_key: `idem:${LOCAL_INSTANCE}:${localId(paymentId)}`,
        provider_ref: `mock:pi_${localId(paymentId)}`,
        confirmation: { method: "redirect", uri: `https://${LOCAL_INSTANCE}/pay/confirm` },
        expires_at: "2026-05-06T10:30:00Z"
      };
    }
    if (step === "evm-payment-intent") {
      const buyerEvmAddress = evmEscrowAddress(order, "buyer_evm_address");
      const sellerEvmAddress = evmEscrowAddress(order, "seller_evm_address");
      const arbiterEvmAddress = evmEscrowAddress(order, "arbiter_evm_address");
      if (!buyerEvmAddress || !sellerEvmAddress || !arbiterEvmAddress) {
        throw new Error("EVM escrow addresses are required before requesting payment.");
      }
      return {
        actor_id: actorId,
        payment_id: paymentId,
        buyer_evm_address: buyerEvmAddress,
        seller_evm_address: sellerEvmAddress,
        arbiter_evm_address: arbiterEvmAddress
      };
    }
    if (step === "payment-capture") {
      return {
        actor_id: actorId,
        payment_id: paymentId,
        adapter: "mock",
        amount,
        currency,
        provider_ref: `mock:cap_${localId(paymentId)}`,
        evidence
      };
    }
    if (step === "entitlement-grant") {
      return {
        actor_id: actorId,
        payment_id: paymentId,
        entitlement_id: entitlementId,
        entitlement_type: "external_entitlement",
        external_ref: `https://${LOCAL_INSTANCE}/entitlements/${localId(entitlementId)}`,
        evidence
      };
    }
    return { actor_id: actorId };
  }

  function evmEscrowAddress(order, name) {
    return pick(order, ["body", name], "")
      || pick(order, ["payment", "body", "confirmation", name], "")
      || configuredEvmAddress(name);
  }

  function isEvmEscrowOrder(order) {
    return pick(order, ["body", "payment_adapter"], "") === "evm_escrow"
      || pick(order, ["payment", "body", "adapter"], "") === "evm_escrow";
  }

  function evmEscrowConfirmation(order) {
    return pick(order, ["payment", "body", "confirmation"], null)
      || pick(order, ["payment", "confirmation"], null)
      || pick(order, ["body", "payment_confirmation"], null)
      || pick(order, ["body", "confirmation"], null)
      || null;
  }

  function formatDurationHint(seconds) {
    if (!Number.isFinite(Number(seconds)) || Number(seconds) <= 0) return "";
    const value = Number(seconds);
    if (value % 3600 === 0) return `${value / 3600} h`;
    if (value % 60 === 0) return `${value / 60} min`;
    return `${value} sec`;
  }

  function feeHintTextValue(value, maxLength = 96) {
    const valueType = typeof value;
    if (valueType !== "string" && valueType !== "number" && valueType !== "bigint") return "";
    if (valueType === "number" && !Number.isFinite(value)) return "";
    const text = String(value).trim();
    if (!text) return "";
    return text.length > maxLength ? `${text.slice(0, maxLength - 3)}...` : text;
  }

  function escrowPolicyHint(confirmation) {
    const policy = confirmation?.policy || {};
    const fee = confirmation?.fee_hint || {};
    const parts = [];
    const deposit = formatDurationHint(policy.deposit_timeout_secs);
    const review = formatDurationHint(policy.buyer_review_timeout_secs);
    if (deposit) parts.push(`Deposit window: ${deposit}`);
    if (review) parts.push(`Buyer review: ${review}`);
    const estimatedFeeUnits = feeHintTextValue(fee.estimated_fee_units);
    const feeTokenSymbol = feeHintTextValue(fee.fee_token_symbol, 24);
    if (estimatedFeeUnits && feeTokenSymbol) {
      parts.push(`Estimated network fee: ${estimatedFeeUnits} ${feeTokenSymbol} units`);
    }
    return parts.join(" | ");
  }

  function evmEscrowWalletTxPlan(confirmation, account) {
    return {
      account,
      approve: {
        to: confirmation.token,
        spender: confirmation.escrow_contract,
        amount: confirmation.amount_units
      },
      deposit: {
        to: confirmation.escrow_contract,
        order_hash: confirmation.order_hash,
        token: confirmation.token,
        amount: confirmation.amount_units,
        seller: confirmation.seller_evm_address,
        buyer: confirmation.buyer_evm_address || account,
        arbiter: confirmation.arbiter_evm_address
      }
    };
  }

  async function requestEvmEscrowDeposit(order) {
    const confirmation = evmEscrowConfirmation(order);
    if (!confirmation || !window.ethereum) {
      throw new Error("EVM wallet is not available for this order");
    }
    const chainId = Number(confirmation.chain_id);
    if (!Number.isFinite(chainId) || chainId <= 0) {
      throw new Error("EVM chain id is not available for this order");
    }
    const [account] = await window.ethereum.request({ method: "eth_requestAccounts" });
    await window.ethereum.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: `0x${chainId.toString(16)}` }]
    });
    return evmEscrowWalletTxPlan(confirmation, account);
  }

  function selectedOffer(formEl) {
    const id = (formEl.elements.offer_id && formEl.elements.offer_id.value.trim()) || DEMO.offerId;
    if (state.selectedOffer && state.selectedOffer.offer_id === id) return state.selectedOffer;
    return state.offers.find((offer) => offer.offer_id === id) || null;
  }

  function currentCustomerId() {
    const input = $('[data-form="buyer-create-order"] [name="customer_id"]');
    return (input && input.value.trim()) || DEMO.customerId;
  }

  function buyerOrder(formEl) {
    const data = form(formEl);
    const offer = selectedProjectedOffer(formEl);
    if (!offer) return null;
    const body = (offer && offer.body) || {};
    const price = (offer && offer.price) || { amount: data.amount || "100.00", currency: data.currency || "USD" };
    const sellerInstance = objectInstance(offer.offer_id || offer.seller_id);
    return {
      customer_id: data.customer_id || DEMO.customerId,
      customer_display_name: "Fixture Customer",
      order_id: data.order_id || DEMO.orderId,
      seller_id: (offer && offer.seller_id) || DEMO.sellerId,
      offer_id: (offer && offer.offer_id) || data.offer_id || DEMO.offerId,
      offer_revision: int((offer && offer.revision) || body.revision, 1),
      catalog_snapshot_id: protocolId("snap", sellerInstance, "CATALOG_SNAPSHOT"),
      price,
      payment_adapter: "mock",
      payment_capture_policy: pick(body, ["payment_terms", "capture_policy"], "before_entitlement"),
      entitlement_type: pick(body, ["entitlement", "type"], "external_entitlement"),
      seller_terms_hash: body.seller_terms_hash || SELLER_TERMS_HASH,
      offer_terms_hash: body.offer_terms_hash || OFFER_TERMS_HASH,
      arbiter_instance: "cases.example",
      arbiter_actor: "arbiter:cases.example:01JARBITER",
      arbitration_policy_id: "standard-digital-v1",
      arbitration_policy_version: "1",
      arbitration_window: "P14D",
      expires_at: "2026-05-06T10:30:00Z"
    };
  }

  function selectedProjectedOffer(formEl) {
    const id = (formEl.elements.offer_id && formEl.elements.offer_id.value.trim()) || "";
    if (!id) return null;
    return state.offers.find((offer) => offer.offer_id === id) || null;
  }

  function statusBadge(status) {
    const value = String(status || "unknown");
    let accent = "accent-cyan";
    if (value.includes("cancel") || value.includes("reject")) accent = "accent-crimson";
    if (value.includes("created") || value.includes("accepted") || value.includes("pending") || value.includes("submitted")) accent = "accent-amber";
    if (value.includes("complete") || value.includes("grant")) accent = "accent-emerald";
    return `<span class="status-pill ${accent}">${esc(value)}</span>`;
  }

  function setBuyerSyncStatus(text, accent = "accent-emerald") {
    const target = document.getElementById("buyer-sync-status");
    if (!target) return;
    target.textContent = text;
    target.className = `status-pill ${accent}`;
  }

  function offerPrice(offer, fallback = "100.00 USD") {
    const amount = pick(offer, ["price", "amount"], pick(offer, ["body", "price", "amount"], ""));
    const currency = pick(offer, ["price", "currency"], pick(offer, ["body", "price", "currency"], ""));
    const text = `${amount || ""} ${currency || ""}`.trim();
    return text || fallback;
  }

  function productForOffer(offer) {
    return closestNamedItem(state.products, "product_id", offer && offer.product_id) || {};
  }

  function productKind(item) {
    const text = [
      pick(item, ["body", "kind"], ""),
      Array.isArray(pick(item, ["body", "categories"], [])) ? pick(item, ["body", "categories"], []).join(" ") : "",
      pick(item, ["body", "title"], ""),
      item && item.product_id,
      item && item.offer_id
    ].join(" ").toLowerCase();
    if (/book|rust|system/.test(text)) return "books";
    if (/case|phone|iphone|android|shield/.test(text)) return "cases";
    if (/shoe|sneaker|runner/.test(text)) return "sneakers";
    if (/cloth|jacket|shirt|fashion/.test(text)) return "clothing";
    return "sneakers";
  }

  function productImage(item) {
    const directImage = item && (item.image_src || pick(item, ["body", "image_src"], ""));
    if (directImage) return directImage;
    const mediaImage = primaryMediaImage(item);
    if (mediaImage) return mediaImage;
    const productId = item && item.product_id;
    if (productId && SEEDED_PRODUCT_IMAGES[productId]) return SEEDED_PRODUCT_IMAGES[productId];
    return PRODUCT_IMAGES[productKind(item)] || PRODUCT_IMAGES.sneakers;
  }

  function primaryMediaImage(item) {
    const media = pick(item, ["body", "media"], item && item.media);
    if (!Array.isArray(media)) return "";
    const image = media.find((entry) => entry && (entry.kind === "image" || entry.type === "image") && entry.uri);
    return image ? image.uri : "";
  }

  function offerImage(offer) {
    const directImage = offer && (offer.image_src || pick(offer, ["body", "image_src"], ""));
    if (directImage) return directImage;
    const product = productForOffer(offer);
    return productImage(product.product_id ? product : offer);
  }

  function offerFromProductCard(card, offerId) {
    if (!card) return null;
    const title = ($("h3", card) || {}).textContent || "Marketplace offer";
    const meta = (($(".product-meta", card) || {}).textContent || `${LOCAL_INSTANCE} · Seller`).split("·").map((part) => part.trim());
    const priceText = (($(".product-card-footer strong", card) || {}).textContent || "100.00 USD").trim().split(/\s+/);
    const image = $("img", card);
    const instance = meta[0] || LOCAL_INSTANCE;
    const sellerLabel = meta[1] || "Seller";
    const amount = priceText[0] || "100.00";
    const currency = priceText[1] || "USD";
    const slug = normalizeTitle(title).toUpperCase().replace(/[^A-Z0-9]+/g, "_").replace(/^_|_$/g, "") || "OFFER";
    const localId = `DEMO_${slug}`.slice(0, 64);
    return {
      offer_id: offerId || `offer:${instance}:${localId}`,
      product_id: `prod:${instance}:${localId}`,
      seller_id: `seller:${instance}:${localId}`,
      price: { amount, currency },
      image_src: image ? image.getAttribute("src") : "",
      body: {
        title,
        description: (($("p", card) || {}).textContent || "").trim(),
        seller_display_name: sellerLabel,
        image_src: image ? image.getAttribute("src") : ""
      }
    };
  }

  function itemInstance(item) {
    const id = item && (item.seller_id || item.product_id || item.offer_id || item.order_id || item.id);
    return objectInstance(id, "local instance");
  }

  function offerTitle(offer) {
    const product = productForOffer(offer);
    return pick(product, ["body", "title"], pick(offer, ["body", "title"], offer && offer.offer_id || "Marketplace offer"));
  }

  function sellerName(sellerId) {
    const seller = closestNamedItem(state.sellers, "seller_id", sellerId);
    return (seller && (seller.display_name || pick(seller, ["body", "display_name"], ""))) || sellerId || "Seller";
  }

  function offerSellerLabel(offer) {
    return pick(offer, ["body", "seller_display_name"], sellerName(offer && offer.seller_id));
  }

  function isLiveProjectedOffer(offer) {
    return Boolean(offer && offer.offer_id && state.offers.some((item) => item.offer_id === offer.offer_id));
  }

  function updateSelectedOfferDetail() {
    const offer = state.selectedOffer;
    const drawer = $(".detail-drawer");
    if (drawer) {
      const title = $("h3", drawer);
      const copy = $(".muted-copy", drawer);
      const details = $$("dd", drawer);
      if (!offer) {
        if (title) title.textContent = "Choose an offer";
        if (copy) copy.textContent = "Select an offer card from Discover to prepare checkout details.";
        if (details[0]) details[0].textContent = "No seller selected";
        if (details[1]) details[1].textContent = "No category selected";
        if (details[2]) details[2].textContent = "No instance selected";
      } else {
        const product = productForOffer(offer);
        const categories = pick(product, ["body", "categories"], pick(offer, ["body", "categories"], []));
        const categoryText = Array.isArray(categories) && categories.length ? categories.join(", ") : normalizeTitle(pick(product, ["body", "kind"], "digital_service"));
        if (title) title.textContent = offerTitle(offer);
        if (copy) copy.textContent = `${offerPrice(offer)} from ${offerSellerLabel(offer)}. Checkout uses this projected offer and keeps protocol ids in Advanced.`;
        if (details[0]) details[0].textContent = offerSellerLabel(offer);
        if (details[1]) details[1].textContent = categoryText;
        if (details[2]) details[2].textContent = itemInstance(offer);
      }
    }
    updateCheckout(offer);
  }

  function updateCheckout(offer) {
    const title = $("[data-checkout-title]");
    const seller = $("[data-checkout-seller]");
    const price = $("[data-checkout-price]");
    const image = $("[data-checkout-image]");
    if (!title || !seller || !price || !image) return;
    const selected = offer || state.selectedOffer || {};
    title.textContent = offerTitle(selected);
    seller.textContent = `${offerSellerLabel(selected)} · ${itemInstance(selected)}`;
    price.textContent = offerPrice(selected);
    image.src = offerImage(selected);
    image.alt = offerTitle(selected);
    const live = isLiveProjectedOffer(selected);
    const assurance = $("[data-checkout-assurance]");
    if (assurance) {
      assurance.innerHTML = live
        ? `<span class="status-pill accent-emerald">Live projected offer</span><span>Seller trusted. Price snapshot ready.</span>`
        : `<span class="status-pill accent-amber">Live offer unavailable</span><span>Load catalog and choose a projected offer before creating an order.</span>`;
    }
    const submit = $(".checkout-submit");
    if (submit) {
      submit.disabled = !live;
      submit.textContent = live ? "Create order" : "Load catalog to buy";
    }
  }

  function setCheckoutOpen(open) {
    const sheet = $("[data-checkout-sheet]");
    const overlay = $("[data-checkout-overlay]");
    if (!sheet || !overlay) return;
    sheet.hidden = !open;
    overlay.hidden = !open;
    document.body.classList.toggle("checkout-open", open);
  }

  function setBuyerOrderFormFromOffer(create, offer) {
    if (!create || !offer) return;
    if (create.elements.offer_id) create.elements.offer_id.value = offer.offer_id || DEMO.offerId;
    if (create.elements.amount) create.elements.amount.value = pick(offer, ["price", "amount"], "100.00");
    if (create.elements.currency) create.elements.currency.value = pick(offer, ["price", "currency"], "USD");
  }

  function hydrateRuntimeDefaults() {
    const sellerForm = $('[data-form="seller-announce"]');
    if (sellerForm) {
      if (sellerForm.elements.seller_id) sellerForm.elements.seller_id.value = DEMO.sellerId;
      if (sellerForm.elements.legal_profile_ref) sellerForm.elements.legal_profile_ref.value = `https://${LOCAL_INSTANCE}/legal`;
      if (sellerForm.elements.terms_ref) sellerForm.elements.terms_ref.value = `https://${LOCAL_INSTANCE}/terms`;
    }
    const sellerDisplayName = $("[data-seller-display-name]");
    if (sellerDisplayName && sellerForm && sellerForm.elements.display_name) {
      sellerDisplayName.value = sellerForm.elements.display_name.value || "Fixture Seller";
      sellerDisplayName.addEventListener("input", () => {
        sellerForm.elements.display_name.value = sellerDisplayName.value || "Fixture Seller";
      });
    }
    const productForm = $('[data-form="seller-product"]');
    if (productForm) {
      if (productForm.elements.product_id) productForm.elements.product_id.value = DEMO.productId;
    }
    const offerForm = $('[data-form="seller-offer"]');
    if (offerForm) {
      if (offerForm.elements.offer_id) offerForm.elements.offer_id.value = DEMO.offerId;
      if (offerForm.elements.product_id) offerForm.elements.product_id.value = DEMO.productId;
    }
    const buyerForm = $('[data-form="buyer-create-order"]');
    if (buyerForm) {
      if (buyerForm.elements.customer_id) buyerForm.elements.customer_id.value = DEMO.customerId;
      if (buyerForm.elements.order_id) buyerForm.elements.order_id.value = DEMO.orderId;
      if (buyerForm.elements.offer_id) buyerForm.elements.offer_id.value = "";
    }
    const buyerTools = $('[data-form="buyer-order-tools"]');
    if (buyerTools && buyerTools.elements.order_id) buyerTools.elements.order_id.value = DEMO.orderId;
    const sellerOrder = $('[data-form="seller-order-action"]');
    if (sellerOrder && sellerOrder.elements.order_id) sellerOrder.elements.order_id.value = DEMO.orderId;
  }

  function resetSellerDraftIds() {
    const productForm = $('[data-form="seller-product"]');
    const offerForm = $('[data-form="seller-offer"]');
    DEMO.productId = freshDraftId("prod");
    DEMO.offerId = freshDraftId("offer");
    if (productForm && productForm.elements.product_id) productForm.elements.product_id.value = DEMO.productId;
    if (offerForm) {
      if (offerForm.elements.product_id) offerForm.elements.product_id.value = DEMO.productId;
      if (offerForm.elements.offer_id) offerForm.elements.offer_id.value = DEMO.offerId;
    }
  }

  function setProductImagePreview(src) {
    const preview = $("[data-product-image-preview]");
    if (!preview) return;
    const image = $("img", preview);
    if (!image) return;
    const productForm = $('[data-form="seller-product"]');
    const category = productForm && productForm.elements.kind ? productForm.elements.kind.value : "marketplace";
    image.src = src || PRODUCT_IMAGES[productKind({ body: { categories: [category], kind: category } })] || PRODUCT_IMAGES.sneakers;
  }

  function clearSellerImage() {
    const productForm = $('[data-form="seller-product"]');
    const input = $("[data-product-image-input]");
    if (productForm && productForm.elements.image_src) productForm.elements.image_src.value = "";
    if (input) input.value = "";
    setProductImagePreview("");
  }

  function readFileAsDataUrl(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result || ""));
      reader.onerror = () => reject(reader.error || new Error("Could not read image file."));
      reader.readAsDataURL(file);
    });
  }

  function loadImage(src) {
    return new Promise((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () => reject(new Error("Could not decode image file."));
      image.src = src;
    });
  }

  async function compressProductImage(file) {
    if (!file || !file.type || !file.type.startsWith("image/")) throw new Error("Choose an image file.");
    const dataUrl = await readFileAsDataUrl(file);
    const image = await loadImage(dataUrl);
    const maxDataUrlLength = 18000;
    let maxSide = 320;
    let quality = 0.72;

    for (let attempt = 0; attempt < 8; attempt += 1) {
      const scale = Math.min(1, maxSide / Math.max(image.naturalWidth || 1, image.naturalHeight || 1));
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round((image.naturalWidth || 1) * scale));
      canvas.height = Math.max(1, Math.round((image.naturalHeight || 1) * scale));
      const context = canvas.getContext("2d");
      if (!context) throw new Error("Could not prepare image preview.");
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const compressed = canvas.toDataURL("image/jpeg", quality);
      if (compressed.length <= maxDataUrlLength) return compressed;
      if (quality > 0.44) {
        quality -= 0.12;
      } else {
        maxSide = Math.max(96, Math.round(maxSide * 0.72));
      }
    }

    throw new Error("Image is too detailed for an inline protocol preview. Use a simpler or smaller image.");
  }

  async function handleProductImageFile(file) {
    try {
      const imageSrc = await compressProductImage(file);
      const productForm = $('[data-form="seller-product"]');
      if (productForm && productForm.elements.image_src) productForm.elements.image_src.value = imageSrc;
      setProductImagePreview(imageSrc);
      toast("Image attached", "success", "Fill title and price, then click Publish listing.");
    } catch (error) {
      clearSellerImage();
      toast("Image not attached", "error", error.message || "Could not use this image.");
    }
  }

  function renderAdminSummary(body) {
    const target = document.getElementById("admin-summary-cards");
    const summary = body || {};
    const catalog = summary.catalog || {};
    if (!target) return;
    const groups = [
      {
        title: "Catalog health",
        accent: "accent-emerald",
        items: [
          ["Sellers", catalog.sellers || 0],
          ["Products", catalog.products || 0],
          ["Offers", catalog.offers || 0],
          ["Tombstones", catalog.tombstones || 0]
        ]
      },
      {
        title: "Orders",
        accent: "accent-cyan",
        items: [["Orders", summary.orders || 0]]
      },
      {
        title: "Settlement",
        accent: "accent-emerald",
        items: [
          ["Payments", summary.payments || 0],
          ["Entitlements", summary.entitlements || 0]
        ]
      },
      {
        title: "Risk",
        accent: "accent-crimson",
        items: [
          ["Disputes", summary.disputes || 0],
          ["Rulings", summary.arbitration_rulings || 0]
        ]
      }
    ];
    target.innerHTML = groups.map((group) =>
      `<section class="counter-group">
        <div class="counter-group-head"><span class="status-pill ${group.accent}">${esc(group.title)}</span></div>
        <div class="counter-group-grid">${group.items.map(([label, value]) =>
          `<div class="metric-card compact-admin-metric"><span>${esc(label)}</span><strong>${esc(value)}</strong></div>`
        ).join("")}</div>
      </section>`
    ).join("");
    setText("admin-catalog-counts", `${catalog.sellers || 0} sellers / ${catalog.products || 0} products / ${catalog.offers || 0} offers`);
    setText("admin-order-counts", `${summary.orders || 0} orders`);
  }

  function renderAdminAllowlist(body) {
    const target = document.getElementById("admin-policy-cards") || document.getElementById("admin-allowlist-view");
    const items = Array.isArray(body && body.allowlist) ? body.allowlist : [];
    if (!target) return;
    if (!items.length) {
      target.innerHTML = `<div class="empty-state">Allowlist is intentionally empty. Source: ${esc(body && body.source || "unknown")}; configured: ${esc(body && body.configured)}</div>`;
      return;
    }
    const source = body && body.source ? body.source : "unknown";
    target.innerHTML = items.map((item) => {
      const capabilities = Array.isArray(item.capabilities) ? item.capabilities : [];
      return `<article class="policy-card">
        <div>
          <span class="status-pill ${item.status === "active" ? "accent-emerald" : "accent-amber"}">${esc(item.status || "configured")}</span>
          <h3>${esc(item.instance_id || "instance")}</h3>
          <p>Source: ${esc(source)} · configured: ${esc(body && body.configured)}</p>
        </div>
        <div class="capability-row">${capabilities.length ? capabilities.map((capability) =>
          `<span class="capability-chip">${esc(capability)}</span>`
        ).join("") : '<span class="capability-chip">no capabilities</span>'}</div>
      </article>`;
    }).join("");
  }

  function renderAdminEvents(body) {
    const rows = document.getElementById("admin-events-rows");
    const list = document.getElementById("admin-incident-list");
    const events = Array.isArray(body && body.events) ? body.events : [];
    state.admin.incidents = events;
    setText("admin-error-count", `${events.length} ${events.length === 1 ? "incident" : "incidents"}`);
    setText("admin-incident-count", events.length ? `${events.length} needs attention` : "No incidents");
    if (!events.length) {
      if (rows) rows.innerHTML = '<tr><td colspan="3" class="empty-cell">No projection errors are recorded.</td></tr>';
      if (list) list.innerHTML = '<div class="empty-state">No projection errors are recorded.</div>';
      updateAdminOverallStatus();
      return;
    }
    if (rows) {
      rows.innerHTML = events.map((event) =>
        `<tr><td>${esc(event.code || "unknown")}</td><td>${esc(event.message || "")}</td><td class="mono">${esc(event.matrix_event_id || "")}</td></tr>`
      ).join("");
    }
    if (list) {
      list.innerHTML = events.map((event) => {
        const code = event.code || "unknown";
        const action = /ROOM/.test(code)
          ? "Open Maintenance and replay the affected order after room membership is repaired."
          : /TERMS|CATALOG/.test(code)
            ? "Review seller terms and rebuild catalog projection after source data is corrected."
            : "Inspect the event payload in Debug before retrying projection.";
        return `<article class="incident-card">
          <div class="incident-main">
            <span class="status-pill accent-crimson">Needs attention</span>
            <h3>${esc(code)}</h3>
            <p>${esc(event.message || "Projection error recorded.")}</p>
            <div class="incident-action"><strong>Suggested action</strong><span>${esc(action)}</span></div>
          </div>
          <button class="btn btn-small" type="button" data-copy-event-id="${esc(event.matrix_event_id || "")}">Copy event id</button>
        </article>`;
      }).join("");
    }
    updateAdminOverallStatus();
  }

  function renderCatalog(kind, items) {
    const target = document.getElementById(`buyer-${kind}`);
    if (!target) return;
    if (!items.length) {
      target.innerHTML = kind === "offers"
        ? `<div class="empty-state order-empty-state"><strong>No live offers yet</strong><span>Catalog sync did not return projected offers. Demo previews stay visible only before the first successful sync.</span><button class="btn btn-primary" type="button" data-action="buyer-catalog">Refresh catalog</button></div>`
        : `<div class="empty-state">No ${esc(kind)} found. Refresh after projection data exists.</div>`;
      return;
    }
    if (kind === "offers") {
      target.innerHTML = items.map((item) => {
        const title = offerTitle(item);
        const seller = sellerName(item.seller_id);
        const selected = state.selectedOffer && state.selectedOffer.offer_id === item.offer_id ? " is-selected" : "";
        const description = pick(productForOffer(item), ["body", "description"], "Trusted marketplace offer ready for checkout.");
        return `<article class="product-card${selected}" data-catalog-kind="offers" data-catalog-id="${esc(item.offer_id || "")}">
          <img src="${esc(offerImage(item))}" alt="${esc(title)}">
          <div class="product-card-body">
            <span class="status-pill accent-emerald">Live projected offer</span>
            <span class="product-meta">${esc(itemInstance(item))} · ${esc(seller)} · trusted source</span>
            <h3>${esc(title)}</h3>
            <p>${esc(description)}</p>
            <div class="product-card-footer">
              <strong>${esc(offerPrice(item))}</strong>
              <button class="btn btn-primary" data-select-offer="${esc(item.offer_id || "")}" data-open-checkout aria-label="Buy ${esc(title)}">Buy</button>
            </div>
          </div>
        </article>`;
      }).join("");
      return;
    }
    target.innerHTML = items.map((item) => {
      const id = item.seller_id || item.product_id || item.offer_id || item.id || "item";
      const seller = sellerName(item.seller_id);
      const title = kind === "offers" ? offerTitle(item) : pick(item, ["body", "title"], item.display_name || item.status || id);
      const instance = itemInstance(item);
      const extra = kind === "offers"
        ? offerPrice(item)
        : kind === "products"
          ? normalizeTitle(pick(item, ["body", "kind"], "catalog item"))
          : displayId(item.status, "announced");
      const button = kind === "offers" ? `<button class="btn btn-small" data-select-offer="${esc(item.offer_id || "")}">Select</button>` : "";
      const selected = kind === "offers" && state.selectedOffer && state.selectedOffer.offer_id === item.offer_id ? " is-selected" : "";
      const subtitle = kind === "offers" ? `${seller} · ${instance}` : `${instance} · ${displayId(id)}`;
      return `<article class="list-item catalog-item${selected}" data-catalog-kind="${esc(kind)}" data-catalog-id="${esc(id)}"><div><strong>${esc(title)}</strong><span>${esc(subtitle)}</span><span class="mono">${esc(extra)}</span></div>${button}</article>`;
    }).join("");
  }

  function sellerOrdersNeedingAction() {
    return state.orders.filter((order) => sellerOrderActions(order && order.status).length > 0).length;
  }

  function updateSellerMetrics() {
    const localOffers = state.offers.filter((offer) => objectInstance(offer.seller_id || offer.offer_id) === LOCAL_INSTANCE);
    setText("seller-published-count", String(localOffers.length));
    setText("seller-draft-count", String(state.pendingListings.length));
    setText("seller-action-count", String(sellerOrdersNeedingAction()));
  }

  function sellerEmptyStore() {
    return `<div class="empty-state order-empty-state seller-empty-store">
      <strong>No live listings yet</strong>
      <span>Create a listing or refresh after projection catches up. Demo previews are shown only before a successful catalog sync.</span>
      <button class="btn btn-primary" type="button" data-seller-quick-add-toggle>Add listing</button>
    </div>`;
  }

  function pendingSellerListingCard(entry) {
    return `<article class="seller-product-card listing-pending">
      <img src="${esc(entry.image_src || PRODUCT_IMAGES[entry.kind] || PRODUCT_IMAGES.sneakers)}" alt="${esc(entry.title)}">
      <div class="seller-card-body">
        <span class="status-pill accent-amber">Projection pending</span>
        <h3>${esc(entry.title)}</h3>
        <p>${esc(LOCAL_INSTANCE)} · ${esc(normalizeTitle(entry.kind || "marketplace"))}</p>
        <div class="seller-card-footer">
          <strong>${esc(entry.price)}</strong>
          <button class="btn btn-small" type="button" data-action="seller-catalog">Refresh catalog</button>
        </div>
      </div>
    </article>`;
  }

  function markSellerListingPending(product, offer) {
    if (!product || !offer) return null;
    const productData = form(product);
    const offerData = form(offer);
    const offerId = offerData.offer_id || DEMO.offerId;
    const entry = {
      offer_id: offerId,
      title: productData.title || "Submitted listing",
      kind: productKind({ body: { categories: [productData.kind], kind: productData.kind } }),
      image_src: productData.image_src || "",
      price: `${offerData.amount || "0.00"} ${offerData.currency || "USD"}`,
      submitted_at: new Date().toISOString()
    };
    state.pendingListings = state.pendingListings.filter((item) => item.offer_id !== offerId);
    state.pendingListings.unshift(entry);
    renderSellerStoreCards();
    showResult("Publish listing", "projection_pending", {
      offer_id: offerId,
      status: "Projection pending",
      guidance: "The listing was submitted. Waiting for /api/v1/catalog/offers projection."
    });
    toast("Listing submitted", "success", "Projection pending in My Store.");
    return entry;
  }

  function renderSellerStoreCards() {
    const target = document.getElementById("seller-store-cards");
    if (!target) return;
    const localOffers = state.offers.filter((offer) => objectInstance(offer.seller_id || offer.offer_id) === LOCAL_INSTANCE);
    const projectedIds = new Set(localOffers.map((offer) => offer.offer_id).filter(Boolean));
    state.pendingListings = state.pendingListings.filter((entry) => entry && entry.offer_id && !projectedIds.has(entry.offer_id));
    const liveCards = localOffers.map((offer) => {
      const product = productForOffer(offer);
      const title = offerTitle(offer);
      const category = normalizeTitle(pick(product, ["body", "kind"], "marketplace"));
      return `<article class="seller-product-card is-live">
        <img src="${esc(offerImage(offer))}" alt="${esc(title)}">
        <div class="seller-card-body">
          <span class="status-pill accent-emerald">Published</span>
          <h3>${esc(title)}</h3>
          <p>${esc(LOCAL_INSTANCE)} · ${esc(category)}</p>
          <div class="seller-card-footer">
            <strong>${esc(offerPrice(offer))}</strong>
            <button class="btn btn-small btn-danger" type="button" data-seller-offer-withdraw data-offer-id="${esc(offer.offer_id || "")}" data-seller-id="${esc(offer.seller_id || "")}" data-revision="${esc(offer.revision || pick(offer, ["body", "revision"], 1))}">Withdraw</button>
          </div>
        </div>
      </article>`;
    });
    const pendingCards = state.pendingListings.map(pendingSellerListingCard);
    const cards = pendingCards.concat(liveCards);
    updateSellerMetrics();
    if (!cards.length) {
      if (target.dataset.catalogSynced === "true") target.innerHTML = sellerEmptyStore();
      return;
    }
    target.innerHTML = cards.join("");
  }

  function orderTimeline(order) {
    const status = String(order.status || "created");
    const paymentDetail = isEvmEscrowOrder(order)
      ? "EVM escrow waits for wallet approval and deposit."
      : "Mock adapter records intent and capture evidence.";
    const steps = [
      ["Created", "Order terms were submitted by the buyer.", true],
      ["Accepted", "Seller confirms the offer revision and terms.", /accepted|authorized|captured|grant|complete/.test(status)],
      ["Payment", paymentDetail, /payment|captured|grant|complete/.test(status)],
      ["Entitlement", "Access evidence is granted before completion.", /entitlement|grant|complete/.test(status)],
      ["Complete", "The order lifecycle is projected as complete.", /complete/.test(status)]
    ];
    return `<ol class="timeline-list compact-timeline">${steps.map(([label, detail, active]) =>
      `<li class="timeline-step"><span>${active ? statusBadge(label.toLowerCase()) : ""}<strong>${esc(label)}</strong><span>${esc(detail)}</span></span></li>`
    ).join("")}</ol>`;
  }

  function pendingOrderMessage(entry) {
    if (entry.state === "timeout") {
      return "Confirmation is taking longer than usual. Refresh orders or check again in a moment.";
    }
    return "Order submitted. Confirmation may take a few seconds.";
  }

  function pendingOrderAction(entry) {
    if (entry.state !== "timeout") return "";
    return `<button class="btn btn-small" type="button" data-action="buyer-orders">Refresh orders</button>`;
  }

  function pendingOrderCard(entry) {
    const status = entry.state === "timeout" ? "projection_timeout" : "Projection pending";
    return `<article class="order-card order-card-pending${entry.state === "timeout" ? " is-timeout" : ""}">
      <div class="section-head compact-head">
        <div>
          <p class="eyebrow">${esc(entry.state === "timeout" ? "Projection delayed" : "Projection pending")}</p>
          <h3>${esc(displayId(entry.order_id, "Submitted order"))}</h3>
          <p class="mono">${esc(displayId(entry.offer_id, "Offer not attached"))}</p>
        </div>
        ${statusBadge(status)}
      </div>
      <p>${esc(pendingOrderMessage(entry))}</p>
      ${pendingOrderAction(entry)}
    </article>`;
  }

  function pendingOrderRow(entry) {
    const status = entry.state === "timeout" ? "projection_timeout" : "Projection pending";
    const guidance = entry.state === "timeout" ? "Refresh orders" : "Waiting for projection";
    return `<tr class="pending-order-row${entry.state === "timeout" ? " is-timeout" : ""}">
      <td class="mono">${esc(entry.order_id)}</td>
      <td>${statusBadge(status)}<span class="pending-guidance">${esc(guidance)}</span></td>
      <td class="mono">${esc(entry.offer_id)}</td>
    </tr>`;
  }

  function unresolvedPendingOrders() {
    const projected = new Set(state.orders.map((order) => order.order_id).filter(Boolean));
    return state.pendingOrders.filter((entry) => entry && entry.order_id && !projected.has(entry.order_id));
  }

  function reconcilePendingOrders() {
    const before = state.pendingOrders.length;
    state.pendingOrders = unresolvedPendingOrders();
    return before !== state.pendingOrders.length;
  }

  function hasProjectedOrder(orderId) {
    return state.orders.some((order) => order && order.order_id === orderId);
  }

  function markBuyerOrderPending(payload) {
    const orderId = payload && payload.order_id;
    if (!orderId) return null;
    state.pendingOrders = state.pendingOrders.filter((entry) => entry.order_id !== orderId);
    const entry = {
      order_id: orderId,
      offer_id: payload.offer_id || "",
      state: "pending",
      submitted_at: new Date().toISOString()
    };
    state.pendingOrders.unshift(entry);
    renderOrders("buyer-orders-rows", "buyer-order-count", 3);
    showResult("POST /api/v1/buyer/orders", "submitted", {
      order_id: orderId,
      status: "Projection pending",
      guidance: "The order was accepted for processing. Waiting for /api/v1/buyer/orders projection."
    });
    toast("Order submitted", "success", "Projection pending in buyer orders.");
    return entry;
  }

  function markBuyerOrderProjectionTimeout(orderId) {
    const entry = state.pendingOrders.find((item) => item.order_id === orderId);
    if (!entry || hasProjectedOrder(orderId)) return;
    entry.state = "timeout";
    entry.timed_out_at = new Date().toISOString();
    renderOrders("buyer-orders-rows", "buyer-order-count", 3);
    showResult("GET /api/v1/buyer/orders", "projection_timeout", {
      order_id: orderId,
      status: "projection_timeout",
      guidance: "Refresh orders or check again after confirmation catches up."
    });
    toast("Confirmation still pending", "neutral", "Refresh orders again in a moment.");
  }

  function ensureOrderCards(rows, rowsId) {
    if (rowsId === "buyer-orders-rows") {
      const existing = document.getElementById("buyer-order-cards");
      if (existing) return existing;
    }
    const id = `${rowsId}-cards`;
    let cards = document.getElementById(id);
    if (cards) return cards;
    const tableWrap = rows.closest(".table-wrap") || rows.parentElement;
    if (!tableWrap || !tableWrap.parentElement) return null;
    cards = document.createElement("div");
    cards.id = id;
    cards.className = "timeline-list order-card-list";
    tableWrap.parentElement.insertBefore(cards, tableWrap);
    return cards;
  }

  function sellerOrderActions(status) {
    const normalized = String(status || "").toLowerCase();
    const actionsByStatus = {
      created: [{ step: "accept", label: "Accept order", primary: false }],
      accepted: [{ step: "payment-intent", label: "Request payment", primary: false }],
      payment_intent_created: [{ step: "payment-capture", label: "Confirm payment", primary: false }],
      payment_authorized: [{ step: "payment-capture", label: "Confirm payment", primary: false }],
      payment_captured: [{ step: "entitlement-grant", label: "Grant access", primary: false }],
      entitlement_granted: [{ step: "complete", label: "Complete order", primary: true }],
      entitlement_activated: [{ step: "complete", label: "Complete order", primary: true }],
      entitlement_completed: [{ step: "complete", label: "Complete order", primary: true }]
    };
    return actionsByStatus[normalized] || [];
  }

  function evmEscrowWalletAction(order) {
    if (!isEvmEscrowOrder(order)) return "";
    const confirmation = evmEscrowConfirmation(order);
    if (!confirmation) {
      return `<div class="wallet-action-row"><span class="muted-text">Waiting for escrow payment intent.</span></div>`;
    }
    const hint = escrowPolicyHint(confirmation);
    const hintMarkup = hint ? `<span class="muted-text">${esc(hint)}</span>` : "";
    return `<div class="wallet-action-row"><button class="btn btn-small btn-primary" type="button" data-evm-escrow-deposit data-order-id="${esc(order.order_id || "")}">Approve and deposit</button><span class="mono">${esc(confirmation.order_hash || "order hash pending")}</span>${hintMarkup}</div>`;
  }

  function sellerOrderActionRow(order) {
    const status = String((order && order.status) || "").toLowerCase();
    const actions = sellerOrderActions(status);
    if (!actions.length) {
      if (status === "completed") {
        return `<div class="button-row stretch order-action-row"><span class="muted-text">Completed - no further seller action needed.</span></div>`;
      }
      const label = status ? status.replaceAll("_", " ") : "unknown status";
      return `<div class="button-row stretch order-action-row"><span class="muted-text">No seller action available for ${esc(label)}.</span></div>`;
    }
    const orderId = esc((order && order.order_id) || "");
    return `<div class="button-row stretch order-action-row">${actions.map((action) => {
      const className = action.primary ? "btn btn-small btn-primary" : "btn btn-small";
      return `<button class="${className}" type="button" data-seller-order-step="${esc(action.step)}" data-order-id="${orderId}">${esc(action.label)}</button>`;
    }).join("")}</div>`;
  }

  function renderOrders(rowsId, countId, columns) {
    const rows = document.getElementById(rowsId);
    if (!rows) return;
    const isSeller = rowsId === "seller-orders-rows";
    const pendingOrders = rowsId === "buyer-orders-rows" ? unresolvedPendingOrders() : [];
    if (countId) {
      const pendingText = pendingOrders.length ? `, ${pendingOrders.length} pending` : "";
      setText(countId, `${state.orders.length} orders${pendingText}`);
    }
    const cards = ensureOrderCards(rows, rowsId);
    if (!state.orders.length && !pendingOrders.length) {
      rows.innerHTML = `<tr><td colspan="${columns}" class="empty-cell">${isSeller ? "No seller orders need action." : "No orders found. Create one from the buyer workspace, then refresh."}</td></tr>`;
      if (cards) {
        cards.innerHTML = isSeller
          ? `<div class="empty-state order-empty-state"><strong>No orders need seller action</strong><span>New buyer orders will appear here after they are projected.</span><a class="btn btn-primary" href="#store" data-action="seller-store">Back to store</a></div>`
          : `<div class="empty-state order-empty-state"><strong>No orders yet</strong><span>Browse catalog and choose a live projected offer to start an order.</span><a class="btn btn-primary" href="#discover" data-action="buyer-discover">Browse catalog</a></div>`;
      }
      if (isSeller) updateSellerMetrics();
      return;
    }
    if (cards) {
      const projectedCards = state.orders.map((order) => {
        const title = displayId(order.order_id, "Order");
        const offer = displayId(order.offer_id, "Offer not attached");
        const actor = columns === 5 ? displayId(order.customer_id, "Customer not attached") : sellerName(order.seller_id);
        const sellerActions = columns === 5 ? sellerOrderActionRow(order) : "";
        const walletAction = columns === 5 ? "" : evmEscrowWalletAction(order);
        return `<article class="order-card"><div class="section-head compact-head"><div><p class="eyebrow">${esc(actor)}</p><h3>${esc(title)}</h3><p class="mono">${esc(offer)}</p></div>${statusBadge(order.status)}</div>${orderTimeline(order)}${walletAction}${sellerActions}</article>`;
      });
      cards.innerHTML = pendingOrders.map(pendingOrderCard).concat(projectedCards).join("");
    }
    const projectedRows = state.orders.map((order) => {
      if (columns === 5) {
        return `<tr><td class="mono">${esc(order.order_id)}</td><td>${statusBadge(order.status)}</td><td class="mono">${esc(order.customer_id)}</td><td class="mono">${esc(order.offer_id)}</td><td class="mono">${esc(order.room_id)}</td></tr>`;
      }
      return `<tr><td class="mono">${esc(order.order_id)}</td><td>${statusBadge(order.status)}</td><td class="mono">${esc(order.offer_id)}</td></tr>`;
    });
    rows.innerHTML = pendingOrders.map(pendingOrderRow).concat(projectedRows).join("");
    if (isSeller) updateSellerMetrics();
  }

  async function refreshAdmin({ silent = true } = {}) {
    const requestOptions = { silent, result: !silent };
    const health = await api("/healthz", { action: "GET /healthz", ...requestOptions });
    state.admin.healthOk = health.ok && ((health.body && health.body.status) || "ok") === "ok";
    setText("admin-health-status", health.ok ? ((health.body && health.body.status) || "ok") : "error");
    const ready = await api("/readyz", { action: "GET /readyz", ...requestOptions });
    state.admin.readyOk = ready.ok && ((ready.body && ready.body.status) || "ready") === "ready";
    setText("admin-ready-status", ready.ok ? ((ready.body && ready.body.status) || "ready") : "error");
    await api("/admin/config", { tokenRole: "admin", action: "GET /admin/config", ...requestOptions });
    const allowlist = await api("/admin/allowlist", { tokenRole: "admin", action: "GET /admin/allowlist", ...requestOptions });
    if (allowlist.ok) renderAdminAllowlist(allowlist.body);
    const summary = await api("/admin/projections/summary", { tokenRole: "admin", action: "GET /admin/projections/summary", ...requestOptions });
    if (summary.ok) renderAdminSummary(summary.body);
    const events = await api("/admin/events", { tokenRole: "admin", action: "GET /admin/events", ...requestOptions });
    if (events.ok) renderAdminEvents(events.body);
    const stamp = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    setText("admin-auto-refresh", `Auto refresh ${stamp}`);
    setText("admin-last-refresh-value", stamp);
    updateAdminOverallStatus();
  }

  async function refreshCatalog() {
    setBuyerSyncStatus("Syncing catalog", "accent-amber");
    setSellerSyncStatus("Syncing catalog", "accent-amber");
    const sellers = await silentGet("/api/v1/catalog/sellers", { action: "GET /api/v1/catalog/sellers" });
    if (sellers.ok) {
      state.sellers = Array.isArray(sellers.body.items) ? sellers.body.items : [];
      renderCatalog("sellers", state.sellers);
    }
    const products = await silentGet("/api/v1/catalog/products", { action: "GET /api/v1/catalog/products" });
    if (products.ok) {
      state.products = Array.isArray(products.body.items) ? products.body.items : [];
      renderCatalog("products", state.products);
    }
    const offers = await silentGet("/api/v1/catalog/offers", { action: "GET /api/v1/catalog/offers" });
    if (offers.ok) {
      state.offers = Array.isArray(offers.body.items) ? offers.body.items : [];
      if (!state.selectedOffer && state.offers.length) state.selectedOffer = state.offers[0];
      renderCatalog("offers", state.offers);
      const sellerTarget = document.getElementById("seller-store-cards");
      if (sellerTarget) sellerTarget.dataset.catalogSynced = "true";
      renderSellerStoreCards();
      updateSelectedOfferDetail();
      const stamp = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      setBuyerSyncStatus(state.offers.length ? `Updated ${stamp}` : "No live offers", state.offers.length ? "accent-emerald" : "accent-amber");
      setSellerSyncStatus(state.offers.length ? `Updated ${stamp}` : "No live listings", state.offers.length ? "accent-emerald" : "accent-amber");
    } else {
      setBuyerSyncStatus("Demo preview", "accent-cyan");
      setSellerSyncStatus("Demo preview", "accent-cyan");
    }
  }

  async function refreshOrders(role) {
    const path = role === "seller" ? "/api/v1/seller/orders" : "/api/v1/buyer/orders";
    const result = await silentGet(path, { tokenRole: role, action: `GET ${path}` });
    if (!result.ok) return;
    state.orders = Array.isArray(result.body && result.body.orders) ? result.body.orders : [];
    if (role === "buyer") reconcilePendingOrders();
    if (role === "seller") {
      renderOrders("seller-orders-rows", "seller-order-count", 5);
      updateSellerMetrics();
    }
    if (role === "buyer") renderOrders("buyer-orders-rows", "buyer-order-count", 3);
  }

  function delay(ms) {
    return new Promise((resolve) => window.setTimeout(resolve, ms));
  }

  async function pollCatalog(attempts = 8) {
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      await refreshCatalog();
      await delay(700);
    }
  }

  function markFieldInvalid(field, message) {
    if (!field) return false;
    field.setCustomValidity(message);
    field.reportValidity();
    window.setTimeout(() => field.setCustomValidity(""), 0);
    return false;
  }

  function validateSellerProductForm(product) {
    if (!product) return false;
    const title = product.elements.title;
    const kind = product.elements.kind;
    if (!String((title && title.value) || "").trim()) {
      return markFieldInvalid(title, "Enter the product title before publishing.");
    }
    if (!String((kind && kind.value) || "").trim()) {
      return markFieldInvalid(kind, "Enter the product category before publishing.");
    }
    return product.reportValidity();
  }

  function validateSellerOfferForm(offer) {
    if (!offer) return false;
    const amount = offer.elements.amount;
    const rawAmount = String((amount && amount.value) || "").trim();
    if (!rawAmount) return markFieldInvalid(amount, "Enter the listing price before publishing.");
    const numericAmount = Number(rawAmount);
    if (!Number.isFinite(numericAmount) || numericAmount <= 0) {
      return markFieldInvalid(amount, "Enter a positive listing price.");
    }
    return offer.reportValidity();
  }

  function validateSellerListing(product, offer) {
    const validProduct = validateSellerProductForm(product);
    if (!validProduct) {
      toast("Listing not published", "error", "Fill product title and category first.");
      showResult("POST /api/v1/seller/offers", "not-submitted", {
        code: "LISTING_FORM_INCOMPLETE",
        error: "Product title and category are required before publishing."
      });
      return false;
    }
    const validOffer = validateSellerOfferForm(offer);
    if (!validOffer) {
      toast("Listing not published", "error", "Fill a positive price first.");
      showResult("POST /api/v1/seller/offers", "not-submitted", {
        code: "LISTING_FORM_INCOMPLETE",
        error: "A positive listing price is required before publishing."
      });
      return false;
    }
    return true;
  }

  async function publishSellerListing(announce, product, offer) {
    if (!validateSellerListing(product, offer)) {
      return { ok: false, status: "not-submitted", body: { code: "LISTING_FORM_INCOMPLETE" } };
    }
    const publishOptions = { silent: true, result: false };
    const announceResult = await api("/api/v1/seller/announce", {
      method: "POST",
      tokenRole: "seller",
      body: sellerAnnounce(announce),
      action: "POST /api/v1/seller/announce",
      ...publishOptions
    });
    if (!announceResult.ok) {
      showResult("POST /api/v1/seller/announce", announceResult.status, announceResult.body);
      toast("Listing not published", "error", "Seller activation failed.");
      return announceResult;
    }

    const productResult = await api("/api/v1/seller/products", {
      method: "POST",
      tokenRole: "seller",
      body: sellerProduct(product),
      action: "POST /api/v1/seller/products",
      ...publishOptions
    });
    if (!productResult.ok) {
      showResult("POST /api/v1/seller/products", productResult.status, productResult.body);
      toast("Listing not published", "error", "Product save failed.");
      return productResult;
    }

    const offerResult = await api("/api/v1/seller/offers", {
      method: "POST",
      tokenRole: "seller",
      body: sellerOffer(offer),
      action: "POST /api/v1/seller/offers",
      ...publishOptions
    });
    if (offerResult.ok) {
      markSellerListingPending(product, offer);
      await pollCatalog(10);
      resetSellerDraftIds();
      clearSellerImage();
      setSellerQuickAddOpen(false);
      showResult("Publish listing", offerResult.status, {
        status: "submitted",
        detail: "Seller, product, and offer events were submitted."
      });
      toast("Listing published", "success", "The product card should appear after projection catches up.");
    } else {
      showResult("POST /api/v1/seller/offers", offerResult.status, offerResult.body);
      toast("Listing not published", "error", "Offer publish failed.");
    }
    return offerResult;
  }

  async function pollOrders(role, attempts = 10, { pendingOrderId } = {}) {
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      await refreshOrders(role);
      if (pendingOrderId && hasProjectedOrder(pendingOrderId)) return true;
      await delay(800);
    }
    if (role === "buyer" && pendingOrderId) markBuyerOrderProjectionTimeout(pendingOrderId);
    return false;
  }

  function bindAdmin() {
    initAdminSettings();
    const replay = $('[data-form="admin-replay"]');
    const executeAdminMaintenance = async (detail) => {
      if (!detail || !detail.action) return;
      if (detail.action === "admin-rebuild") {
        const result = await api("/admin/catalog/rebuild", { method: "POST", tokenRole: "admin", body: {}, action: "POST /admin/catalog/rebuild" });
        if (result.ok && result.body && result.body.catalog) {
          renderAdminSummary({ catalog: result.body.catalog, orders: 0, payments: 0, entitlements: 0, disputes: 0, arbitration_rulings: 0 });
        }
        return;
      }
      if (detail.action === "admin-replay") {
        const orderId = detail.orderId || DEMO.orderId;
        return api(`/admin/orders/${encodeURIComponent(orderId)}/replay`, { method: "POST", tokenRole: "admin", body: {}, action: "POST /admin/orders/{order_id}/replay" });
      }
    };
    document.addEventListener("click", async (event) => {
      const settingsToggle = event.target.closest("[data-admin-settings-toggle]");
      if (settingsToggle) {
        setAdminSettingsOpen(true);
        return;
      }
      if (event.target.closest("[data-admin-debug-toggle]")) {
        openAdminDebug();
        return;
      }
      const copyEvent = event.target.closest("[data-copy-event-id]");
      if (copyEvent) {
        const value = copyEvent.dataset.copyEventId || "";
        if (value && navigator.clipboard) await navigator.clipboard.writeText(value);
        toast("Event id copied", "success", value || "No event id available.");
        return;
      }
      const confirmButton = event.target.closest("[data-maintenance-confirm]");
      if (confirmButton) {
        event.preventDefault();
        const action = confirmButton.dataset.action || "admin-rebuild";
        const orderId = replay && replay.elements.order_id ? replay.elements.order_id.value : DEMO.orderId;
        setMaintenanceConfirm(true, {
          action,
          orderId,
          title: action === "admin-replay" ? "Replay order" : "Rebuild catalog",
          message: action === "admin-replay"
            ? `Replay projection for ${orderId}.`
            : "Rebuild catalog projections from stored events."
        });
        return;
      }
      if (event.target.closest("[data-maintenance-confirm-cancel]")) {
        setMaintenanceConfirm(false);
        return;
      }
      if (event.target.closest("[data-maintenance-confirm-run]")) {
        const detail = state.admin.pendingMaintenance;
        setMaintenanceConfirm(false);
        await executeAdminMaintenance(detail);
      }
    });
    replay && replay.addEventListener("submit", (event) => {
      event.preventDefault();
      const data = form(replay);
      setMaintenanceConfirm(true, {
        action: "admin-replay",
        orderId: data.order_id || DEMO.orderId,
        title: "Replay order",
        message: `Replay projection for ${data.order_id || DEMO.orderId}.`
      });
    });
    refreshAdmin();
    window.setInterval(() => {
      if (document.visibilityState !== "hidden") refreshAdmin();
    }, 10000);
  }

  function bindSeller() {
    initSellerSettings();
    initSellerQuickAdd();
    const announce = $('[data-form="seller-announce"]');
    const product = $('[data-form="seller-product"]');
    const offer = $('[data-form="seller-offer"]');
    announce && announce.addEventListener("submit", async (event) => {
      event.preventDefault();
      const result = await api("/api/v1/seller/announce", { method: "POST", tokenRole: "seller", body: sellerAnnounce(announce), action: "POST /api/v1/seller/announce" });
      if (result.ok) await pollCatalog();
    });
    offer && offer.addEventListener("submit", async (event) => {
      event.preventDefault();
      await publishSellerListing(announce, product, offer);
    });
    const imageInput = $("[data-product-image-input]");
    imageInput && imageInput.addEventListener("change", async () => {
      const file = imageInput.files && imageInput.files[0];
      if (file) await handleProductImageFile(file);
    });
    product && product.elements.kind && product.elements.kind.addEventListener("input", () => {
      if (!product.elements.image_src || !product.elements.image_src.value) setProductImagePreview("");
    });
    document.addEventListener("click", (event) => {
      const quickAddToggle = event.target.closest("[data-seller-quick-add-toggle]");
      if (quickAddToggle) {
        setSellerQuickAddOpen(true);
        return;
      }
      const orderAction = event.target.closest("[data-seller-order-step]");
      if (orderAction) {
        const orderId = orderAction.dataset.orderId || DEMO.orderId;
        const step = orderAction.dataset.sellerOrderStep || "accept";
        const order = state.orders.find((item) => item.order_id === orderId) || {};
        const pathId = encodeURIComponent(orderId);
        const evmPaymentIntent = step === "payment-intent" && isEvmEscrowOrder(order);
        const requestStep = evmPaymentIntent ? "evm-payment-intent" : step;
        const path = step === "complete"
          ? `/api/v1/seller/orders/${pathId}/complete`
          : evmPaymentIntent
            ? `/api/v1/seller/orders/${pathId}/evm-escrow/payment-intent`
            : `/api/v1/seller/orders/${pathId}/${step}`;
        let body;
        try {
          body = sellerOrder(requestStep, orderId);
        } catch (error) {
          showResult("EVM escrow payment intent", "not-submitted", { error: error.message });
          toast("EVM escrow address missing", "error", error.message);
          setSellerSettingsOpen(true);
          return;
        }
        api(path, { method: "POST", tokenRole: "seller", body, action: `POST ${path}` })
          .then((result) => result.ok && pollOrders("seller"));
        return;
      }
      const withdraw = event.target.closest("[data-seller-offer-withdraw]");
      if (withdraw) {
        const offerId = withdraw.dataset.offerId || "";
        const sellerId = withdraw.dataset.sellerId || currentSellerId();
        const revision = Number(withdraw.dataset.revision || 1);
        if (!offerId) return;
        const path = `/api/v1/seller/offers/${encodeURIComponent(offerId)}/withdraw`;
        api(path, {
          method: "POST",
          tokenRole: "seller",
          body: { seller_id: sellerId, revision, reason: "seller_withdrawn" },
          action: `POST ${path}`
        }).then((result) => result.ok && pollCatalog());
        return;
      }
      const button = event.target.closest("[data-action='seller-orders']");
      if (button) refreshOrders("seller");
      const catalogButton = event.target.closest("[data-action='seller-catalog']");
      if (catalogButton) refreshCatalog();
      const storeButton = event.target.closest("[data-action='seller-store']");
      if (storeButton) {
        const storeTab = $('.role-tab[href="#store"]');
        if (storeTab) storeTab.click();
      }
      const clearImage = event.target.closest("[data-product-image-clear]");
      if (clearImage) clearSellerImage();
    });
    refreshCatalog();
    refreshOrders("seller");
    window.setInterval(() => {
      if (document.visibilityState !== "hidden") {
        refreshCatalog();
        refreshOrders("seller");
      }
    }, 15000);
  }

  function bindBuyer() {
    const create = $('[data-form="buyer-create-order"]');
    const tools = $('[data-form="buyer-order-tools"]');
    create && create.addEventListener("submit", async (event) => {
      event.preventDefault();
      const payload = buyerOrder(create);
      if (!payload) {
        toast("Live offer unavailable", "error", "Load catalog and choose a projected offer before creating an order.");
        showResult("POST /api/v1/buyer/orders", "not-submitted", { code: "OFFER_NOT_PROJECTED", error: "Choose a live projected offer before creating an order." });
        return;
      }
      const result = await api("/api/v1/buyer/orders", { method: "POST", tokenRole: "buyer", body: payload, action: "POST /api/v1/buyer/orders" });
      if (result.ok) {
        setCheckoutOpen(false);
        markBuyerOrderPending(payload);
        await pollOrders("buyer", 12, { pendingOrderId: payload.order_id });
        if (create.elements.order_id) create.elements.order_id.value = protocolId("ord", LOCAL_INSTANCE, `ORDER_${Date.now().toString(36).toUpperCase()}`);
      }
    });
    tools && tools.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = form(tools);
      const rawOrderId = decodeURIComponent(data.order_id || DEMO.orderId);
      const orderId = encodeURIComponent(rawOrderId);
      const step = event.submitter ? event.submitter.dataset.step : "show";
      if (step === "cancel") {
        const result = await api(`/api/v1/buyer/orders/${orderId}/cancel`, { method: "POST", tokenRole: "buyer", body: { actor_id: currentCustomerId() }, action: "POST /api/v1/buyer/orders/{order_id}/cancel" });
        if (result.ok) await refreshOrders("buyer");
        return;
      }
      silentGet(`/api/v1/buyer/orders/${orderId}`, { tokenRole: "buyer", action: "GET /api/v1/buyer/orders/{order_id}" });
    });
    document.addEventListener("click", (event) => {
      if (event.target.closest("[data-checkout-close]") || event.target.closest("[data-checkout-overlay]")) {
        setCheckoutOpen(false);
        return;
      }
      const evmDeposit = event.target.closest("[data-evm-escrow-deposit]");
      if (evmDeposit) {
        const order = state.orders.find((item) => item.order_id === evmDeposit.dataset.orderId);
        requestEvmEscrowDeposit(order)
          .then((plan) => {
            showResult("EVM escrow deposit", "wallet_plan_ready", plan);
            toast("Wallet plan ready", "success", "Approve token spend, then submit the escrow deposit.");
          })
          .catch((error) => {
            showResult("EVM escrow deposit", "wallet_unavailable", { error: error.message });
            toast("Wallet unavailable", "error", error.message);
          });
        return;
      }
      const offerButton = event.target.closest("[data-select-offer]");
      if (offerButton) {
        const offer = state.offers.find((item) => item.offer_id === offerButton.dataset.selectOffer);
        if (offer && create) {
          state.selectedOffer = offer;
          setBuyerOrderFormFromOffer(create, offer);
          updateSelectedOfferDetail();
          renderCatalog("offers", state.offers);
          if (offerButton.hasAttribute("data-open-checkout")) setCheckoutOpen(true);
          else toast("Offer selected", "success", offer.offer_id);
        } else {
          toast("Live offer unavailable", "error", "Refresh catalog and choose a projected offer.");
        }
        return;
      }
      const button = event.target.closest("[data-action]");
      if (!button) return;
      if (button.dataset.action === "buyer-discover") {
        event.preventDefault();
        const discoverTab = $('.role-tab[href="#discover"]');
        if (discoverTab) discoverTab.click();
      }
      if (button.dataset.action === "buyer-catalog") refreshCatalog();
      if (button.dataset.action === "buyer-orders") refreshOrders("buyer");
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setCheckoutOpen(false);
    });
    refreshCatalog();
    refreshOrders("buyer");
    window.setInterval(() => {
      if (document.visibilityState !== "hidden") {
        refreshCatalog();
        refreshOrders("buyer");
      }
    }, 15000);
  }

  function init() {
    resultPanel = document.getElementById("result-panel");
    document.documentElement.dataset.morpheusUi = "ready";
    hydrateRuntimeDefaults();
    initTokens();
    initBuyerSettings();
    initRoleTabs();
    updateSelectedOfferDetail();
    const page = document.body.dataset.page;
    if (page === "admin") bindAdmin();
    if (page === "seller") bindSeller();
    if (page === "buyer") bindBuyer();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
