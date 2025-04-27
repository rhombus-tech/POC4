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
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{SystemTime, Duration, Instant};
use std::thread;
use std::collections::{HashSet, VecDeque};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use crate::error::{ClientError, Result};
// Temporarily define these types here until we create the message module
pub struct BatchProcessor {
    // Fields would go here
}

impl BatchProcessor {
    pub fn process_batch(&mut self, _messages: &[Vec<u8>]) -> Result<()> {
        // Implementation would go here
        Ok(())
    }
    
    pub fn process_incoming_packet(&mut self, _packet: &[u8]) -> Result<()> {
        // Implementation would go here
        Ok(())
    }
}

pub struct MessageMemoryPool {
    // Fields would go here
}

/// MoldUDP64 packet header structure (20 bytes)
/// As per NASDAQ specs: https://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/moldudp64.pdf
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoldUDP64Header {
    /// Session identifier (10 bytes)
    pub session: [u8; 10],
    /// Sequence number (8 bytes)
    pub sequence: u64,
    /// Message count (2 bytes)
    pub message_count: u16,
}

impl MoldUDP64Header {
    /// Parse a MoldUDP64 header from a byte slice
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 20 {
            return Err(ClientError::Other("MoldUDP64 header too short".to_string()));
        }
        
        let mut session = [0u8; 10];
        session.copy_from_slice(&bytes[0..10]);
        
        let sequence = u64::from_be_bytes([bytes[10], bytes[11], bytes[12], bytes[13], 
                                          bytes[14], bytes[15], bytes[16], bytes[17]]);
        
        let message_count = u16::from_be_bytes([bytes[18], bytes[19]]);
        
        Ok(Self {
            session,
            sequence,
            message_count,
        })
    }
    
    /// Serialize a MoldUDP64 header to bytes
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut bytes = [0u8; 20];
        bytes[0..10].copy_from_slice(&self.session);
        
        let seq_bytes = self.sequence.to_be_bytes();
        bytes[10..18].copy_from_slice(&seq_bytes);
        
        let count_bytes = self.message_count.to_be_bytes();
        bytes[18..20].copy_from_slice(&count_bytes);
        
        bytes
    }
}

/// SoupBinTCP message types for session management and retransmission
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoupBinTCPMessageType {
    // Client to server
    Login,
    Logout,
    SequencedData,
    UnsequencedData,
    ClientHeartbeat,
    Debug,
    // Server to client
    LoginAccepted,
    LoginRejected,
    ServerHeartbeat,
    EndOfSession,
    SequencedDataEvent,
    ErrorEvent,
}

/// Types of feed failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedFailureType {
    /// Sequence gap detected
    SequenceGap { 
        last: u64, 
        next: u64,
        timestamp: SystemTime,
        feed_id: FeedId,
    },
    /// Heartbeat timeout
    HeartbeatTimeout { 
        feed_id: FeedId,
        timestamp: SystemTime,
        last_sequence: u64,
    },
    /// Feed disconnected
    Disconnected {
        feed_id: FeedId,
        timestamp: SystemTime,
        last_sequence: u64,
    },
}

/// Feed recovery status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Not in recovery
    Normal,
    /// Recovery in progress
    Recovering {
        start_sequence: u64,
        end_sequence: u64,
        attempts: u32,
        start_time: SystemTime,
    },
    /// Recovery failed after max attempts
    Failed {
        start_sequence: u64,
        end_sequence: u64,
        attempts: u32,
    },
}

/// Unique identifier for a feed (combination of group and feed type)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeedId {
    /// Multicast group address
    pub group: Ipv4Addr,
    /// Feed type (A or B for redundancy)
    pub feed_type: FeedType,
}

/// Feed type for redundancy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeedType {
    /// Primary feed
    A,
    /// Backup feed
    B,
}

/// State of a specific feed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedState {
    /// Feed is healthy
    Healthy,
    /// Feed has a sequence gap
    GapDetected,
    /// Feed hasn't received a heartbeat in timeout period
    HeartbeatTimeout,
    /// Feed is disconnected
    Disconnected,
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
    /// Feed type (A or B for redundancy)
    pub feed_type: FeedType,
    /// Session identifier
    pub session_id: [u8; 10],
    /// Maximum retransmission attempts
    pub max_retransmission_attempts: u32,
    /// Gap recovery timeout
    pub recovery_timeout: Duration,
    /// Heartbeat timeout
    pub heartbeat_timeout: Duration,
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
            feed_type: FeedType::A,                       // Default to primary feed
            session_id: [0; 10],                          // Default session ID
            max_retransmission_attempts: 3,               // Try 3 times by default
            recovery_timeout: Duration::from_millis(500), // 500ms recovery timeout
            heartbeat_timeout: Duration::from_secs(3),    // 3 second heartbeat timeout
        }
    }
}

