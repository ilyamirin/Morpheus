(function () {
  "use strict";

  const HASH_A = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const SELLER_TERMS_HASH = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
  const OFFER_TERMS_HASH = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
  const DEMO = {
    sellerId: "seller:shop.example:01JSELLER",
    productId: "prod:shop.example:01JPROD",
    offerId: "offer:shop.example:01JOFFER",
    customerId: "customer:shop.example:01JCUST",
    orderId: "ord:shop.example:01JORDER2",
    paymentId: "pay:shop.example:01JPAY",
    entitlementId: "ent:shop.example:01JENT"
  };
  const state = { sellers: [], products: [], offers: [], orders: [], selectedOffer: null };
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
  }

  function token(role) {
    const input = $(`[data-token="${role}"]`);
    return (input && (input.value.trim() || input.placeholder)) || `${role}-token`;
  }

  function form(formEl) {
    return Object.fromEntries(new FormData(formEl).entries());
  }

  function setText(idOrKey, text) {
    const el = document.getElementById(idOrKey) || $(`[data-text="${idOrKey}"]`);
    if (el) el.textContent = text;
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

  async function api(path, { method = "GET", tokenRole, body, action } = {}) {
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
      showResult(label, response.status, responseBody);
      toast(label, response.ok ? "success" : "error", `${response.status} ${response.statusText}`);
      return { ok: response.ok, status: response.status, body: responseBody };
    } catch (error) {
      const responseBody = { error: error.message, hint: "Server route may not be mounted yet." };
      showResult(label, "network-error", responseBody);
      toast(label, "error", error.message);
      return { ok: false, status: "network-error", body: responseBody };
    }
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
      legal_profile_ref: data.legal_profile_ref || "https://shop.example/legal",
      terms_ref: data.terms_ref || "https://shop.example/terms",
      terms_hash: HASH_A,
      supported_payment_adapters: ["mock"],
      supported_entitlement_types: ["external_entitlement"]
    };
  }

  function sellerProduct(formEl) {
    const data = form(formEl);
    return {
      seller_id: currentSellerId(),
      product_id: data.product_id || DEMO.productId,
      revision: int(data.revision, 1),
      title: data.title || "Morpheus Operations Seat",
      description: data.description || "Operator workspace with marketplace workflow controls.",
      kind: data.kind || "digital_service",
      categories: ["operations", "marketplace"],
      tags: ["morpheus", "operator", "poc"],
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
      price: { amount: data.amount || "100.00", currency: data.currency || "USD" },
      payment_capture_policy: data.payment_capture_policy || "before_entitlement",
      seller_terms_hash: SELLER_TERMS_HASH,
      offer_terms_hash: OFFER_TERMS_HASH,
      entitlement_type: "external_entitlement",
      availability_mode: "unlimited"
    };
  }

  function sellerOrder(step) {
    const actorId = currentSellerId();
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
        payment_id: DEMO.paymentId,
        adapter: "mock",
        amount: "100.00",
        currency: "USD",
        capture_policy: "before_entitlement",
        idempotency_key: "idem:shop.example:01JPAY",
        provider_ref: "mock:pi_01JPAY",
        confirmation: { method: "redirect", uri: "https://shop.example/pay/confirm" },
        expires_at: "2026-05-04T10:30:00Z"
      };
    }
    if (step === "payment-capture") {
      return {
        actor_id: actorId,
        payment_id: DEMO.paymentId,
        adapter: "mock",
        amount: "100.00",
        currency: "USD",
        provider_ref: "mock:cap_01JPAY",
        evidence: { captured: true, source: "seller-ui-poc" }
      };
    }
    if (step === "entitlement-grant") {
      return {
        actor_id: actorId,
        payment_id: DEMO.paymentId,
        entitlement_id: DEMO.entitlementId,
        entitlement_type: "external_entitlement",
        external_ref: "https://shop.example/entitlements/01JENT",
        evidence: { granted: true, source: "seller-ui-poc" }
      };
    }
    return { actor_id: actorId };
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
    const offer = selectedOffer(formEl);
    const body = (offer && offer.body) || {};
    const price = (offer && offer.price) || { amount: data.amount || "100.00", currency: data.currency || "USD" };
    return {
      customer_id: data.customer_id || DEMO.customerId,
      customer_display_name: "Fixture Customer",
      order_id: data.order_id || DEMO.orderId,
      room_id: data.room_id || "!order2:shop.example",
      seller_id: (offer && offer.seller_id) || DEMO.sellerId,
      offer_id: (offer && offer.offer_id) || data.offer_id || DEMO.offerId,
      offer_revision: int((offer && offer.revision) || body.revision, 1),
      catalog_snapshot_id: "snap:shop.example:01JSNAP",
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
      expires_at: "2026-05-04T10:30:00Z"
    };
  }

  function statusBadge(status) {
    const value = String(status || "unknown");
    let accent = "accent-cyan";
    if (value.includes("cancel") || value.includes("reject")) accent = "accent-crimson";
    if (value.includes("created") || value.includes("accepted")) accent = "accent-amber";
    if (value.includes("complete") || value.includes("grant")) accent = "accent-emerald";
    return `<span class="status-pill ${accent}">${esc(value)}</span>`;
  }

  function renderAdminSummary(body) {
    const target = document.getElementById("admin-summary-cards");
    const summary = body || {};
    const catalog = summary.catalog || {};
    if (!target) return;
    const items = [
      ["Sellers", catalog.sellers || 0, "accent-emerald"],
      ["Products", catalog.products || 0, "accent-cyan"],
      ["Offers", catalog.offers || 0, "accent-amber"],
      ["Tombstones", catalog.tombstones || 0, "accent-crimson"],
      ["Orders", summary.orders || 0, "accent-cyan"],
      ["Payments", summary.payments || 0, "accent-emerald"],
      ["Entitlements", summary.entitlements || 0, "accent-emerald"],
      ["Disputes", summary.disputes || 0, "accent-crimson"],
      ["Rulings", summary.arbitration_rulings || 0, "accent-amber"]
    ];
    target.innerHTML = items.map(([label, value, accent]) =>
      `<div class="metric-card"><span class="status-pill ${accent}">${esc(label)}</span><strong>${esc(value)}</strong></div>`
    ).join("");
    setText("admin-catalog-counts", `${catalog.sellers || 0} / ${catalog.products || 0} / ${catalog.offers || 0}`);
    setText("admin-order-counts", `${summary.orders || 0} orders`);
  }

  function renderAdminAllowlist(body) {
    const target = document.getElementById("admin-allowlist-view");
    const items = Array.isArray(body && body.allowlist) ? body.allowlist : [];
    if (!target) return;
    if (!items.length) {
      target.innerHTML = `<div class="empty-state">Allowlist is intentionally empty. Source: ${esc(body && body.source || "unknown")}; configured: ${esc(body && body.configured)}</div>`;
      return;
    }
    target.innerHTML = items.map((item) =>
      `<article class="list-item"><strong>${esc(item.instance_id || "instance")}</strong><span>${esc(JSON.stringify(item))}</span></article>`
    ).join("");
  }

  function renderAdminEvents(body) {
    const rows = document.getElementById("admin-events-rows");
    const events = Array.isArray(body && body.events) ? body.events : [];
    if (!rows) return;
    if (!events.length) {
      rows.innerHTML = '<tr><td colspan="3" class="empty-cell">No projection errors are recorded.</td></tr>';
      return;
    }
    rows.innerHTML = events.map((event) =>
      `<tr><td>${esc(event.code || "unknown")}</td><td>${esc(event.message || "")}</td><td class="mono">${esc(event.matrix_event_id || "")}</td></tr>`
    ).join("");
  }

  function renderCatalog(kind, items) {
    const target = document.getElementById(`buyer-${kind}`);
    if (!target) return;
    if (!items.length) {
      target.innerHTML = `<div class="empty-state">No ${esc(kind)} found. Refresh after projection data exists.</div>`;
      return;
    }
    target.innerHTML = items.map((item) => {
      const id = item.seller_id || item.product_id || item.offer_id || item.id || "item";
      const title = pick(item, ["body", "title"], item.display_name || item.status || id);
      const extra = kind === "offers" ? `${pick(item, ["price", "amount"], "0")} ${pick(item, ["price", "currency"], "")}` : id;
      const button = kind === "offers" ? `<button class="btn btn-small" data-select-offer="${esc(item.offer_id || "")}">Use offer</button>` : "";
      const selected = kind === "offers" && state.selectedOffer && state.selectedOffer.offer_id === item.offer_id ? " is-selected" : "";
      return `<article class="list-item catalog-item${selected}"><div><strong>${esc(title)}</strong><span class="mono">${esc(extra)}</span></div>${button}</article>`;
    }).join("");
  }

  function renderOrders(rowsId, countId, columns) {
    const rows = document.getElementById(rowsId);
    if (!rows) return;
    if (countId) setText(countId, `${state.orders.length} orders`);
    if (!state.orders.length) {
      rows.innerHTML = `<tr><td colspan="${columns}" class="empty-cell">No orders found. Create one from the buyer workspace, then refresh.</td></tr>`;
      return;
    }
    rows.innerHTML = state.orders.map((order) => {
      if (columns === 5) {
        return `<tr><td class="mono">${esc(order.order_id)}</td><td>${statusBadge(order.status)}</td><td class="mono">${esc(order.customer_id)}</td><td class="mono">${esc(order.offer_id)}</td><td class="mono">${esc(order.room_id)}</td></tr>`;
      }
      return `<tr><td class="mono">${esc(order.order_id)}</td><td>${statusBadge(order.status)}</td><td class="mono">${esc(order.offer_id)}</td></tr>`;
    }).join("");
  }

  async function refreshAdmin() {
    const health = await api("/healthz", { action: "GET /healthz" });
    setText("admin-health-status", health.ok ? ((health.body && health.body.status) || "ok") : "error");
    const ready = await api("/readyz", { action: "GET /readyz" });
    setText("admin-ready-status", ready.ok ? ((ready.body && ready.body.status) || "ready") : "error");
    await api("/admin/config", { tokenRole: "admin", action: "GET /admin/config" });
    const allowlist = await api("/admin/allowlist", { tokenRole: "admin", action: "GET /admin/allowlist" });
    if (allowlist.ok) renderAdminAllowlist(allowlist.body);
    const summary = await api("/admin/projections/summary", { tokenRole: "admin", action: "GET /admin/projections/summary" });
    if (summary.ok) renderAdminSummary(summary.body);
    const events = await api("/admin/events", { tokenRole: "admin", action: "GET /admin/events" });
    if (events.ok) renderAdminEvents(events.body);
  }

  async function refreshCatalog() {
    const sellers = await api("/api/v1/catalog/sellers", { action: "GET /api/v1/catalog/sellers" });
    if (sellers.ok) {
      state.sellers = Array.isArray(sellers.body.items) ? sellers.body.items : [];
      renderCatalog("sellers", state.sellers);
    }
    const products = await api("/api/v1/catalog/products", { action: "GET /api/v1/catalog/products" });
    if (products.ok) {
      state.products = Array.isArray(products.body.items) ? products.body.items : [];
      renderCatalog("products", state.products);
    }
    const offers = await api("/api/v1/catalog/offers", { action: "GET /api/v1/catalog/offers" });
    if (offers.ok) {
      state.offers = Array.isArray(offers.body.items) ? offers.body.items : [];
      renderCatalog("offers", state.offers);
    }
  }

  async function refreshOrders(role) {
    const path = role === "seller" ? "/api/v1/seller/orders" : "/api/v1/buyer/orders";
    const result = await api(path, { tokenRole: role, action: `GET ${path}` });
    if (!result.ok) return;
    state.orders = Array.isArray(result.body && result.body.orders) ? result.body.orders : [];
    if (role === "seller") renderOrders("seller-orders-rows", "seller-order-count", 5);
    if (role === "buyer") renderOrders("buyer-orders-rows", "buyer-order-count", 3);
  }

  function bindAdmin() {
    document.addEventListener("click", async (event) => {
      const button = event.target.closest("[data-action], [data-refresh]");
      if (!button) return;
      if (button.dataset.refresh === "admin") return refreshAdmin();
      if (button.dataset.action === "admin-health") {
        const result = await api("/healthz", { action: "GET /healthz" });
        return setText("admin-health-status", result.ok ? ((result.body && result.body.status) || "ok") : "error");
      }
      if (button.dataset.action === "admin-ready") {
        const result = await api("/readyz", { action: "GET /readyz" });
        return setText("admin-ready-status", result.ok ? ((result.body && result.body.status) || "ready") : "error");
      }
      if (button.dataset.action === "admin-config") return api("/admin/config", { tokenRole: "admin", action: "GET /admin/config" });
      if (button.dataset.action === "admin-allowlist") {
        const result = await api("/admin/allowlist", { tokenRole: "admin", action: "GET /admin/allowlist" });
        if (result.ok) renderAdminAllowlist(result.body);
      }
      if (button.dataset.action === "admin-summary") {
        const result = await api("/admin/projections/summary", { tokenRole: "admin", action: "GET /admin/projections/summary" });
        if (result.ok) renderAdminSummary(result.body);
      }
      if (button.dataset.action === "admin-events") {
        const result = await api("/admin/events", { tokenRole: "admin", action: "GET /admin/events" });
        if (result.ok) renderAdminEvents(result.body);
      }
      if (button.dataset.action === "admin-rebuild") {
        const result = await api("/admin/catalog/rebuild", { method: "POST", tokenRole: "admin", body: {}, action: "POST /admin/catalog/rebuild" });
        if (result.ok && result.body && result.body.catalog) renderAdminSummary({ catalog: result.body.catalog, orders: 0, payments: 0, entitlements: 0, disputes: 0, arbitration_rulings: 0 });
      }
    });
    const replay = $('[data-form="admin-replay"]');
    replay && replay.addEventListener("submit", (event) => {
      event.preventDefault();
      const data = form(replay);
      api(`/admin/orders/${encodeURIComponent(data.order_id || DEMO.orderId)}/replay`, { method: "POST", tokenRole: "admin", body: {}, action: "POST /admin/orders/{order_id}/replay" });
    });
  }

  function bindSeller() {
    const announce = $('[data-form="seller-announce"]');
    const product = $('[data-form="seller-product"]');
    const offer = $('[data-form="seller-offer"]');
    const order = $('[data-form="seller-order-action"]');
    announce && announce.addEventListener("submit", (event) => {
      event.preventDefault();
      api("/api/v1/seller/announce", { method: "POST", tokenRole: "seller", body: sellerAnnounce(announce), action: "POST /api/v1/seller/announce" });
    });
    product && product.addEventListener("submit", (event) => {
      event.preventDefault();
      api("/api/v1/seller/products", { method: "POST", tokenRole: "seller", body: sellerProduct(product), action: "POST /api/v1/seller/products" });
    });
    offer && offer.addEventListener("submit", (event) => {
      event.preventDefault();
      api("/api/v1/seller/offers", { method: "POST", tokenRole: "seller", body: sellerOffer(offer), action: "POST /api/v1/seller/offers" });
    });
    order && order.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = form(order);
      const step = event.submitter ? event.submitter.dataset.step : "accept";
      const orderId = encodeURIComponent(data.order_id || DEMO.orderId);
      const path = step === "complete" ? `/api/v1/seller/orders/${orderId}/complete` : `/api/v1/seller/orders/${orderId}/${step}`;
      const result = await api(path, { method: "POST", tokenRole: "seller", body: sellerOrder(step), action: `POST ${path}` });
      if (result.ok) await refreshOrders("seller");
    });
    document.addEventListener("click", (event) => {
      const button = event.target.closest("[data-action='seller-orders']");
      if (button) refreshOrders("seller");
    });
  }

  function bindBuyer() {
    const create = $('[data-form="buyer-create-order"]');
    const tools = $('[data-form="buyer-order-tools"]');
    create && create.addEventListener("submit", async (event) => {
      event.preventDefault();
      const result = await api("/api/v1/buyer/orders", { method: "POST", tokenRole: "buyer", body: buyerOrder(create), action: "POST /api/v1/buyer/orders" });
      if (result.ok) await refreshOrders("buyer");
    });
    tools && tools.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = form(tools);
      const orderId = encodeURIComponent(data.order_id || DEMO.orderId);
      const step = event.submitter ? event.submitter.dataset.step : "show";
      if (step === "cancel") {
        const result = await api(`/api/v1/buyer/orders/${orderId}/cancel`, { method: "POST", tokenRole: "buyer", body: { actor_id: currentCustomerId() }, action: "POST /api/v1/buyer/orders/{order_id}/cancel" });
        if (result.ok) await refreshOrders("buyer");
        return;
      }
      api(`/api/v1/buyer/orders/${orderId}`, { tokenRole: "buyer", action: "GET /api/v1/buyer/orders/{order_id}" });
    });
    document.addEventListener("click", (event) => {
      const offerButton = event.target.closest("[data-select-offer]");
      if (offerButton) {
        const offer = state.offers.find((item) => item.offer_id === offerButton.dataset.selectOffer);
        if (offer && create) {
          state.selectedOffer = offer;
          create.elements.offer_id.value = offer.offer_id || DEMO.offerId;
          create.elements.amount.value = pick(offer, ["price", "amount"], "100.00");
          create.elements.currency.value = pick(offer, ["price", "currency"], "USD");
          renderCatalog("offers", state.offers);
          toast("Offer selected", "success", offer.offer_id);
        }
        return;
      }
      const button = event.target.closest("[data-action]");
      if (!button) return;
      if (button.dataset.action === "buyer-catalog") refreshCatalog();
      if (button.dataset.action === "buyer-orders") refreshOrders("buyer");
    });
  }

  function init() {
    resultPanel = document.getElementById("result-panel");
    document.documentElement.dataset.morpheusUi = "ready";
    initTokens();
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
