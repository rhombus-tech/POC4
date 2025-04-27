#!/bin/bash

set -e

ROOT_DIR="$(pwd)"
OUTPUT_DIR="../../../target/wasm32-unknown-unknown/release"
mkdir -p "$OUTPUT_DIR"

# Build each contract
for contract_dir in simple_add simple_multiply token_transfer key_value_store data_oracle; do
    echo "Building $contract_dir..."
    cd "$ROOT_DIR/$contract_dir"
    
    # Get the actual crate name from Cargo.toml
    CRATE_NAME=$(grep -m 1 'name\s*=' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
    echo "Crate name: $CRATE_NAME"
    
    # Build the contract
    cargo build --target wasm32-unknown-unknown --release
    
    # Copy the WASM file if it exists
    WASM_PATH="target/wasm32-unknown-unknown/release/${CRATE_NAME}.wasm"
    if [ -f "$WASM_PATH" ]; then
        echo "Copying $WASM_PATH to $OUTPUT_DIR"
        cp -f "$WASM_PATH" "$OUTPUT_DIR/"
    else
        echo "Warning: $WASM_PATH not found"
    fi
    
    echo "Done."
done

echo "All contracts built successfully!"