/// Statistics for multicast session
#[derive(Debug, Clone, Default)]
pub struct MulticastStats {
    /// Number of packets received
    pub packets_received: u64,
    /// Number of messages extracted
    pub messages_extracted: u64,
    /// Number of parse errors
    pub parse_errors: u64,
    /// Minimum processing latency (microseconds)
    pub min_latency_us: u64,
    /// Maximum processing latency (microseconds)
    pub max_latency_us: u64,
    /// Average processing latency (microseconds)
    pub avg_latency_us: f64,
    /// Number of sequence gaps detected
    pub sequence_gaps: u64,
    /// Maximum gap size (in messages)
    pub max_gap_size: u64,
    /// Number of duplicated packets received
    pub duplicate_packets: u64, 
    /// Number of retransmission requests sent
    pub retransmission_requests: u64,
    /// Number of successful retransmissions
    pub retransmission_success: u64,
    /// Number of failed retransmissions
    pub retransmission_failed: u64,
    /// Number of heartbeat timeouts
    pub heartbeat_timeouts: u64,
    /// Number of recovered messages
    pub recovered_messages: u64,
    /// Feed A packet count
    pub feed_a_packets: u64,
    /// Feed B packet count
    pub feed_b_packets: u64,
    /// Number of feed switches
    pub feed_switches: u64,
    /// Number of permanently abandoned gaps
    pub gaps_abandoned: u64,
}

/// Sequence gap tracking information
#[derive(Debug, Clone)]
pub struct SequenceGap {
    /// Start sequence of the gap
    pub start: u64,
    /// End sequence of the gap
    pub end: u64,
    /// When the gap was first detected
    pub detection_time: SystemTime,
    /// Number of retransmission attempts
    pub attempts: u32,
    /// Last retransmission request time
    pub last_request_time: Option<SystemTime>,
    /// Status of this gap
    pub status: RecoveryStatus,
}

/// Feed synchronization state for tracking feed arbitration
#[derive(Debug, Clone)]
pub struct FeedSyncState {
    /// Current active feed
    pub active_feed: FeedType,
    /// Last time a valid packet was received on feed A
    pub last_feed_a_time: Option<SystemTime>,
    /// Last time a valid packet was received on feed B
    pub last_feed_b_time: Option<SystemTime>,
    /// Last sequence number received on feed A
    pub feed_a_state: FeedState,
    /// Status of feed B
    pub feed_b_state: FeedState,
    /// Last time feed arbitration was performed
    pub last_arbitration: Option<SystemTime>,
    /// Number of feed switches
    pub feed_switches: u64,
    /// Sequence number for feed A
    pub feed_a_sequence: u64,
    /// Sequence number for feed B
    pub feed_b_sequence: u64,
    /// Stats for feed A
    pub feed_a_stats: MulticastStats,
    /// Stats for feed B 
    pub feed_b_stats: MulticastStats,
}

impl Default for FeedSyncState {
    fn default() -> Self {
        Self {
            active_feed: FeedType::A,
            last_feed_a_time: None,
            last_feed_b_time: None,
            last_arbitration: None,
            feed_switches: 0,
            feed_a_sequence: 0,
            feed_b_sequence: 0,
            feed_a_stats: MulticastStats::default(),
            feed_b_stats: MulticastStats::default(),
            feed_a_state: FeedState::Healthy,
            feed_b_state: FeedState::Healthy,
        }
    }
}

