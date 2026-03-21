.PHONY: build test clean fmt clippy optimize

build:
	cargo build --target wasm32-unknown-unknown --release

test:
	cargo test --all

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets -- -D warnings

check: fmt clippy test

clean:
	cargo clean

optimize: build
	@for wasm in target/wasm32-unknown-unknown/release/helix_*.wasm; do \
		if [ -f "$$wasm" ]; then \
			echo "Optimizing $$wasm..."; \
			stellar contract optimize --wasm "$$wasm"; \
		fi \
	done

sizes: build
	@echo "=== WASM Sizes ==="
	@for wasm in target/wasm32-unknown-unknown/release/helix_*.wasm; do \
		if [ -f "$$wasm" ]; then \
			echo "  $$(basename $$wasm): $$(wc -c < $$wasm) bytes"; \
		fi \
	done

NETWORK = testnet
ADMIN = helix-admin

deploy-token: optimize
	stellar contract deploy \
		--wasm target/wasm32-unknown-unknown/release/helix_token.optimized.wasm \
		--source $(ADMIN) --network $(NETWORK)

deploy-vault: optimize
	stellar contract deploy \
		--wasm target/wasm32-unknown-unknown/release/helix_vault.optimized.wasm \
		--source $(ADMIN) --network $(NETWORK)

deploy-oracle: optimize
	stellar contract deploy \
		--wasm target/wasm32-unknown-unknown/release/helix_oracle_adaptor.optimized.wasm \
		--source $(ADMIN) --network $(NETWORK)
