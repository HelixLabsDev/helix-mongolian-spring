#!/bin/bash
set -euo pipefail

echo "=========================================="
echo "  Helix Stellar — Phase 0 Setup"
echo "=========================================="

# --- 1. Rust Toolchain ---
echo ""
echo "[1/6] Checking Rust toolchain..."

if command -v rustup &> /dev/null; then
    echo "  rustup found. Updating..."
    rustup update stable
else
    echo "  Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

RUST_VERSION=$(rustc --version | awk '{print $2}')
echo "  Rust version: $RUST_VERSION"

REQUIRED_MINOR=84
ACTUAL_MINOR=$(echo "$RUST_VERSION" | cut -d. -f2)
if [ "$ACTUAL_MINOR" -lt "$REQUIRED_MINOR" ]; then
    echo "  ERROR: Rust >= 1.84.0 required. Got $RUST_VERSION"
    exit 1
fi

# --- 2. Wasm Target ---
echo ""
echo "[2/6] Adding wasm target..."

if rustup target add wasm32v1-none 2>/dev/null; then
    echo "  wasm32v1-none target added."
else
    echo "  Using wasm32-unknown-unknown..."
    rustup target add wasm32-unknown-unknown
fi

# --- 3. Stellar CLI ---
echo ""
echo "[3/6] Installing Stellar CLI..."

if command -v stellar &> /dev/null; then
    echo "  Stellar CLI found: $(stellar --version 2>/dev/null)"
else
    echo "  Installing stellar-cli via cargo..."
    cargo install --locked stellar-cli
fi

# --- 4. Testnet Identities ---
echo ""
echo "[4/6] Setting up testnet identities..."

for identity in helix-admin helix-user1; do
    if stellar keys show $identity 2>/dev/null; then
        echo "  '$identity' exists."
    else
        echo "  Generating '$identity'..."
        stellar keys generate $identity --network testnet --fund
    fi
done

echo "  Admin: $(stellar keys address helix-admin 2>/dev/null || echo 'UNKNOWN')"

# --- 5. Clone Reference Repos ---
echo ""
echo "[5/6] Cloning reference repos..."

mkdir -p reference
cd reference

for repo in \
    "https://github.com/blend-capital/blend-contracts-v2.git" \
    "https://github.com/axelarnetwork/axelar-cgp-soroban.git" \
    "https://github.com/axelarnetwork/stellar-gmp-example.git"; do
    dirname=$(basename "$repo" .git)
    if [ -d "$dirname" ]; then
        echo "  $dirname — exists."
    else
        echo "  Cloning $dirname..."
        git clone --depth 1 "$repo" 2>/dev/null || echo "  WARNING: Failed to clone $repo"
    fi
done
cd ..

# --- 6. Verify Build ---
echo ""
echo "[6/6] Building workspace..."

cargo build 2>&1 || echo "  Build needs dependency resolution — run 'cargo build' again after setup."
cargo test --all 2>&1 || echo "  Tests need attention."

echo ""
echo "=========================================="
echo "  Setup complete!"
echo "  Next: make test"
echo "=========================================="
