import assert from "node:assert/strict";
import {
  evmEscrowConfirmation,
  buildDepositCalls,
  buildReleaseCall,
  buildRefundCall,
  buildPartialRefundCall
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
