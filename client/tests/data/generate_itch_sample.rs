use std::fs::File;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

// Generate a simple ITCH binary file for testing
fn main() -> io::Result<()> {
    let mut file = File::create("tests/data/sample.itch")?;
    
    // Current timestamp in nanoseconds
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    // System Event Message (Type 'S')
    write_message(&mut file, b'S', &[
        // Timestamp (8 bytes)
        &timestamp.to_be_bytes(),
        // Event code (1 byte) - 'O' for start of trading
        b"O",
    ])?;
    
    // Add Order Message (Type 'A')
    write_message(&mut file, b'A', &[
        // Timestamp (8 bytes)
        &(timestamp + 1000).to_be_bytes(),
        // Order Reference Number (8 bytes)
        &1001u64.to_be_bytes(),
        // Buy/Sell Indicator (1 byte) - 'B' for buy
        b"B",
        // Shares (4 bytes)
        &100u32.to_be_bytes(),
        // Stock (8 bytes, space-padded)
        b"AAPL    ",
        // Price (4 bytes) - $150.00 * 10000 = 1500000
        &1500000u32.to_be_bytes(),
    ])?;
    
    // Add Order Message (Type 'A')
    write_message(&mut file, b'A', &[
        // Timestamp (8 bytes)
        &(timestamp + 2000).to_be_bytes(),
        // Order Reference Number (8 bytes)
        &1002u64.to_be_bytes(),
        // Buy/Sell Indicator (1 byte) - 'S' for sell
        b"S",
        // Shares (4 bytes)
        &200u32.to_be_bytes(),
        // Stock (8 bytes, space-padded)
        b"AAPL    ",
        // Price (4 bytes) - $151.00 * 10000 = 1510000
        &1510000u32.to_be_bytes(),
    ])?;
    
    // Add Order Message (Type 'A') for different stock
    write_message(&mut file, b'A', &[
        // Timestamp (8 bytes)
        &(timestamp + 3000).to_be_bytes(),
        // Order Reference Number (8 bytes)
        &1003u64.to_be_bytes(),
        // Buy/Sell Indicator (1 byte) - 'B' for buy
        b"B",
        // Shares (4 bytes)
        &500u32.to_be_bytes(),
        // Stock (8 bytes, space-padded)
        b"MSFT    ",
        // Price (4 bytes) - $300.00 * 10000 = 3000000
        &3000000u32.to_be_bytes(),
    ])?;
    
    // Order Executed Message (Type 'E')
    write_message(&mut file, b'E', &[
        // Timestamp (8 bytes)
        &(timestamp + 5000).to_be_bytes(),
        // Order Reference Number (8 bytes)
        &1001u64.to_be_bytes(),
        // Executed Shares (4 bytes)
        &50u32.to_be_bytes(),
        // Match Number (8 bytes)
        &5001u64.to_be_bytes(),
    ])?;
    
    println!("Generated sample.itch test file with 4 ITCH messages");
    Ok(())
}

fn write_message(file: &mut File, msg_type: u8, fields: &[&[u8]]) -> io::Result<()> {
    // Calculate total length
    let mut length = 1; // Message type
    for field in fields {
        length += field.len();
    }
    
    // Write message size (2 bytes, big endian)
    file.write_all(&(length as u16).to_be_bytes())?;
    
    // Write message type
    file.write_all(&[msg_type])?;
    
    // Write fields
    for field in fields {
        file.write_all(field)?;
    }
    
    Ok(())
}
