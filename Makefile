.PHONY: build build-wasm test clean fmt clippy check smoke-real-blend-oracle preflight-t3 probe-blend-testnet-oracle optimize sizes

CONTRACT_PACKAGES = bridge-poc bridge-handler helix-mock-bridge helix-mock-oracle helix-oracle-adaptor helix-blend-oracle-adaptor helix-token helix-vault
WASM_DIR = target/wasm32v1-none/release

build:
	@for pkg in $(CONTRACT_PACKAGES); do \
		echo "Building $$pkg..."; \
		stellar contract build --package "$$pkg" --optimize; \
	done

test:
	cargo test --all

build-wasm:
	cargo build --target wasm32v1-none --release

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets -- -D warnings

check: fmt clippy test

smoke-real-blend-oracle:
	scripts/smoke-real-blend-oracle.sh

preflight-t3: build-wasm check smoke-real-blend-oracle

probe-blend-testnet-oracle:
	scripts/probe-blend-testnet-oracle.sh

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
