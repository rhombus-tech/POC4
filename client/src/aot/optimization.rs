/*!
 * Optimizations for ITCH Message Processing
 * 
 * This module provides specific optimizations for ITCH message processing,
 * focusing on high-performance, low-latency market data handling in TEE environments.
 */

use crate::error::AdapterError;
use crate::itch::types::{MessageType, ITCHMessage};
use super::{AotConfig, MarketProfile};
use std::collections::HashMap;
use std::time::Instant;

/// Optimization profile for different hardware targets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareTarget {
    /// Intel SGX TEE environment
    IntelSGX,
    /// AMD SEV TEE environment
    AMDSEV,
    /// Generic hardware (no TEE)
    Generic,
}

/// Profile-guided optimization data
#[derive(Debug, Clone)]
pub struct PgoProfile {
    /// Common message type sequences
    pub common_sequences: Vec<Vec<MessageType>>,
    /// Hot paths through the message processing
    pub hot_paths: HashMap<MessageType, Vec<(MessageType, f64)>>,
    /// Message type probabilities
    pub type_probabilities: HashMap<MessageType, f64>,
}

/// Optimization metrics
#[derive(Debug, Default, Clone)]
pub struct OptimizationMetrics {
    /// Time saved by AOT optimizations in nanoseconds
    pub time_saved_ns: u64,
    /// Percentage improvement over baseline
    pub percentage_improvement: f64,
    /// Cache efficiency (hit rate)
    pub cache_hit_rate: f64,
    /// Memory overhead in bytes
    pub memory_overhead_bytes: usize,
}

/// Main optimization controller
pub struct OptimizationController {
    /// Configuration for optimizations
    config: AotConfig,
    /// Current market profile
    market_profile: MarketProfile,
    /// Target hardware
    hardware_target: HardwareTarget,
    /// Profile-guided optimization data
    pgo_data: Option<PgoProfile>,
    /// Performance metrics
    metrics: OptimizationMetrics,
    /// Message processing timings for baseline comparison
    baseline_timings: HashMap<MessageType, u64>,
    /// Message processing timings with optimizations
    optimized_timings: HashMap<MessageType, u64>,
}

impl OptimizationController {
    /// Create a new optimization controller
    pub fn new(config: AotConfig, hardware_target: HardwareTarget) -> Self {
        Self {
            config,
            market_profile: MarketProfile::Normal,
            hardware_target,
            pgo_data: None,
            metrics: OptimizationMetrics::default(),
            baseline_timings: HashMap::new(),
            optimized_timings: HashMap::new(),
        }
    }
    
    /// Set the current market profile
    pub fn set_market_profile(&mut self, profile: MarketProfile) {
        self.market_profile = profile;
        // Adjust optimizations based on new profile
        self.adjust_optimizations();
    }
    
    /// Load profile-guided optimization data
    pub fn load_pgo_data(&mut self, pgo_data: PgoProfile) {
        self.pgo_data = Some(pgo_data);
        // Apply PGO-based optimizations
        self.apply_pgo_optimizations();
    }
    
    /// Adjust optimizations based on current market profile
    fn adjust_optimizations(&mut self) {
        // Different optimization strategies for different market profiles
        match self.market_profile {
            MarketProfile::Opening => {
                // Market opening has lots of adds, few executions
                // Optimize add order message handling
            },
            MarketProfile::HighVolatility => {
                // High volatility has many executions and cancels
                // Optimize execution and cancel message handling
            },
            MarketProfile::LowLiquidity => {
                // Low liquidity has fewer messages but more critical timing
                // Focus on latency over throughput
            },
            MarketProfile::Normal | MarketProfile::Custom => {
                // Balanced optimizations
            }
        }
        
        // Adjust for hardware target
        match self.hardware_target {
            HardwareTarget::IntelSGX => {
                // Optimize for SGX memory constraints
            },
            HardwareTarget::AMDSEV => {
                // Optimize for SEV performance characteristics
            },
            HardwareTarget::Generic => {
                // General optimizations
            }
        }
    }
    
