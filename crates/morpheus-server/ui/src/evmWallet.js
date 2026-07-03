import { createWalletClient, custom } from "viem";

export function evmWalletBuildMarker() {
  return {
    adapter: "evm_escrow",
    library: "viem",
    writeMethod: "writeContract",
    createWalletClient,
    custom
  };
}

export async function requestEvmEscrowDeposit(_order) {
  evmWalletBuildMarker();
  throw new Error("EVM wallet deposit requires Task 7");
}

export async function requestEvmEscrowRelease(_order) {
  evmWalletBuildMarker();
  throw new Error("EVM wallet release requires Task 8");
}

export async function requestEvmEscrowRefund(_order) {
  evmWalletBuildMarker();
  throw new Error("EVM wallet refund requires Task 9");
}

export async function requestEvmEscrowPartialRefund(_order, _buyerAmount) {
  evmWalletBuildMarker();
  throw new Error("EVM wallet partial refund requires Task 9");
}
