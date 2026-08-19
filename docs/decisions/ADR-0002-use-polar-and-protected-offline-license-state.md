# ADR-0002: Use Polar and protected offline license state

Status: Accepted
Date: 2026-08-19
Related phases: HUM-10, HUM-70
Supersedes:
Superseded by:

## Context

Hum will sell for $19 as a one-time Windows desktop license. The purchase flow needs global tax handling, receipts, refunds, license delivery, device limits, and a customer recovery path. Hum does not need user accounts or a subscription service.

A direct Stripe Checkout integration would leave SYVR responsible for tax registrations and filings unless Stripe Managed Payments is used. Managed Payments is still a public preview, and Stripe does not provide Hum's desktop license lifecycle. Lemon Squeezy covers tax and license keys, but charges 5 percent plus 50 cents for the same basic transaction that Polar currently handles for 4 percent plus 40 cents. Both are credible, but SYVR already selected Polar for Loomwerks and has a documented exit path.

Polar provides one-time products, hosted checkout, Merchant of Record tax handling, license keys, activation limits, public client activation and validation endpoints, refunds, benefit revocation, and a customer portal where buyers can retrieve keys and release device activations.

## Decision

Hum will use Polar for checkout, license issuance, activation limits, benefit status, refunds, and the hosted customer portal.

The app will call Polar's public customer license endpoints directly. Hum will not ship a Polar organization access token or any other provider secret. The Polar organization ID is public configuration.

One license permits three active Windows devices. Activation uses Polar's activation ID, not a hardware fingerprint. The app labels an activation with a generic device name and a short random install identifier. It does not send a serial number, Windows MachineGuid, MAC address, or raw hostname.

Hum stores the license key, Polar activation ID, install identifier, last successful verification time, and last observed wall-clock time in a Windows DPAPI-protected record under the app data directory. Shared Rust owns the record format and entitlement policy. A Windows storage adapter owns protection and recovery. Future macOS and Linux builds will provide their own secure storage adapters.

The entitlement itself is perpetual for Hum 1.x. Hum tries to revalidate every 30 days. If Polar or the network is unavailable, the last valid activation receives 30 more days of full use. The UI starts warning before the grace period ends. Invalid, revoked, refunded, device-limit, clock-rollback, and service-unavailable results remain separate states with separate recovery text.

Development builds use a clearly identified local development entitlement. Release builds do not contain a bypass key or a hidden unlock route.

## Consequences

- Polar handles indirect tax, receipts, refunds, and license delivery for the one-time purchase.
- Customers can restore a key and manage device activations without a Hum account.
- Hum has no license server to host for 1.0.
- The app must keep Polar response parsing narrow and covered by fixtures because provider payloads can change.
- A provider outage does not immediately stop a verified customer. The local record must detect rollback and explain stale verification before access changes.
- Moving away from Polar requires a license migration or a compatibility adapter for existing keys.
- Refund and revocation enforcement occurs at the next successful validation, or when the offline grace period ends.

## Alternatives considered

### Stripe Checkout with a custom license service

Stripe is already used elsewhere in SYVR, but ordinary Checkout does not make Stripe the Merchant of Record. SYVR would own tax registration and filing, plus every part of key issuance and device management. Stripe Managed Payments reduces that burden but remains a preview and still needs a separate license system.

### Lemon Squeezy

Lemon Squeezy is a Merchant of Record with a mature license API. Its current base fee is higher than Polar's at Hum's $19 price, and adopting it would create a second licensing pattern beside Loomwerks without a product benefit.

### A self-hosted license server

A small service could issue signed entitlements, but it adds an operational dependency, webhook handling, a customer recovery surface, and tax work. Hum can use Polar's public client endpoints and protected local state without that service.

### No device activation

A plain purchase receipt would be simpler, but it would make casual key sharing unlimited and give support no useful recovery state. Three self-managed activations keep enforcement light without making normal desktop and laptop use annoying.

## Verification

This decision remains sound when:

- A Polar test purchase creates a perpetual key with three activations.
- The Windows app activates, validates, deactivates, and restores that key without a provider secret.
- The protected record cannot be read as plain text and fails closed when modified.
- Valid customers keep full use through the documented offline window.
- Refunded or revoked test licenses leave the active state after validation.
- The website, checkout, receipt, app, privacy page, terms, and support copy state the same price, device count, refund window, and update policy.

Review this ADR if Polar removes public desktop activation endpoints, stops acting as Merchant of Record for software, materially changes its pricing, or cannot provide reliable benefit revocation and customer recovery.
