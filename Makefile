.PHONY: check rust-check ts-check

check: rust-check ts-check

rust-check:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

ts-check:
	npm run check
