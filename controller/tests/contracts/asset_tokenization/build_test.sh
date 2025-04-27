#!/bin/bash
set -e

# Clean any previous build artifacts
cargo clean

# Build for WebAssembly target
echo "Building for WebAssembly target..."
cargo build --target wasm32-unknown-unknown --release

echo "Build successful!"
