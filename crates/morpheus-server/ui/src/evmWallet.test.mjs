import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  evmEscrowConfirmation,
  escrowPolicyHint,
  buildDepositCalls,
  buildReleaseCall,
  buildRefundCall,
  buildPartialRefundCall,
  classifyWalletError,
  requestEvmEscrowDeposit,
  requireRoleWallet,
  walletChainFromConfirmation
} from "./evmWallet.js";

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

assert.equal(
  evmEscrowConfirmation(order).order_hash,
  order.payment.body.confirmation.order_hash
);

const walletChain = walletChainFromConfirmation(order.payment.body.confirmation);
assert.equal(walletChain.id, 31337);
assert.equal(walletChain.name, "Morpheus EVM escrow 31337");
assert.deepEqual(walletChain.nativeCurrency, { name: "Ether", symbol: "ETH", decimals: 18 });
assert.deepEqual(walletChain.rpcUrls, { default: { http: ["http://127.0.0.1"] } });
assert.throws(
  () => walletChainFromConfirmation({}),
  /EVM chain id is not available for this order/
);
assert.throws(
  () => walletChainFromConfirmation({ chain_id: 0 }),
  /EVM chain id is not available for this order/
);

const calls = buildDepositCalls(order, "0x0000000000000000000000000000000000000004");
assert.equal(calls.approve.address, order.payment.body.confirmation.token);
assert.equal(calls.deposit.address, order.payment.body.confirmation.escrow_contract);
assert.equal(calls.deposit.functionName, "deposit");
assert.equal(calls.deposit.args[0], order.payment.body.confirmation.order_hash);
assert.equal(calls.deposit.args[2], 25000000n);

const release = buildReleaseCall(order);
assert.equal(release.address, order.payment.body.confirmation.escrow_contract);
assert.equal(release.functionName, "release");
assert.deepEqual(release.args, [order.payment.body.confirmation.order_hash]);

const refund = buildRefundCall(order);
assert.equal(refund.address, order.payment.body.confirmation.escrow_contract);
assert.equal(refund.functionName, "refund");
assert.deepEqual(refund.args, [order.payment.body.confirmation.order_hash]);

const partialRefund = buildPartialRefundCall(order, "10000000");
assert.equal(partialRefund.address, order.payment.body.confirmation.escrow_contract);
assert.equal(partialRefund.functionName, "partial_refund");
assert.deepEqual(partialRefund.args, [order.payment.body.confirmation.order_hash, 10000000n]);

assert.equal(
  requireRoleWallet("buyer", "0x0000000000000000000000000000000000000004", order.payment.body.confirmation),
  "0x0000000000000000000000000000000000000004"
);
assert.throws(
  () => requireRoleWallet("seller", "0x0000000000000000000000000000000000000004", order.payment.body.confirmation),
  /Expected seller wallet 0x0000000000000000000000000000000000000003/
);
assert.throws(
  () => requireRoleWallet("buyer", "", order.payment.body.confirmation),
  /Connected wallet address is not available/
);
for (const role of ["buyer", "seller", "arbiter"]) {
  const confirmation = { ...order.payment.body.confirmation };
  delete confirmation[`${role}_evm_address`];
  assert.throws(
    () => requireRoleWallet(role, "0x0000000000000000000000000000000000000004", confirmation),
    new RegExp(`Expected ${role} wallet is not available`)
  );
}

assert.deepEqual(classifyWalletError(new Error("User rejected the request.")), {
  code: "wallet_rejected",
  message: "User rejected the request."
});
assert.deepEqual(classifyWalletError(new Error("wallet_switchEthereumChain failed")), {
  code: "chain_mismatch",
  message: "wallet_switchEthereumChain failed"
});
assert.deepEqual(classifyWalletError(new Error("wrong chain")), {
  code: "chain_mismatch",
  message: "wrong chain"
});
assert.deepEqual(classifyWalletError(new Error("unsupported chain")), {
  code: "chain_mismatch",
  message: "unsupported chain"
});
assert.deepEqual(classifyWalletError(new Error("chain id 31337 required")), {
  code: "chain_mismatch",
  message: "chain id 31337 required"
});
assert.deepEqual(classifyWalletError(new Error("RPC timeout")), {
  code: "wallet_error",
  message: "RPC timeout"
});
assert.deepEqual(classifyWalletError(new Error("chainmail timeout")), {
  code: "wallet_error",
  message: "chainmail timeout"
});

const wrongRoleRequestLog = [];
const wrongRoleEthereum = {
  async request({ method }) {
    wrongRoleRequestLog.push(method);
    if (method === "eth_requestAccounts") return ["0x0000000000000000000000000000000000000003"];
    throw new Error(`unexpected provider request: ${method}`);
  }
};
await assert.rejects(
  () => requestEvmEscrowDeposit(order, wrongRoleEthereum),
  /Expected buyer wallet 0x0000000000000000000000000000000000000004/
);
assert.ok(!wrongRoleRequestLog.includes("wallet_switchEthereumChain"));
assert.ok(!wrongRoleRequestLog.includes("eth_sendTransaction"));
assert.ok(!wrongRoleRequestLog.includes("wallet_sendTransaction"));

const longFee = "9".repeat(120);
const maliciousSymbol = "<img src=x onerror=alert(1)>";
assert.equal(
  escrowPolicyHint({
    policy: {
      deposit_timeout_secs: 900,
      buyer_review_timeout_secs: 3600
    },
    fee_hint: {
      estimated_fee_units: longFee,
      fee_token_symbol: maliciousSymbol
    }
  }),
  `Deposit window: 15 min | Buyer review: 1 h | Estimated network fee: ${"9".repeat(93)}... <img src=x onerror=al... units`
);

assert.equal(
  escrowPolicyHint({
    policy: { deposit_timeout_secs: 0 },
    fee_hint: { estimated_fee_units: Number.POSITIVE_INFINITY, fee_token_symbol: "ETH" }
  }),
  ""
);

const appSource = readFileSync(fileURLToPath(new URL("./app.js", import.meta.url)), "utf8");
assert.match(appSource, /<span class="muted-text">\$\{esc\(hint\)\}<\/span>/);
