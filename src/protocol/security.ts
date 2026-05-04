import { MarketplaceValidationError } from "./errors.js";

export interface SecurityEnvelope {
  protocol_version?: string;
  min_consumer_version?: string;
  sender?: string;
  issuer?: {
    instance_id?: string;
    matrix_user_id?: string;
  };
}

export interface SecurityContext {
  supportedVersion: string;
}

export function validateSecurityEnvelope(envelope: SecurityEnvelope, context: SecurityContext): void {
  if (envelope.min_consumer_version && compareVersion(context.supportedVersion, envelope.min_consumer_version) < 0) {
    throw new MarketplaceValidationError("UNSUPPORTED_PROTOCOL_VERSION", "Protocol downgrade or unsupported consumer version", {
      supportedVersion: context.supportedVersion,
      minConsumerVersion: envelope.min_consumer_version
    });
  }

  if (envelope.sender && envelope.issuer?.instance_id) {
    const senderServer = envelope.sender.split(":")[1];
    if (senderServer !== envelope.issuer.instance_id) {
      throw new MarketplaceValidationError("UNAUTHORIZED_SENDER", "Sender server does not match issuer instance", {
        sender: envelope.sender,
        issuerInstance: envelope.issuer.instance_id
      });
    }
  }
}

function compareVersion(left: string, right: string): number {
  const [leftMajor = 0, leftMinor = 0] = left.split(".").map(Number);
  const [rightMajor = 0, rightMinor = 0] = right.split(".").map(Number);
  return leftMajor === rightMajor ? leftMinor - rightMinor : leftMajor - rightMajor;
}
