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
    const category = data.kind || "marketplace";
    return {
      seller_id: currentSellerId(),
      product_id: data.product_id || DEMO.productId,
      revision: int(data.revision, 1),
      title: data.title || "Soft Runner",
      description: data.description || "Operator workspace with marketplace workflow controls.",
      kind: "digital_service",
      categories: [category, "marketplace"],
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
    if (value.includes("created") || value.includes("accepted")) accent = "accent-amber";
    if (value.includes("complete") || value.includes("grant")) accent = "accent-emerald";
    return `<span class="status-pill ${accent}">${esc(value)}</span>`;
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
    const productId = item && item.product_id;
    if (productId && SEEDED_PRODUCT_IMAGES[productId]) return SEEDED_PRODUCT_IMAGES[productId];
    return PRODUCT_IMAGES[productKind(item)] || PRODUCT_IMAGES.sneakers;
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
    if (kind === "offers") {
      target.innerHTML = items.map((item) => {
        const title = offerTitle(item);
        const seller = sellerName(item.seller_id);
        const selected = state.selectedOffer && state.selectedOffer.offer_id === item.offer_id ? " is-selected" : "";
        const description = pick(productForOffer(item), ["body", "description"], "Trusted marketplace offer ready for checkout.");
        return `<article class="product-card${selected}" data-catalog-kind="offers" data-catalog-id="${esc(item.offer_id || "")}">
          <img src="${esc(offerImage(item))}" alt="${esc(title)}">
          <div class="product-card-body">
            <span class="product-meta">${esc(itemInstance(item))} · ${esc(seller)}</span>
            <h3>${esc(title)}</h3>
            <p>${esc(description)}</p>
            <div class="product-card-footer">
              <strong>${esc(offerPrice(item))}</strong>
              <button class="btn btn-primary" data-select-offer="${esc(item.offer_id || "")}" data-open-checkout>Buy</button>
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

  function renderSellerStoreCards() {
    const target = document.getElementById("seller-store-cards");
    if (!target) return;
    const localOffers = state.offers.filter((offer) => objectInstance(offer.seller_id || offer.offer_id) === LOCAL_INSTANCE);
    if (!localOffers.length) return;
    target.innerHTML = localOffers.map((offer) => {
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
            <span>Live</span>
          </div>
        </div>
      </article>`;
    }).join("");
  }

  function orderTimeline(order) {
    const status = String(order.status || "created");
    const steps = [
      ["Created", "Order terms were submitted by the buyer.", true],
      ["Accepted", "Seller confirms the offer revision and terms.", /accepted|authorized|captured|grant|complete/.test(status)],
      ["Payment", "Mock adapter records intent and capture evidence.", /payment|captured|grant|complete/.test(status)],
      ["Entitlement", "Access evidence is granted before completion.", /entitlement|grant|complete/.test(status)],
      ["Complete", "The order lifecycle is projected as complete.", /complete/.test(status)]
    ];
    return `<ol class="timeline-list compact-timeline">${steps.map(([label, detail, active]) =>
      `<li class="timeline-step"><span>${active ? statusBadge(label.toLowerCase()) : ""}<strong>${esc(label)}</strong><span>${esc(detail)}</span></span></li>`
    ).join("")}</ol>`;
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

  function renderOrders(rowsId, countId, columns) {
    const rows = document.getElementById(rowsId);
    if (!rows) return;
    if (countId) setText(countId, `${state.orders.length} orders`);
    const cards = ensureOrderCards(rows, rowsId);
    if (!state.orders.length) {
      rows.innerHTML = `<tr><td colspan="${columns}" class="empty-cell">No orders found. Create one from the buyer workspace, then refresh.</td></tr>`;
      if (cards) cards.innerHTML = `<div class="empty-state">No orders loaded yet.</div>`;
      return;
    }
    if (cards) {
      cards.innerHTML = state.orders.map((order) => {
        const title = displayId(order.order_id, "Order");
        const offer = displayId(order.offer_id, "Offer not attached");
        const actor = columns === 5 ? displayId(order.customer_id, "Customer not attached") : sellerName(order.seller_id);
        const sellerActions = columns === 5 ? `<div class="button-row stretch order-action-row">
          <button class="btn btn-small" type="button" data-seller-order-step="accept" data-order-id="${esc(order.order_id || "")}">Accept</button>
          <button class="btn btn-small" type="button" data-seller-order-step="payment-intent" data-order-id="${esc(order.order_id || "")}">Intent</button>
          <button class="btn btn-small" type="button" data-seller-order-step="payment-capture" data-order-id="${esc(order.order_id || "")}">Capture</button>
          <button class="btn btn-small" type="button" data-seller-order-step="entitlement-grant" data-order-id="${esc(order.order_id || "")}">Grant</button>
          <button class="btn btn-small btn-primary" type="button" data-seller-order-step="complete" data-order-id="${esc(order.order_id || "")}">Complete</button>
        </div>` : "";
        return `<article class="order-card"><div class="section-head compact-head"><div><p class="eyebrow">${esc(actor)}</p><h3>${esc(title)}</h3><p class="mono">${esc(offer)}</p></div>${statusBadge(order.status)}</div>${orderTimeline(order)}${sellerActions}</article>`;
      }).join("");
    }
    rows.innerHTML = state.orders.map((order) => {
      if (columns === 5) {
        return `<tr><td class="mono">${esc(order.order_id)}</td><td>${statusBadge(order.status)}</td><td class="mono">${esc(order.customer_id)}</td><td class="mono">${esc(order.offer_id)}</td><td class="mono">${esc(order.room_id)}</td></tr>`;
      }
      return `<tr><td class="mono">${esc(order.order_id)}</td><td>${statusBadge(order.status)}</td><td class="mono">${esc(order.offer_id)}</td></tr>`;
    }).join("");
  }

  async function refreshAdmin({ silent = true } = {}) {
    const requestOptions = { silent, result: !silent };
    const health = await api("/healthz", { action: "GET /healthz", ...requestOptions });
    setText("admin-health-status", health.ok ? ((health.body && health.body.status) || "ok") : "error");
    const ready = await api("/readyz", { action: "GET /readyz", ...requestOptions });
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
      if (!state.selectedOffer && state.offers.length) state.selectedOffer = state.offers[0];
      renderCatalog("offers", state.offers);
      renderSellerStoreCards();
      updateSelectedOfferDetail();
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

  function delay(ms) {
    return new Promise((resolve) => window.setTimeout(resolve, ms));
  }

  async function pollCatalog(attempts = 8) {
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      await refreshCatalog();
      await delay(700);
    }
  }

  async function pollOrders(role, attempts = 10) {
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      await refreshOrders(role);
      await delay(800);
    }
  }

  function bindAdmin() {
    document.addEventListener("click", async (event) => {
      const button = event.target.closest("[data-action], [data-refresh]");
      if (!button) return;
      if (button.dataset.refresh === "admin") return refreshAdmin({ silent: false });
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
    refreshAdmin();
    window.setInterval(() => {
      if (document.visibilityState !== "hidden") refreshAdmin();
    }, 10000);
  }

  function bindSeller() {
    const announce = $('[data-form="seller-announce"]');
    const product = $('[data-form="seller-product"]');
    const offer = $('[data-form="seller-offer"]');
    const order = $('[data-form="seller-order-action"]');
    announce && announce.addEventListener("submit", async (event) => {
      event.preventDefault();
      const result = await api("/api/v1/seller/announce", { method: "POST", tokenRole: "seller", body: sellerAnnounce(announce), action: "POST /api/v1/seller/announce" });
      if (result.ok) await pollCatalog();
    });
    product && product.addEventListener("submit", async (event) => {
      event.preventDefault();
      const result = await api("/api/v1/seller/products", { method: "POST", tokenRole: "seller", body: sellerProduct(product), action: "POST /api/v1/seller/products" });
      if (result.ok) await pollCatalog();
    });
    offer && offer.addEventListener("submit", async (event) => {
      event.preventDefault();
      const result = await api("/api/v1/seller/offers", { method: "POST", tokenRole: "seller", body: sellerOffer(offer), action: "POST /api/v1/seller/offers" });
      if (result.ok) await pollCatalog();
    });
    order && order.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = form(order);
      const step = event.submitter ? event.submitter.dataset.step : "accept";
      const rawOrderId = decodeURIComponent(data.order_id || DEMO.orderId);
      const orderId = encodeURIComponent(rawOrderId);
      const path = step === "complete" ? `/api/v1/seller/orders/${orderId}/complete` : `/api/v1/seller/orders/${orderId}/${step}`;
      const result = await api(path, { method: "POST", tokenRole: "seller", body: sellerOrder(step, rawOrderId), action: `POST ${path}` });
      if (result.ok) await refreshOrders("seller");
    });
    document.addEventListener("click", (event) => {
      const orderAction = event.target.closest("[data-seller-order-step]");
      if (orderAction && order) {
        const orderId = orderAction.dataset.orderId || DEMO.orderId;
        if (order.elements.order_id) order.elements.order_id.value = orderId;
        const step = orderAction.dataset.sellerOrderStep || "accept";
        const pathId = encodeURIComponent(orderId);
        const path = step === "complete" ? `/api/v1/seller/orders/${pathId}/complete` : `/api/v1/seller/orders/${pathId}/${step}`;
        api(path, { method: "POST", tokenRole: "seller", body: sellerOrder(step, orderId), action: `POST ${path}` })
          .then((result) => result.ok && pollOrders("seller"));
        return;
      }
      const button = event.target.closest("[data-action='seller-orders']");
      if (button) refreshOrders("seller");
    });
    refreshCatalog();
    refreshOrders("seller");
  }

  function bindBuyer() {
    const create = $('[data-form="buyer-create-order"]');
    const tools = $('[data-form="buyer-order-tools"]');
    create && create.addEventListener("submit", async (event) => {
      event.preventDefault();
      const payload = buyerOrder(create);
      if (!payload) {
        toast("Refresh catalog first", "error", "Choose a real projected offer before creating an order.");
        showResult("POST /api/v1/buyer/orders", "not-submitted", { code: "OFFER_NOT_PROJECTED", error: "Choose a real projected offer before creating an order." });
        return;
      }
      const result = await api("/api/v1/buyer/orders", { method: "POST", tokenRole: "buyer", body: payload, action: "POST /api/v1/buyer/orders" });
      if (result.ok) {
        setCheckoutOpen(false);
        await pollOrders("buyer");
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
      api(`/api/v1/buyer/orders/${orderId}`, { tokenRole: "buyer", action: "GET /api/v1/buyer/orders/{order_id}" });
    });
    document.addEventListener("click", (event) => {
      if (event.target.closest("[data-checkout-close]") || event.target.closest("[data-checkout-overlay]")) {
        setCheckoutOpen(false);
        return;
      }
      const offerButton = event.target.closest("[data-select-offer]");
      if (offerButton) {
        const offer = state.offers.find((item) => item.offer_id === offerButton.dataset.selectOffer) || offerFromProductCard(offerButton.closest(".product-card"), offerButton.dataset.selectOffer) || {
          offer_id: offerButton.dataset.selectOffer || DEMO.offerId,
          product_id: DEMO.productId,
          seller_id: DEMO.sellerId,
          price: { amount: "100.00", currency: "USD" },
          body: { title: "Soft Runner" }
        };
        if (offer && create) {
          state.selectedOffer = offer;
          setBuyerOrderFormFromOffer(create, offer);
          updateSelectedOfferDetail();
          renderCatalog("offers", state.offers);
          if (offerButton.hasAttribute("data-open-checkout")) setCheckoutOpen(true);
          else toast("Offer selected", "success", offer.offer_id);
        }
        return;
      }
      const demoBuy = event.target.closest("[data-demo-buy]");
      if (demoBuy) {
        state.selectedOffer = offerFromProductCard(demoBuy.closest(".product-card")) || {
          offer_id: DEMO.offerId,
          product_id: DEMO.productId,
          seller_id: DEMO.sellerId,
          price: { amount: "100.00", currency: "USD" },
          body: { title: "Soft Runner" }
        };
        setBuyerOrderFormFromOffer(create, state.selectedOffer);
        updateSelectedOfferDetail();
        setCheckoutOpen(true);
        return;
      }
      const button = event.target.closest("[data-action]");
      if (!button) return;
      if (button.dataset.action === "buyer-catalog") refreshCatalog();
      if (button.dataset.action === "buyer-orders") refreshOrders("buyer");
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setCheckoutOpen(false);
    });
    refreshCatalog();
    refreshOrders("buyer");
  }

  function init() {
    resultPanel = document.getElementById("result-panel");
    document.documentElement.dataset.morpheusUi = "ready";
    hydrateRuntimeDefaults();
    initTokens();
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
