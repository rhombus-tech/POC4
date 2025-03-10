#!/bin/bash

# Change to the execution directory
cd "$(dirname "$0")/../.."

# Set the log level for better debugging
export RUST_LOG=debug

# Run mesh network tests using the correct package name
echo "Running mesh network tests..."
cargo test --package tee-controller --test mesh_network_test -- --nocapture

echo "Tests completed"
