.PHONY: check rust-check coverage-protocol

check: rust-check coverage-protocol

rust-check:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

coverage-protocol:
	PATH=/Users/ilyagmirin/.cargo/bin:$$PATH LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata cargo llvm-cov --workspace --exclude morpheus-cli --exclude morpheus-server --exclude morpheus-store --fail-under-lines 98