    /// Apply profile-guided optimizations
    fn apply_pgo_optimizations(&mut self) {
        // Clone the data we need to avoid borrow conflicts
        let sequences_to_optimize = if let Some(ref pgo_data) = &self.pgo_data {
            pgo_data.common_sequences.clone()
        } else {
            Vec::new()
        };
        
        // Process common sequences
        for sequence in &sequences_to_optimize {
            if !sequence.is_empty() {
                self.optimize_sequence(sequence);
            }
        }
        
        // Now handle hot paths - collect them first to avoid borrow issues
        let mut transitions_to_optimize = Vec::new();
        if let Some(ref pgo_data) = &self.pgo_data {
            for (src_type, destinations) in &pgo_data.hot_paths {
                for (dst_type, probability) in destinations {
                    if *probability > 0.2 {
                        transitions_to_optimize.push((*src_type, *dst_type));
                    }
                }
            }
        }
        
        // Now process the transitions we collected
        for (src_type, dst_type) in transitions_to_optimize {
            self.optimize_transition(src_type, dst_type);
        }
    }
    
    /// Optimize a sequence of message types
    fn optimize_sequence(&mut self, _sequence: &[MessageType]) {
        // This would generate specialized code for processing this sequence
        // For now, just track that we've seen this sequence
        
        // In production, this would generate:
        // 1. Combined parsing for consecutive messages
        // 2. Optimized memory access patterns
        // 3. Potentially fused operations across messages
    }
    
    /// Optimize transitions between message types
    fn optimize_transition(&mut self, _from_type: MessageType, _to_type: MessageType) {
        // This would optimize the state transition between message types
        // For example, by preparing data structures for the next message
        
        // In production, this would:
        // 1. Pre-allocate buffers for the likely next message
        // 2. Prefetch relevant data for the next message
        // 3. Keep state between messages to avoid recalculation
    }
    
    /// Process a message with timing
    pub fn process_message(&mut self, 
                          message_data: &[u8], 
                          use_optimizations: bool,
                          process_fn: impl Fn(&[u8]) -> Result<ITCHMessage, AdapterError>) -> Result<ITCHMessage, AdapterError> {
        if message_data.is_empty() {
            return Err(AdapterError::ResponseParsing("Empty message".to_string()));
        }
        
        let msg_type = MessageType::from(message_data[0]);
        
        // Time the processing
        let start = Instant::now();
        let result = process_fn(message_data);
        let elapsed = start.elapsed().as_nanos() as u64;
        
        // Store timing data
        if use_optimizations {
            self.optimized_timings.entry(msg_type)
                .and_modify(|timing| *timing = (*timing * 9 + elapsed) / 10) // Rolling average
                .or_insert(elapsed);
        } else {
            self.baseline_timings.entry(msg_type)
                .and_modify(|timing| *timing = (*timing * 9 + elapsed) / 10) // Rolling average
                .or_insert(elapsed);
        }
        
        // Update metrics
        self.update_metrics();
        
        result
    }
    
    /// Update optimization metrics
    fn update_metrics(&mut self) {
        let mut total_baseline = 0u64;
        let mut total_optimized = 0u64;
        let mut matching_types = 0usize;
        
        // Calculate time saved across all message types
        for (msg_type, baseline_time) in &self.baseline_timings {
            if let Some(optimized_time) = self.optimized_timings.get(msg_type) {
                let _time_diff = if *baseline_time > *optimized_time {
                    baseline_time - optimized_time
                } else {
                    0 // No improvement or regression
                };
                
                total_baseline += *baseline_time;
                total_optimized += *optimized_time;
                matching_types += 1;
            }
        }
        
        // Calculate percentage improvement
        if total_baseline > 0 && matching_types > 0 {
            self.metrics.time_saved_ns = if total_baseline > total_optimized {
                total_baseline - total_optimized
            } else {
                0 // No overall improvement
            };
            
            self.metrics.percentage_improvement = 
                if total_baseline > total_optimized {
                    ((total_baseline - total_optimized) as f64 / total_baseline as f64) * 100.0
                } else {
                    0.0 // No improvement or regression
                };
        }
    }
    
