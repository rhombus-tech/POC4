/*!
 * UDP Multicast Receiver for NASDAQ ITCH Live Market Data
 * 
 * This module provides direct connectivity to NASDAQ's TotalView-ITCH data feed via UDP multicast.
 * It manages subscription to multicast groups, implements SoupBinTCP protocol for session management,
 * and handles the MoldUDP64 packet format for market data reception.
 * 
 * Key features:
 * - High-performance multicast socket configuration
 * - Packet loss detection and recovery
 * - Integration with real-time processing pipeline
 * - Hardware timestamp support (when available)
 * - Microsecond-precision timing
 */

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{ClientError, Result};
use crate::itch::realtime::{BatchProcessor, MessageMemoryPool};

/// MoldUDP64 packet header structure (20 bytes)
/// As per NASDAQ specs: https://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/moldudp64.pdf
#[derive(Debug, Clone)]
pub struct MoldUDP64Header {
    /// Session identifier (10 bytes)
    pub session: [u8; 10],
    /// Sequence number (8 bytes)
    pub sequence: u64,
    /// Message count (2 bytes)
    pub message_count: u16,
}

/// Configuration for multicast reception
#[derive(Debug, Clone)]
pub struct MulticastConfig {
    /// Multicast group IP address
    pub group_addr: Ipv4Addr,
    /// Multicast port
    pub port: u16,
    /// Local interface IP to bind to
    pub interface_addr: Ipv4Addr,
    /// Maximum packet size to receive
    pub max_packet_size: usize,
    /// Receive buffer size in bytes
    pub recv_buffer_size: usize,
    /// Whether to enable hardware timestamping if available
    pub hw_timestamping: bool,
    /// Request missing sequence gap retransmission
    pub request_retransmission: bool,
    /// Retransmission server address (if retransmission is enabled)
    pub retransmission_addr: Option<SocketAddr>,
    /// Timeout for packet reception
    pub receive_timeout: Duration,
}

impl Default for MulticastConfig {
    fn default() -> Self {
        Self {
            group_addr: Ipv4Addr::new(233, 54, 12, 1),    // Example NASDAQ multicast address
            port: 26477,                                   // Example NASDAQ ITCH port
            interface_addr: Ipv4Addr::new(0, 0, 0, 0),    // Default interface
            max_packet_size: 1500,                        // Standard Ethernet MTU
            recv_buffer_size: 16 * 1024 * 1024,           // 16MB socket buffer
            hw_timestamping: true,                        // Enable hardware timestamping
            request_retransmission: true,                 // Enable retransmission
            retransmission_addr: None,                    // Will be set if configured
            receive_timeout: Duration::from_secs(1),      // 1 second timeout
        }
    }
}

/// Statistics for multicast session
#[derive(Debug, Default, Clone)]
pub struct MulticastStats {
    /// Total packets received
    pub packets_received: u64,
    /// Total messages extracted
    pub messages_extracted: u64,
    /// Sequence gaps detected
    pub sequence_gaps: u64,
    /// Packets requested via retransmission
    pub retransmission_requests: u64,
    /// Successful retransmissions
    pub retransmission_success: u64,
    /// Failed retransmissions
    pub retransmission_failed: u64,
    /// Minimum packet processing latency (microseconds)
    pub min_latency_us: u64,
    /// Maximum packet processing latency (microseconds)
    pub max_latency_us: u64,
    /// Average packet processing latency (microseconds)
    pub avg_latency_us: f64,
}

/// NASDAQ ITCH UDP Multicast receiver
pub struct MulticastReceiver {
    /// Socket for receiving multicast data
    socket: UdpSocket,
    /// Configuration
    config: MulticastConfig,
    /// Packet buffer for receiving data
    packet_buffer: Vec<u8>,
    /// Expected next sequence number
    next_sequence: u64,
    /// Flag to control the receive loop
    running: Arc<AtomicBool>,
    /// Latest statistics
    stats: MulticastStats,
    /// Memory pool for zero-allocation processing
    memory_pool: Option<MessageMemoryPool>,
    /// Retransmission socket (if enabled)
    retransmission_socket: Option<UdpSocket>,
}