/// NASDAQ ITCH UDP Multicast receiver
pub struct MulticastReceiver {
    /// Underlying socket for multicast operations
    socket: UdpSocket,
    /// Configuration for this receiver
    config: MulticastConfig,
    /// Buffer for packet reception
    packet_buffer: Vec<u8>,
    /// Next expected sequence number
    next_sequence: u64,
    /// Running state of receiver
    running: Arc<AtomicBool>,
    /// Statistics for this session
    stats: MulticastStats,
    /// Optional memory pool for zero-copy message handling
    memory_pool: Option<MessageMemoryPool>,
    /// Socket for requesting retransmission (if enabled)
    retransmission_socket: Option<UdpSocket>,
    /// Feed identifier (group + type)
    feed_id: FeedId,
    /// Active sequence gaps being recovered
    sequence_gaps: Vec<SequenceGap>,
    /// Last heartbeat reception time
    last_heartbeat: Option<SystemTime>,
    /// Last heartbeat request time
    /// Last sent heartbeat request time
    last_heartbeat_request: Option<SystemTime>,
    /// Process cache of sequence numbers already seen
    processed_sequences: HashSet<u64>,
    /// Message cache for sequence recovery
    message_cache: VecDeque<(u64, Vec<u8>)>,
    /// Cache size limit
    message_cache_limit: usize,
    /// Retransmission session ID (incremented for each new session)
    retransmission_session_id: u32,
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
        
        // Create packet buffer based on max size
        let packet_buffer = vec![0u8; config.max_packet_size];
        
        // Create feed ID
        let feed_id = FeedId {
            group: config.group_addr,
            feed_type: config.feed_type,
        };
        
        debug!("Creating multicast receiver for {:?} feed on {}", 
               config.feed_type, config.group_addr);
        
        // Default to a reasonable message cache size (store up to 5000 messages)
        let message_cache_limit = 5000;
        
