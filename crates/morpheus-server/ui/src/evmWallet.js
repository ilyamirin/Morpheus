import { createWalletClient, custom } from "viem";

export const erc20Abi = [
  {
    type: "function",
    name: "approve",
    stateMutability: "nonpayable",
    inputs: [
      { name: "spender", type: "address" },
      { name: "amount", type: "uint256" }
    ],
    outputs: [{ name: "", type: "bool" }]
  }
];

export const escrowAbi = [
  {
    type: "function",
    name: "deposit",
    stateMutability: "nonpayable",
    inputs: [
      { name: "order_hash", type: "bytes32" },
      { name: "token", type: "address" },
      { name: "amount", type: "uint256" },
      { name: "seller", type: "address" },
      { name: "buyer", type: "address" },
      { name: "arbiter", type: "address" }
    ],
    outputs: []
  },
  {
    type: "function",
    name: "release",
    stateMutability: "nonpayable",
    inputs: [{ name: "order_hash", type: "bytes32" }],
    outputs: []
  },
  {
    type: "function",
    name: "refund",
    stateMutability: "nonpayable",
    inputs: [{ name: "order_hash", type: "bytes32" }],
    outputs: []
  },
  {
    type: "function",
    name: "partial_refund",
    stateMutability: "nonpayable",
    inputs: [
      { name: "order_hash", type: "bytes32" },
      { name: "buyer_amount", type: "uint256" }
    ],
    outputs: []
  }
];

export function evmEscrowConfirmation(order) {
  return order?.payment?.body?.confirmation
    || order?.payment?.confirmation
    || order?.body?.payment_confirmation
    || order?.body?.confirmation
    || null;
}

export function requireConfirmation(order) {
  const confirmation = evmEscrowConfirmation(order);
  if (!confirmation) throw new Error("EVM escrow confirmation is not available for this order");
  return confirmation;
}

export function requireEthereum(ethereum) {
  if (!ethereum) throw new Error("EVM wallet is not available");
  return ethereum;
}

export function formatDurationHint(seconds) {
  if (!Number.isFinite(Number(seconds)) || Number(seconds) <= 0) return "";
  const value = Number(seconds);
  if (value % 3600 === 0) return `${value / 3600} h`;
  if (value % 60 === 0) return `${value / 60} min`;
  return `${value} sec`;
}

export function feeHintTextValue(value, maxLength = 96) {
  const valueType = typeof value;
  if (valueType !== "string" && valueType !== "number" && valueType !== "bigint") return "";
  if (valueType === "number" && !Number.isFinite(value)) return "";
  const text = String(value).trim();
  if (!text) return "";
  return text.length > maxLength ? `${text.slice(0, maxLength - 3)}...` : text;
}

export function escrowPolicyHint(confirmation) {
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

export async function switchWalletChain(ethereum, chainId) {
  const numeric = Number(chainId);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    throw new Error("EVM chain id is not available for this order");
  }
  await ethereum.request({
    method: "wallet_switchEthereumChain",
    params: [{ chainId: `0x${numeric.toString(16)}` }]
  });
}

export function buildDepositCalls(order, account) {
  const confirmation = requireConfirmation(order);
  const buyer = confirmation.buyer_evm_address || account;
  return {
    approve: {
      address: confirmation.token,
      abi: erc20Abi,
      functionName: "approve",
      args: [confirmation.escrow_contract, BigInt(confirmation.amount_units)]
    },
    deposit: {
      address: confirmation.escrow_contract,
      abi: escrowAbi,
      functionName: "deposit",
      args: [
        confirmation.order_hash,
        confirmation.token,
        BigInt(confirmation.amount_units),
        confirmation.seller_evm_address,
        buyer,
        confirmation.arbiter_evm_address
      ]
    }
  };
}

export async function requestEvmEscrowDeposit(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
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

export function buildReleaseCall(order) {
  const confirmation = requireConfirmation(order);
  return {
    address: confirmation.escrow_contract,
    abi: escrowAbi,
    functionName: "release",
    args: [confirmation.order_hash]
  };
}

export async function requestEvmEscrowRelease(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const release = buildReleaseCall(order);
  const releaseTxHash = await wallet.writeContract({ ...release, account });
  return {
    account,
    release_tx_hash: releaseTxHash,
    status: "submitted_waiting_for_watcher"
  };
}

export function buildRefundCall(order) {
  const confirmation = requireConfirmation(order);
  return {
    address: confirmation.escrow_contract,
    abi: escrowAbi,
    functionName: "refund",
    args: [confirmation.order_hash]
  };
}

export async function requestEvmEscrowRefund(order, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const refund = buildRefundCall(order);
  const refundTxHash = await wallet.writeContract({ ...refund, account });
  return {
    account,
    refund_tx_hash: refundTxHash,
    status: "submitted_waiting_for_watcher"
  };
}

export function buildPartialRefundCall(order, buyerAmount) {
  const confirmation = requireConfirmation(order);
  return {
    address: confirmation.escrow_contract,
    abi: escrowAbi,
    functionName: "partial_refund",
    args: [confirmation.order_hash, BigInt(buyerAmount)]
  };
}

export async function requestEvmEscrowPartialRefund(order, buyerAmount, ethereum = window.ethereum) {
  const confirmation = requireConfirmation(order);
  const wallet = createWalletClient({ transport: custom(requireEthereum(ethereum)) });
  const [account] = await wallet.requestAddresses();
  await switchWalletChain(ethereum, confirmation.chain_id);
  const partialRefund = buildPartialRefundCall(order, buyerAmount);
  const partialRefundTxHash = await wallet.writeContract({ ...partialRefund, account });
  return {
    account,
    partial_refund_tx_hash: partialRefundTxHash,
    status: "submitted_waiting_for_watcher"
  };
}
