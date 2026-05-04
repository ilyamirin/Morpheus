import { MarketplaceValidationError } from "./errors.js";

export interface ApplicationServiceContext {
  instanceId: string;
  serverName: string;
  exclusiveUserLocalpart: string;
}

export interface AppserviceTransaction {
  txnId: string;
  eventIds: string[];
}

export function validateApplicationServiceSender(sender: string, context: ApplicationServiceContext): void {
  const expected = `@${context.exclusiveUserLocalpart}:${context.serverName}`;
  const namespacePrefix = `@${context.exclusiveUserLocalpart}_`;
  const namespaceSuffix = `:${context.serverName}`;
  if (sender !== expected && !(sender.startsWith(namespacePrefix) && sender.endsWith(namespaceSuffix))) {
    throw new MarketplaceValidationError("UNAUTHORIZED_SENDER", "Sender is outside marketplace Application Service namespace", {
      sender,
      expected,
      namespacePrefix,
      namespaceSuffix,
      instanceId: context.instanceId
    });
  }
}

export function validateAppserviceTransaction(
  transaction: AppserviceTransaction,
  seen: Map<string, string[]>
): void {
  const previous = seen.get(transaction.txnId);
  if (previous) {
    const same = previous.length === transaction.eventIds.length && previous.every((eventId, index) => eventId === transaction.eventIds[index]);
    if (!same) {
      throw new MarketplaceValidationError("DUPLICATE_EVENT", "Appservice transactions must be idempotent", {
        txnId: transaction.txnId,
        previous,
        actual: transaction.eventIds
      });
    }
    return;
  }
  seen.set(transaction.txnId, [...transaction.eventIds]);
}