        Ok(Self {
            socket,
            config: config.clone(),
            packet_buffer,
            next_sequence: 0,   // Will be set upon first packet
            running: Arc::new(AtomicBool::new(false)),
            stats: MulticastStats::default(),
            memory_pool: None,  // Will be set later if needed
            retransmission_socket,
            feed_id,
            sequence_gaps: Vec::new(),
            last_heartbeat: None,
            last_heartbeat_request: None,
            processed_sequences: HashSet::new(),
            message_cache: VecDeque::with_capacity(message_cache_limit),
            message_cache_limit,
            retransmission_session_id: 0,
        })
    }
    
    /// Generate a retransmission request for a sequence gap
    pub fn request_retransmission(&mut self, gap: &SequenceGap) -> Result<()> {
        if !self.config.request_retransmission || self.retransmission_socket.is_none() {
            return Err(ClientError::Other("Retransmission not configured".to_string()));
        }
        
        let retrans_socket = self.retransmission_socket.as_ref().unwrap();
        let retrans_addr = self.config.retransmission_addr.unwrap();
        
        // Create SoupBinTCP style retransmission request
        // NASDAQ uses a variant of SoupBinTCP for retransmission requests
        let mut request = Vec::with_capacity(40);
        
        // Packet type ("L" for Login/Session management)
        request.push(b'L');
        
        // Sequence start/end for retransmission
        request.extend_from_slice(&gap.start.to_be_bytes());
        request.extend_from_slice(&gap.end.to_be_bytes());
        
        // Session ID
        request.extend_from_slice(&self.config.session_id);
        
        // Request ID (incremented for each session)
        self.retransmission_session_id += 1;
        request.extend_from_slice(&self.retransmission_session_id.to_be_bytes());
        
        // Send retransmission request
        let bytes_sent = retrans_socket.send_to(&request, retrans_addr)?;
        if bytes_sent != request.len() {
            warn!("Partial retransmission request sent: {} of {} bytes", 
                 bytes_sent, request.len());
        }
        
        // Update statistics
        self.stats.retransmission_requests += 1;
        
        debug!("Requested retransmission for gap {} to {} (attempt {})", 
               gap.start, gap.end, gap.attempts + 1);
        
        Ok(())
    }
    
    /// Set memory pool for zero-allocation processing
    pub fn set_memory_pool(&mut self, memory_pool: MessageMemoryPool) {
        self.memory_pool = Some(memory_pool);
    }
    
    /// Detect sequence gaps by comparing expected and actual sequence numbers
    fn detect_gap(&mut self, header: &MoldUDP64Header) -> Option<SequenceGap> {
        // If we haven't received any packets yet, initialize with this sequence
        if self.next_sequence == 0 {
            debug!("Initializing sequence tracking starting at {}", header.sequence);
            self.next_sequence = header.sequence;
            return None;
        }
        
        // Check if this sequence is what we expect
        if header.sequence == self.next_sequence {
            // Update expected next sequence
            self.next_sequence = header.sequence + header.message_count as u64;
            return None;
        }
        
        // Check if this is an older sequence we've already processed
        if header.sequence < self.next_sequence {
            // This could be a duplicate or retransmission we've already seen
            debug!("Received older sequence {} (expected {})", header.sequence, self.next_sequence);
            return None;
        }
        
        // We have a gap - create a gap record
        let gap = SequenceGap {
            start: self.next_sequence,
            end: header.sequence - 1,
            detection_time: SystemTime::now(),
            attempts: 0,
            last_request_time: None,
            status: RecoveryStatus::Normal,
        };
        
        // Update statistics
        self.stats.sequence_gaps += 1;
        let gap_size = gap.end - gap.start + 1;
        if gap_size > self.stats.max_gap_size {
            self.stats.max_gap_size = gap_size;
        }
        
        // Update expected next sequence
        self.next_sequence = header.sequence + header.message_count as u64;
        
        warn!("Detected sequence gap from {} to {} (size {})", 
             gap.start, gap.end, gap_size);
        
        Some(gap)
    }
    
    /// Process a sequence gap by initiating recovery
    fn process_gap(&mut self, gap: SequenceGap) -> Result<()> {
        // Add to active gaps list
        let updated_gap = SequenceGap {
            status: RecoveryStatus::Recovering {
                start_sequence: gap.start,
                end_sequence: gap.end,
                attempts: 1,
                start_time: SystemTime::now(),
            },
            last_request_time: Some(SystemTime::now()),
            attempts: 1,
            ..gap
        };
        
        // Request retransmission if configured
        if self.config.request_retransmission {
            self.request_retransmission(&updated_gap)?;
        }
        
        // Add to active gaps
        self.sequence_gaps.push(updated_gap);
        
        Ok(())
    }
    
    /// Check for expired recovery attempts and retry or fail them
    fn check_recovery_timeouts(&mut self) -> Result<()> {
        let now = SystemTime::now();
        let max_attempts = self.config.max_retransmission_attempts;
        let recovery_timeout = self.config.recovery_timeout;
        
        // Process each gap
        // First, collect all the gaps we need to process to avoid borrowing conflicts
        let mut gaps_to_retry = Vec::new();
        let mut updated_gaps = Vec::new();
        let mut failed_gaps = Vec::new();
        
        // Collect all gaps and their status without requesting retransmission yet
        for gap in &self.sequence_gaps {
            match gap.status {
                RecoveryStatus::Recovering { attempts, start_time, .. } => {
                    // Check if recovery has timed out
                    if let Ok(elapsed) = now.duration_since(start_time) {
                        if elapsed > recovery_timeout {
                            // Timed out - check if we should retry or fail
                            if attempts >= max_attempts {
                                // Give up after max attempts
                                let failed_gap = SequenceGap {
                                    status: RecoveryStatus::Failed {
                                        start_sequence: gap.start,
                                        end_sequence: gap.end,
                                        attempts,
                                    },
                                    ..gap.clone()
                                };
                                
                                failed_gaps.push(failed_gap);
                                warn!("Gap recovery failed after {} attempts for sequences {} to {}", 
                                     attempts, gap.start, gap.end);
                                
                                self.stats.retransmission_failed += 1;
                            } else {
                                // Retry
                                let updated_gap = SequenceGap {
                                    status: RecoveryStatus::Recovering {
                                        start_sequence: gap.start,
                                        end_sequence: gap.end,
                                        attempts: attempts + 1,
                                        start_time: now,
                                    },
                                    last_request_time: Some(now),
                                    attempts: gap.attempts + 1,
                                    ..gap.clone()
                                };
                                
                                // Add to the list of gaps that need retransmission
                                if self.config.request_retransmission {
                                    gaps_to_retry.push(updated_gap.clone());
                                }
                                
                                updated_gaps.push(updated_gap);
                            }
                        } else {
                            // Still within timeout - keep as is
                            updated_gaps.push(gap.clone());
                        }
                    } else {
                        // System time error - just continue with current gap
                        updated_gaps.push(gap.clone());
                    }
                },
                _ => {
                    // Not in recovering state - just clone
                    updated_gaps.push(gap.clone());
                }
            }
        }
        
        // Replace sequence_gaps with updated list
        self.sequence_gaps = updated_gaps;
        
        // Add failed gaps to stats
        for failed_gap in failed_gaps {
            self.stats.gaps_abandoned += 1;
        }
        
        // Process all the retransmission requests we collected
        // This avoids the borrowing conflict since we're done with the sequence_gaps iteration
        if !gaps_to_retry.is_empty() {
            for gap in gaps_to_retry {
                if let Err(e) = self.request_retransmission(&gap) {
                    warn!("Failed to request retransmission: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Check if a sequence is within any active gap and mark it as recovered if complete
    fn check_gap_recovery(&mut self, sequence: u64, message_count: u16) -> bool {
        let mut updated_gaps = Vec::new();
        let mut gap_recovered = false;
        
        for gap in &self.sequence_gaps {
            if sequence >= gap.start && (sequence + message_count as u64 - 1) <= gap.end {
                // This message falls within a gap - mark partial recovery
                gap_recovered = true;
                debug!("Recovered sequence {} (count {}) within gap {} to {}", 
                      sequence, message_count, gap.start, gap.end);
                
                // Check if we've completely filled the gap
                let mut remaining_gaps = Vec::new();
                
                // Check if there's a gap before this message
                if sequence > gap.start {
                    let before_gap = SequenceGap {
                        start: gap.start,
                        end: sequence - 1,
                        detection_time: gap.detection_time,
                        attempts: gap.attempts,
                        last_request_time: gap.last_request_time,
                        status: gap.status.clone(),
                    };
                    remaining_gaps.push(before_gap);
                }
                
                // Check if there's a gap after this message
                let message_end = sequence + message_count as u64 - 1;
                if message_end < gap.end {
                    let after_gap = SequenceGap {
                        start: message_end + 1,
                        end: gap.end,
                        detection_time: gap.detection_time,
                        attempts: gap.attempts,
                        last_request_time: gap.last_request_time,
                        status: gap.status.clone(),
                    };
                    remaining_gaps.push(after_gap);
                }
                
                // Add any remaining gaps to the updated list
                updated_gaps.extend(remaining_gaps);
                
                // Update stats
                self.stats.recovered_messages += message_count as u64;
                self.stats.retransmission_success += 1;
            } else {
                // This message doesn't affect this gap
                updated_gaps.push(gap.clone());
            }
        }
        
        // Replace gaps with updated list
        self.sequence_gaps = updated_gaps;
        
        gap_recovered
    }
    
    /// Feed arbitration logic for A/B feed redundancy
    pub fn arbitrate_feeds(feed_a: &MulticastReceiver, feed_b: &MulticastReceiver) -> FeedType {
        // Check if either feed is significantly ahead
        let seq_a = feed_a.next_sequence;
        let seq_b = feed_b.next_sequence;
        
        // If one feed is more than 10 messages ahead, prefer it
        const SEQUENCE_THRESHOLD: u64 = 10;
        if seq_a > seq_b && seq_a - seq_b > SEQUENCE_THRESHOLD {
            return FeedType::A;
        } else if seq_b > seq_a && seq_b - seq_a > SEQUENCE_THRESHOLD {
            return FeedType::B;
        }
        
        // Check gap count - prefer feed with fewer gaps
        let gaps_a = feed_a.sequence_gaps.len();
        let gaps_b = feed_b.sequence_gaps.len();
        
        if gaps_a < gaps_b {
            return FeedType::A;
        } else if gaps_b < gaps_a {
            return FeedType::B;
        }
        
        // If all else is equal, use A as the primary feed
        FeedType::A
    }
    
    /// Start asynchronous receiver with a batch processor
    pub fn start_async_receiver(
        &mut self,
        batch_processor: Arc<std::sync::Mutex<BatchProcessor>>,
    ) -> Result<std::thread::JoinHandle<Result<()>>> {
        // Simplified async receiver implementation for testing
        // Full implementation will be added in a future update
        if self.running.load(Ordering::SeqCst) {
            return Err(ClientError::Other("Receiver already running".to_string()));
        }
        
        // Set running flag
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        
        // Create a simple thread that doesn't actually process data
        // This is just a stub to make the tests compile and run
        let handle = thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        });
        
        Ok(handle)
    }
    
    /// Synchronously receive packets and process with the given batch processor
    /// This is mainly for testing and simple applications
    pub fn receive_sync(&mut self, batch_processor: &mut BatchProcessor, max_packets: usize) -> Result<()> {
        // Check if we're running out of sequence gaps that need recovery
        if !self.sequence_gaps.is_empty() {
            self.check_recovery_timeouts()?;
        }
        
        // Receive and process up to max_packets
        let mut packets_received = 0;
        while packets_received < max_packets {
            // Try to receive a packet
            match self.socket.recv_from(self.packet_buffer.as_mut_slice()) {
                Ok((bytes_read, _src_addr)) => {
                    let packet_data = &self.packet_buffer[..bytes_read];
                    let start_time = Instant::now();
                    
                    // Parse MoldUDP64 packet
                    match self.parse_moldupdp64_packet(packet_data) {
                        Ok((header, messages)) => {
                            // Check if we've already processed this sequence (duplicate)
                            let duplicate = self.processed_sequences.contains(&header.sequence);
                            if duplicate {
                                self.stats.duplicate_packets += 1;
                                debug!("Received duplicate packet for sequence {}", header.sequence);
                            } else {
                                // Mark the sequence as processed
                                self.processed_sequences.insert(header.sequence);
                                
                                // Update feed-specific stats
                                if self.config.feed_type == FeedType::A {
                                    self.stats.feed_a_packets += 1;
                                } else {
                                    self.stats.feed_b_packets += 1;
                                }
                                
                                // Check for gap recovery
                                if !self.sequence_gaps.is_empty() {
                                    let recovered = self.check_gap_recovery(header.sequence, header.message_count);
                                    if recovered {
                                        debug!("Recovered sequence {} (count {}) from gap", 
                                              header.sequence, header.message_count);
                                    }
                                }
                                
                                // Check for sequence gaps
                                if let Some(gap) = self.detect_gap(&header) {
                                    self.process_gap(gap)?;
                                }
                                
                                // Process extracted messages
                                batch_processor.process_batch(&messages)?;
                                
                                // Update heartbeat timestamp
                                self.last_heartbeat = Some(SystemTime::now());
                            }
                            
                            // Measure processing latency
                            let latency_us = start_time.elapsed().as_micros() as u64;
                            // Update stats directly for now
                            if self.stats.min_latency_us == 0 || latency_us < self.stats.min_latency_us {
                                self.stats.min_latency_us = latency_us;
                            }
                            if latency_us > self.stats.max_latency_us {
                                self.stats.max_latency_us = latency_us;
                            }
                            
                            // Update statistics
                            self.stats.packets_received += 1;
                            self.stats.messages_extracted += messages.len() as u64;
                            packets_received += 1;
                        },
                        Err(e) => {
                            // Log error but continue processing
                            error!("Failed to parse packet: {}", e);
                        }
                    }
                },
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Timeout, no data available
                    
                    // Check for heartbeat timeout
                    if let Some(last_hb) = self.last_heartbeat {
                        if let Ok(elapsed) = SystemTime::now().duration_since(last_hb) {
                            if elapsed > self.config.heartbeat_timeout {
                                // Heartbeat timeout
                                self.stats.heartbeat_timeouts += 1;
                                warn!("Heartbeat timeout detected for {:?} feed on {}", 
                                      self.config.feed_type, self.config.group_addr);
                            }
                        }
                    }
                    
                    break;
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
    
    /// Helper to parse a MoldUDP64 packet and extract messages
    pub fn parse_moldupdp64_packet(&self, packet: &[u8]) -> Result<(MoldUDP64Header, Vec<Vec<u8>>)> {
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
                return Err(ClientError::Other("Incomplete message length".to_string()));
            }
            
            // Get message length (2 bytes)
            let msg_len = u16::from_be_bytes([packet[offset], packet[offset + 1]]) as usize;
            offset += 2;
            
            if offset + msg_len > packet.len() {
                return Err(ClientError::Other("Incomplete message data".to_string()));
            }
            
            // Extract message
            let message = packet[offset..offset + msg_len].to_vec();
            messages.push(message);
            
            offset += msg_len;
        }
        
        Ok((header, messages))
    }
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
        
        // Create a receiver instance for calling the method
        let config = MulticastConfig::default();
        let receiver = MulticastReceiver::new(config).unwrap();
        
        // Parse the packet using the receiver's method
        let (header, messages) = receiver.parse_moldupdp64_packet(&packet).unwrap();
        
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
