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

export async function requestEvmEscrowRefund(_order) {
  throw new Error("EVM wallet refund requires Task 9");
}

export async function requestEvmEscrowPartialRefund(_order, _buyerAmount) {
  throw new Error("EVM wallet partial refund requires Task 9");
}
