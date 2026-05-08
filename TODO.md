# TODO

## Product Media

- Product image upload is implemented for the local dev UI by compressing the selected file in-browser and publishing it as product `media[]` metadata. Remaining production work:
  - add object storage for larger product media instead of inline data URLs;
  - add replace/change image controls for already-published products;
  - define production URL/content safety rules for `media[]` before using untrusted remote media outside dev/E2E;
  - keep category-derived demo images as a fallback when a product has no custom image.