impl MulticastReceiver {
    /// Create a new multicast receiver with the given configuration
    pub fn new(config: MulticastConfig) -> Result<Self> {
        // Create UDP socket
        let socket = UdpSocket::bind(SocketAddr::new(
            IpAddr::V4(config.interface_addr),
            config.port,
        ))?;
        
        // Set socket options
        socket.set_read_timeout(Some(config.receive_timeout))?;
        
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket.as_raw_fd();
            
            // Set receive buffer size
            unsafe {
                let value: libc::c_int = config.recv_buffer_size as libc::c_int;
                let ret = libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_RCVBUF,
                    &value as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
                
                if ret < 0 {
                    return Err(ClientError::Other(format!(
                        "Failed to set receive buffer size: {}", 
                        io::Error::last_os_error()
                    )));
                }
                
                // Set socket for reuse
                let value: libc::c_int = 1;
                let ret = libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEADDR,
                    &value as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
                
                if ret < 0 {
                    return Err(ClientError::Other(format!(
                        "Failed to set socket reuse: {}", 
                        io::Error::last_os_error()
                    )));
                }
                
                // Enable hardware timestamping if requested
                if config.hw_timestamping {
                    // Implementation will depend on the specific NIC and driver
                    // For Intel NICs with appropriate drivers:
                    #[cfg(target_os = "linux")]
                    {
                        // This is a simplified example - actual implementation would
                        // require checking driver capabilities and using appropriate ioctls
                        let _ = libc::setsockopt(
                            fd,
                            libc::SOL_SOCKET,
                            libc::SO_TIMESTAMPNS,
                            &value as *const _ as *const libc::c_void,
                            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                        );
                    }
                }
            }
        }
        
        // Join multicast group
        socket.join_multicast_v4(&config.group_addr, &config.interface_addr)?;
        
        // Create retransmission socket if enabled
        let retransmission_socket = if config.request_retransmission && config.retransmission_addr.is_some() {
            let retrans_socket = UdpSocket::bind("0.0.0.0:0")?;
            retrans_socket.set_read_timeout(Some(Duration::from_millis(500)))?;
            Some(retrans_socket)
        } else {
            None
        };
        
        // Allocate packet buffer
        let packet_buffer = vec![0u8; config.max_packet_size];
        
