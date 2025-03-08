#!/bin/bash
set -e

# Ensure wasm32 target is installed
rustup target add wasm32-unknown-unknown

# Build the contract for WebAssembly
cargo build --target wasm32-unknown-unknown --release

echo "Contract built at target/wasm32-unknown-unknown/release/wasmlanche_contract_test.wasm"

# Copy to a convenient location
cp target/wasm32-unknown-unknown/release/wasmlanche_contract_test.wasm ./wasmlanche_contract.wasm
echo "Contract copied to ./wasmlanche_contract.wasm"
