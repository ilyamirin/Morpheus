import { MarketplaceValidationError } from "./errors.js";

const reverseDnsPattern = /^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*){1,}\.[a-z][a-z0-9._-]*$/;

export function validateExtensionNamespace(extension: string): void {
  if (extension.startsWith("io.marketplace.")) {
    throw new MarketplaceValidationError("UNKNOWN_CRITICAL_EXTENSION", "Extensions must not use the io.marketplace namespace", {
      extension
    });
  }
  if (!reverseDnsPattern.test(extension)) {
    throw new MarketplaceValidationError("UNKNOWN_CRITICAL_EXTENSION", "Extensions must use a reverse-DNS namespace", {
      extension
    });
  }
}
