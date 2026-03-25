.PHONY: build test clean fmt clippy optimize sizes

CONTRACT_PACKAGES = bridge-poc helix-mock-bridge helix-oracle-adaptor helix-token helix-vault
WASM_DIR = target/wasm32v1-none/release

build:
	@for pkg in $(CONTRACT_PACKAGES); do \
		echo "Building $$pkg..."; \
		stellar contract build --package "$$pkg" --optimize; \
	done

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

sizes: build
	@echo "=== WASM Sizes ==="
	@for wasm in $(WASM_DIR)/*.wasm; do \
		if [ -f "$$wasm" ]; then \
			echo "  $$(basename $$wasm): $$(wc -c < $$wasm) bytes"; \
		fi \
	done

NETWORK = testnet
ADMIN = helix-admin

deploy-token: optimize
	stellar contract deploy \
		--wasm $(WASM_DIR)/helix_token.wasm \
		--source $(ADMIN) --network $(NETWORK)

deploy-vault: optimize
	stellar contract deploy \
		--wasm $(WASM_DIR)/helix_vault.wasm \
		--source $(ADMIN) --network $(NETWORK)

deploy-oracle: optimize
	stellar contract deploy \
		--wasm $(WASM_DIR)/helix_oracle_adaptor.wasm \
		--source $(ADMIN) --network $(NETWORK)
