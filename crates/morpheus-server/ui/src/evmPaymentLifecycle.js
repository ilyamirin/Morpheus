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
  const base = String(network?.explorer_base_url || "").trim();
  const id = value === undefined || value === null ? "" : String(value).trim();
  if (!base || !id) return "";
  let url;
  try {
    url = new URL(base);
  } catch {
    return "";
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return "";
  const encodedId = encodeURIComponent(id);
  const pathBase = url.pathname.replace(/\/+$/, "");
  if (kind === "tx") url.pathname = `${pathBase}/tx/${encodedId}`;
  else if (kind === "address") url.pathname = `${pathBase}/address/${encodedId}`;
  else if (kind === "token") url.pathname = `${pathBase}/token/${encodedId}`;
  else return "";
  url.search = "";
  url.hash = "";
  return url.toString();
}

function rowValue(value) {
  return value === undefined || value === null ? "" : String(value);
}

function hasRowValue(row) {
  return row.value !== "";
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
    { label: "Chain", value: rowValue(confirmation.chain_id) },
    {
      label: "Escrow contract",
      value: rowValue(confirmation.escrow_contract),
      href: buildExplorerLink(network, "address", confirmation.escrow_contract)
    },
    {
      label: "Token",
      value: rowValue(confirmation.token),
      href: buildExplorerLink(network, "token", confirmation.token)
    },
    { label: "Amount units", value: rowValue(confirmation.amount_units) },
    { label: "Order hash", value: rowValue(confirmation.order_hash) },
    { label: "Confirmations", value: rowValue(confirmation?.fee_hint?.confirmations) },
    { label: "Watcher", value: watcherStatusLabel(watcher) }
  ];
  if (txHash) rows.push({ label: "Pending tx", value: txHash, href: buildExplorerLink(network, "tx", txHash) });
  return rows.filter(hasRowValue);
}
