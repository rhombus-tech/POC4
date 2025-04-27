# NASDAQ ITCH Sample Data Integration Guide

This guide explains how to download NASDAQ ITCH sample data and process it using your WebAssembly-compatible ITCH parser for TEE integration.

## Overview

The implementation supports both WebAssembly parameter passing patterns:
1. **Length-prefixed format** (first 4 bytes as little-endian u32 length + data)
2. **Direct data format** (raw data without length prefix)

## Step 1: Download Sample Data

Run the download script to fetch sample ITCH data from NASDAQ:

```bash
cd execution/client/tools
./download_itch_samples.sh
```

This will download a sample file and extract it to `execution/client/data/itch/samples/`.

## Step 2: Process the Sample Data

Build and run the example processor that demonstrates integration with both parameter formats:

```bash
cd execution/client
cargo run --example process_itch_sample --features market-data
```

This example will:
1. Load and parse the NASDAQ ITCH binary data
2. Reconstruct order books for all stocks in the sample
3. Demonstrate parameter conversion for both WebAssembly formats
4. Show statistics for the most active stocks

## Step 3: TEE Integration

The example shows how to prepare order book data for secure execution in the TEE environment:

1. Messages are parsed using the high-performance binary ITCH parser
2. Parameters are formatted in both supported formats (length-prefixed and direct)
3. Data can be passed to WebAssembly contracts in the TEE

## Advanced Usage

For a complete demonstration of TEE integration, you can:

1. Develop a WebAssembly contract that consumes order book data
2. Deploy the contract to the TEE using your existing deployment functionality
3. Call the contract with parameters in either format
4. Process results securely within the TEE mesh architecture

## Sample WebAssembly Contract Logic

Here's a conceptual example of how a WebAssembly contract would handle the parameters:

```rust
// Inside a WebAssembly contract
fn handle_parameters(params_ptr: i32, params_len: i32) -> i32 {
    // Check format type (length-prefixed vs direct)
    if params_len >= 4 {
        let len_bytes = [
            read_memory(params_ptr),
            read_memory(params_ptr + 1),
            read_memory(params_ptr + 2),
            read_memory(params_ptr + 3)
        ];
        
        let declared_len = u32::from_le_bytes(len_bytes);
        
        // If reasonable length (0 < len <= 1024) - it's length-prefixed format
        if declared_len > 0 && declared_len <= 1024 && 
           (declared_len as i32) == params_len - 4 {
            // Process as length-prefixed format
            process_length_prefixed(params_ptr, params_len);
        } else {
            // Process as direct format
            process_direct(params_ptr, params_len);
        }
    } else {
        // Must be direct format (too short for length prefix)
        process_direct(params_ptr, params_len);
    }
    
    // Return success
    0
}
```

## Metrics and Performance

The example processor will report:
- Processing speed
- Number of messages handled
- Number of unique stocks
- Order book statistics

This helps you evaluate the performance of your TEE integration with real-world data volumes.