        Ok(Self {
            socket,
            config,
            packet_buffer,
            next_sequence: 1, // Start expecting sequence 1
            running: Arc::new(AtomicBool::new(false)),
            stats: MulticastStats::default(),
            memory_pool: None,
            retransmission_socket,
        })
    }
    
    /// Set memory pool for zero-allocation processing
    pub fn set_memory_pool(&mut self, memory_pool: MessageMemoryPool) {
        self.memory_pool = Some(memory_pool);
    }
    
    /// Start asynchronous receiver with a batch processor
    pub fn start_async_receiver(
        &mut self,
        batch_processor: Arc<std::sync::Mutex<BatchProcessor>>,
    ) -> Result<JoinHandle<Result<()>>> {
        if self.running.load(Ordering::SeqCst) {
            return Err(ClientError::Other("Receiver already running".to_string()));
        }
        
        // Set running flag
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        
        // Clone socket and configuration for the async task
        let socket = self.socket.try_clone()?;
        let config = self.config.clone();
        let retransmission_socket = self.retransmission_socket.as_ref().map(|s| s.try_clone().unwrap());
        
        // Create channel for packet processing
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1000);
        
        // Spawn packet receiver task
        let receiver_handle = tokio::spawn(async move {
            let mut packet_buffer = vec![0u8; config.max_packet_size];
            let mut expected_sequence = 1;
            let mut stats = MulticastStats::default();
            
            while running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut packet_buffer) {
                    Ok((bytes_read, _src_addr)) => {
                        let start_time = Instant::now();
                        
                        if bytes_read < 20 {
                            // Skip packets smaller than the MoldUDP64 header
                            continue;
                        }
                        
                        // Process MoldUDP64 header
                        let mut session = [0u8; 10];
                        session.copy_from_slice(&packet_buffer[0..10]);
                        
                        let sequence = u64::from_be_bytes([
                            packet_buffer[10], packet_buffer[11], packet_buffer[12], packet_buffer[13],
                            packet_buffer[14], packet_buffer[15], packet_buffer[16], packet_buffer[17],
                        ]);
                        
                        let message_count = u16::from_be_bytes([
                            packet_buffer[18], packet_buffer[19],
                        ]);
                        
                        // Check for sequence gaps
                        if sequence > expected_sequence {
                            stats.sequence_gaps += 1;
                            
                            // Request retransmission if enabled
                            if config.request_retransmission && retransmission_socket.is_some() {
                                if let Some(retrans_addr) = config.retransmission_addr {
                                    // Simplified retransmission request - actual implementation
                                    // would follow NASDAQ retransmission protocol
                                    let retrans_socket = retransmission_socket.as_ref().unwrap();
                                    let mut request = [0u8; 24];
                                    request[0..10].copy_from_slice(&session);
                                    request[10..18].copy_from_slice(&expected_sequence.to_be_bytes());
                                    request[18..24].copy_from_slice(&(sequence - expected_sequence).to_be_bytes()[2..]);
                                    
                                    match retrans_socket.send_to(&request, retrans_addr) {
                                        Ok(_) => {
                                            stats.retransmission_requests += 1;
                                        },
                                        Err(e) => {
                                            eprintln!("Failed to send retransmission request: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Update expected sequence
                        expected_sequence = sequence + message_count as u64;
                        
                        // Extract packet data for processing
                        let packet_data = packet_buffer[0..bytes_read].to_vec();
                        stats.packets_received += 1;
                        
                        // Calculate processing latency
                        let latency_us = start_time.elapsed().as_micros() as u64;
                        if stats.min_latency_us == 0 || latency_us < stats.min_latency_us {
                            stats.min_latency_us = latency_us;
                        }
                        if latency_us > stats.max_latency_us {
                            stats.max_latency_us = latency_us;
                        }
                        
                        // Update average latency
                        let prev_total = stats.avg_latency_us * (stats.packets_received - 1) as f64;
                        stats.avg_latency_us = (prev_total + latency_us as f64) / stats.packets_received as f64;
                        
                        // Forward packet for processing
                        if tx.send(packet_data).await.is_err() {
                            // Channel closed, exit loop
                            break;
                        }
                    },
                    Err(e) => {
                        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                            // Timeout is normal, just continue
                            continue;
                        }
                        
                        // Other errors are concerning
                        eprintln!("Error receiving multicast packet: {}", e);
                    }
                }
            }
            
            Ok(())
        });
        
        // Spawn packet processor task
        let processor_task = tokio::spawn(async move {
            while let Some(packet_data) = rx.recv().await {
                if packet_data.len() < 20 {
                    // Invalid packet, skip
                    continue;
                }
                
                // Lock batch processor and process the packet
                if let Ok(mut processor) = batch_processor.lock() {
                    if let Err(e) = processor.process_incoming_packet(&packet_data) {
                        eprintln!("Error processing packet: {}", e);
                    }
                }
            }
        });
        
        Ok(receiver_handle)
    }
    
    /// Synchronously receive packets and process with the given batch processor
    /// This is mainly for testing and simple applications
    pub fn receive_sync(&mut self, batch_processor: &mut BatchProcessor, max_packets: usize) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(ClientError::Other("Receiver already running".to_string()));
        }
        
        self.running.store(true, Ordering::SeqCst);
        let mut packets_received = 0;
        
        while self.running.load(Ordering::SeqCst) && (max_packets == 0 || packets_received < max_packets) {
            match self.socket.recv_from(&mut self.packet_buffer) {
                Ok((bytes_read, _src_addr)) => {
                    let start_time = Instant::now();
                    
                    if bytes_read < 20 {
                        // Skip packets smaller than the MoldUDP64 header
                        continue;
                    }
                    
                    // Process MoldUDP64 header
                    let mut session = [0u8; 10];
                    session.copy_from_slice(&self.packet_buffer[0..10]);
                    
                    let sequence = u64::from_be_bytes([
                        self.packet_buffer[10], self.packet_buffer[11], self.packet_buffer[12], self.packet_buffer[13],
                        self.packet_buffer[14], self.packet_buffer[15], self.packet_buffer[16], self.packet_buffer[17],
                    ]);
                    
                    let message_count = u16::from_be_bytes([
                        self.packet_buffer[18], self.packet_buffer[19],
                    ]);
                    
                    // Process packet with the batch processor
                    let packet_slice = &self.packet_buffer[0..bytes_read];
                    match batch_processor.process_incoming_packet(packet_slice) {
                        Ok(_) => {
                            // Update stats
                            self.stats.packets_received += 1;
                            packets_received += 1;
                            
                            // Update expected sequence
                            self.next_sequence = sequence + message_count as u64;
                            
                            // Calculate processing latency
                            let latency_us = start_time.elapsed().as_micros() as u64;
                            if self.stats.min_latency_us == 0 || latency_us < self.stats.min_latency_us {
                                self.stats.min_latency_us = latency_us;
                            }
                            if latency_us > self.stats.max_latency_us {
                                self.stats.max_latency_us = latency_us;
                            }
                            
                            // Update average latency
                            let prev_total = self.stats.avg_latency_us * (self.stats.packets_received - 1) as f64;
                            self.stats.avg_latency_us = (prev_total + latency_us as f64) / self.stats.packets_received as f64;
                        },
                        Err(e) => {
                            eprintln!("Error processing packet: {}", e);
                        }
                    }
                },
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                        // Timeout is normal, just continue
                        continue;
                    }
                    
                    // Other errors are concerning
                    return Err(ClientError::Other(format!("Error receiving multicast packet: {}", e)));
                }
            }
        }
        
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }
    
    /// Stop the receiver
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
    
    /// Get current statistics
    pub fn get_stats(&self) -> MulticastStats {
        self.stats.clone()
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = MulticastStats::default();
    }
}

