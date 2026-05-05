# Gallery First Seller and Buyer UI Design

Date: 2026-05-05

## Summary

The approved UI direction for Morpheus seller and buyer surfaces is **Gallery First + Storefront Management**.

The goal is to move away from technical admin-style forms and toward simple, readable marketplace interfaces. The buyer should feel like they are shopping in a clean product gallery. The seller should feel like they are managing a small storefront, not operating a backend console.

Protocol details remain available, but they must stay out of the primary user flow.

## Approved Direction

### Buyer

Approved direction: **Gallery First**.

The buyer first sees a product gallery with generated product images. The first screen should emphasize:

- product image;
- product name;
- price;
- seller;
- trusted instance;
- one clear `Buy` action.

The buyer should not need to understand Matrix rooms, event ids, seller ids, offer ids, or order ids to browse or buy.

### Checkout

Approved direction: **Shop-simple bottom sheet**.

Clicking `Buy` opens a bottom sheet or modal. The checkout sheet contains only:

- selected product image;
- product name;
- seller and instance;
- total price;
- primary `Create order` action.

Trust and protocol metadata can be shown in product details or `Advanced`, but the checkout itself stays minimal.

### Seller

Approved direction: **Storefront + Quick Add**.

The seller first sees `My Store`, a gallery of their own product cards. Product cards show:

- image;
- title;
- price;
- draft or published status;
- order count or attention state;
- compact edit/publish actions.

Quick Add sits near the storefront instead of sending the seller to a separate technical form page. It should support a short flow:

1. Product image and title.
2. Price and category.
3. Publish.

Advanced protocol fields stay collapsed.

## Visual Assets

Product UI should use generated product imagery, not placeholder boxes or abstract gradients.

The concept round used generated images for:

- technical books;
- smartphone cases;
- sneakers;
- clothing.

Implementation should include a small checked-in demo asset set for local UI and tests. These assets should be product images, not decorative backgrounds.

## Information Architecture

### Buyer Page

The buyer page should be a single coherent marketplace surface:

- top navigation stays minimal;
- gallery is the default view;
- product detail and checkout are overlays or inline panels;
- order history is secondary;
- `Advanced` contains protocol ids and raw API output.

Recommended sections:

- `Discover`
- `Orders`
- `Advanced`

The selected offer view should not feel like a separate technical page. It should be integrated with the product card and checkout sheet.

### Seller Page

The seller page should be a single storefront management surface:

- `My Store` first;
- `Quick Add` visible and lightweight;
- order attention visible but not dominant;
- `Advanced` contains ids, hashes, raw responses, and protocol fields.

Recommended sections:

- `Store`
- `Orders`
- `Advanced`

The previous profile/product/offer step cards should be compressed into one friendly listing creation flow.

## Component Model

### Shared Components

- Product card
- Seller product card
- Gallery grid
- Bottom sheet/modal
- Quick Add panel
- Order attention card
- Advanced disclosure
- Empty state

### Buyer Components

- Product gallery
- Product detail preview
- Checkout bottom sheet
- Buyer order card

### Seller Components

- Storefront gallery
- Quick Add form
- Listing status badge
- Seller order action card

## Data Flow

No backend API changes are required for this UI redesign.

Buyer flow:

1. Buyer refreshes catalog.
2. UI renders offers as product cards.
3. Buyer clicks `Buy`.
4. UI opens checkout bottom sheet.
5. Buyer clicks `Create order`.
6. Existing buyer order API submits the order.
7. Projection updates still come from the Morpheus/Synapse ingest path.

Seller flow:

1. Seller views storefront cards.
2. Seller uses Quick Add to submit seller/product/offer data through existing endpoints.
3. UI shows submitted result in `Advanced`.
4. Seller refreshes or receives projected state from existing APIs.
5. Seller handles order actions through existing seller order endpoints.

## Constraints

- Keep static HTML/CSS/JS.
- Do not add React, Tailwind runtime, npm dependency, or a new frontend build pipeline.
- Preserve current bearer token hooks.
- Preserve current seller and buyer HTTP endpoints.
- Keep admin UI out of this redesign except for shared style compatibility.
- Hide protocol ids from the main buyer/seller paths.
- Keep product images visible on the first viewport for buyer and seller.

## Error Handling

Primary user flows should show short, human-readable messages:

- order submitted;
- publish submitted;
- missing token;
- request failed;
- projection not updated yet.

Raw JSON responses remain visible in `Advanced`.

For network or API failures, show a concise toast and keep the user's current form state.

## Testing

Route-level tests should assert:

- `/ui/buyer` and `/ui/seller` still serve successfully;
- required hooks remain present;
- product gallery, bottom sheet, storefront, quick add, and advanced anchors exist;
- old technical-first labels are not the primary anchors.

Browser QA should verify:

- desktop and mobile layouts have no horizontal overflow;
- bottom sheet fits mobile viewport;
- product cards keep stable aspect ratios;
- text does not overlap images or buttons;
- seller quick add does not dominate the storefront;
- protocol details are hidden unless `Advanced` is opened.

## Open Implementation Notes

- Product image assets should be committed under the UI asset tree during implementation.
- The UI can start with demo images and later bind real product image URLs when the protocol supports product media metadata.
- If product media is not yet part of the protocol payload, the UI should derive demo images by category for local/dev use only.

## Self-Review

- No placeholder requirements remain.
- The design does not require backend API changes.
- The buyer and seller flows are separate but share visual components.
- The scope is limited to seller and buyer UI redesign.
- The design preserves the protocol-first architecture by keeping projections and writes on the existing API path.