    /// Get the current optimization metrics
    pub fn get_metrics(&self) -> OptimizationMetrics {
        self.metrics.clone()
    }
    
    /// Get optimization recommendations based on current performance
    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        // Analyze current performance and suggest improvements
        if self.metrics.percentage_improvement < 5.0 {
            recommendations.push("Consider enabling more aggressive optimizations".to_string());
        }
        
        if self.metrics.memory_overhead_bytes > 10_000_000 {
            recommendations.push("Memory overhead is high, consider reducing cache sizes".to_string());
        }
        
        if self.metrics.cache_hit_rate < 0.5 {
            recommendations.push("Cache hit rate is low, consider adjusting caching strategy".to_string());
        }
        
        // Add hardware-specific recommendations
        match self.hardware_target {
            HardwareTarget::IntelSGX => {
                recommendations.push("Optimize for SGX EPC size constraints".to_string());
            },
            HardwareTarget::AMDSEV => {
                recommendations.push("Consider SEV-specific memory access patterns".to_string());
            },
            _ => {}
        }
        
        // Add market profile recommendations
        match self.market_profile {
            MarketProfile::HighVolatility => {
                recommendations.push("Pre-allocate larger order pools for high volatility".to_string());
            },
            MarketProfile::Opening => {
                recommendations.push("Optimize for burst processing during market opening".to_string());
            },
            _ => {}
        }
        
        recommendations
    }
}

/// Generate PGO profile from ITCH message samples
pub fn generate_pgo_profile(messages: &[ITCHMessage]) -> PgoProfile {
    let mut common_sequences = Vec::new();
    let mut hot_paths = HashMap::new();
    let mut type_probabilities = HashMap::new();
    let mut seq_counts = HashMap::new();
    
    // Process messages to extract patterns
    if !messages.is_empty() {
        // Count message types
        for msg in messages {
            *type_probabilities.entry(msg.message_type).or_insert(0.0) += 1.0;
        }
        
        // Normalize probabilities
        let total = messages.len() as f64;
        for count in type_probabilities.values_mut() {
            *count /= total;
        }
        
        // Find common sequences (up to 3 messages)
        for window_size in 2..=3 {
            if messages.len() >= window_size {
                for i in 0..=(messages.len() - window_size) {
                    let seq: Vec<_> = messages[i..(i + window_size)]
                        .iter()
                        .map(|m| m.message_type)
                        .collect();
                    
                    *seq_counts.entry(seq.clone()).or_insert(0) += 1;
                }
            }
        }
        
        // Find hot paths (transitions between message types)
        for i in 0..(messages.len() - 1) {
            let from_type = messages[i].message_type;
            let to_type = messages[i + 1].message_type;
            
            hot_paths.entry(from_type)
                .or_insert_with(Vec::new)
                .push((to_type, 1.0)); // Will normalize later
        }
        
        // Normalize hot path probabilities
        for transitions in hot_paths.values_mut() {
            // Count occurrences of each destination
            let mut counts = HashMap::new();
            for (dst, _) in transitions.iter() {
                *counts.entry(*dst).or_insert(0) += 1;
            }
            
            // Calculate probabilities
            let total = transitions.len();
            for (dst, count) in &counts {
                for (t_dst, prob) in transitions.iter_mut() {
                    if t_dst == dst {
                        *prob = *count as f64 / total as f64;
                    }
                }
            }
        }
        
        // Extract most common sequences
        let mut seq_vec: Vec<_> = seq_counts.into_iter().collect();
        seq_vec.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by count, descending
        
        // Take top 10 sequences
        common_sequences = seq_vec.into_iter()
            .take(10)
            .map(|(seq, _)| seq)
            .collect();
    }
    
    PgoProfile {
        common_sequences,
        hot_paths,
        type_probabilities,
    }
}