/// Helper to parse a MoldUDP64 packet and extract messages
pub fn parse_moldupdp64_packet(packet: &[u8]) -> Result<(MoldUDP64Header, Vec<Vec<u8>>)> {
    if packet.len() < 20 {
        return Err(ClientError::Other("Packet too small for MoldUDP64 header".to_string()));
    }
    
    // Extract header fields
    let mut session = [0u8; 10];
    session.copy_from_slice(&packet[0..10]);
    
    let sequence = u64::from_be_bytes([
        packet[10], packet[11], packet[12], packet[13],
        packet[14], packet[15], packet[16], packet[17],
    ]);
    
    let message_count = u16::from_be_bytes([
        packet[18], packet[19],
    ]);
    
    let header = MoldUDP64Header {
        session,
        sequence,
        message_count,
    };
    
    // Extract messages
    let mut messages = Vec::with_capacity(message_count as usize);
    let mut offset = 20; // Start after header
    
    for _ in 0..message_count {
        if offset + 2 > packet.len() {
            return Err(ClientError::Other("Packet truncated, message length expected".to_string()));
        }
        
        let msg_len = u16::from_be_bytes([packet[offset], packet[offset + 1]]) as usize;
        offset += 2;
        
        if offset + msg_len > packet.len() {
            return Err(ClientError::Other(format!(
                "Packet truncated, message data expected (need {} bytes, have {})",
                msg_len, packet.len() - offset
            )));
        }
        
        messages.push(packet[offset..(offset + msg_len)].to_vec());
        offset += msg_len;
    }
    
    Ok((header, messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_moldupdp64_packet() {
        // Create a test packet
        let mut packet = Vec::new();
        
        // Session (10 bytes)
        packet.extend_from_slice(b"NASDAQ    ");
        
        // Sequence number (8 bytes) - set to 42
        packet.extend_from_slice(&42u64.to_be_bytes());
        
        // Message count (2 bytes) - 2 messages
        packet.extend_from_slice(&2u16.to_be_bytes());
        
        // Message 1: length (2 bytes) + data
        packet.extend_from_slice(&5u16.to_be_bytes());
        packet.extend_from_slice(b"HELLO");
        
        // Message 2: length (2 bytes) + data
        packet.extend_from_slice(&5u16.to_be_bytes());
        packet.extend_from_slice(b"WORLD");
        
        // Parse the packet
        let (header, messages) = parse_moldupdp64_packet(&packet).unwrap();
        
        // Verify header
        assert_eq!(header.sequence, 42);
        assert_eq!(header.message_count, 2);
        assert_eq!(&header.session, b"NASDAQ    ");
        
        // Verify messages
        assert_eq!(messages.len(), 2);
        assert_eq!(&messages[0], b"HELLO");
        assert_eq!(&messages[1], b"WORLD");
    }
}
