.PHONY: check rust-check coverage-protocol e2e-three-synapse e2e-three-synapse-down

check: rust-check coverage-protocol

rust-check:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

coverage-protocol:
	PATH=/Users/ilyagmirin/.cargo/bin:$$PATH LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata cargo llvm-cov --workspace --exclude morpheus-cli --exclude morpheus-server --exclude morpheus-store --fail-under-lines 98

e2e-three-synapse:
	scripts/e2e/run-three-synapse.sh

e2e-three-synapse-down:
	docker compose -f docker-compose.e2e.yml down -v
	find .local/e2e -path '.local/e2e/synapse-*' -type f \( -name 'homeserver.db' -o -name 'homeserver.db-*' \) -delete
