.PHONY: check rust-check coverage-protocol coverage-payment ui-wallet-flow e2e-three-synapse e2e-three-synapse-down e2e-evm-escrow testnet-evm-escrow audit-evm-escrow

PAYMENT_COVERAGE_MIN ?= 65

check: rust-check coverage-protocol

rust-check:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

coverage-protocol:
	PATH=/Users/ilyagmirin/.cargo/bin:$$PATH LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata cargo llvm-cov --workspace --exclude morpheus-cli --exclude morpheus-server --exclude morpheus-store --fail-under-lines 98

coverage-payment:
	PATH=/Users/ilyagmirin/.cargo/bin:$$PATH LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata cargo llvm-cov --package morpheus-server --package morpheus-store --all-targets --fail-under-lines $(PAYMENT_COVERAGE_MIN)

ui-wallet-flow:
	npm run test:ui-wallet-flow

e2e-three-synapse:
	scripts/e2e/run-three-synapse.sh

e2e-three-synapse-down:
	docker compose -f docker-compose.e2e.yml down -v
	find .local/e2e -path '.local/e2e/synapse-*' -type f \( -name 'homeserver.db' -o -name 'homeserver.db-*' \) -delete

e2e-evm-escrow:
	./scripts/e2e/run-evm-escrow.sh

testnet-evm-escrow:
	./scripts/e2e/run-evm-escrow-testnet-drill.sh

audit-evm-escrow:
	./scripts/e2e/check-evm-escrow-audit-artifacts.sh
