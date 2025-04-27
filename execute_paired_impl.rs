pub async fn execute_paired(
    &self,
    target_tee: String,
    region_id: String,
    tee_type: String,
    input: Vec<u8>,
    timeout: Duration,
    is_async: bool,
    allow_fallback: bool,
) -> Result<MeshExecutionResult, std::io::Error> {
    info!("Executing paired execution: target={}, region={}, type={}", 
          target_tee, region_id, tee_type);
    
    // Generate cache key
    let cache_key = self.generate_cache_key(&target_tee, &region_id, &tee_type, &input);
    
    // Check cache if requested
    if let Some(cached_result) = self.check_cache(&cache_key, timeout) {
        info!("Using cached result for paired execution");
        return Ok(cached_result);
    }
    
    // Find a complementary TEE of the other type in the same region
    // If primary is SGX, find a SEV TEE, and vice versa
    let complementary_tee_type = if tee_type == "IntelSGX" { "SEV" } else { "IntelSGX" };
    let mut complementary_tee = None;
    {
        let peers = self.peers.read().unwrap();
        
        // Find a paired TEE of the complementary type in the same region
        for (id, peer) in peers.iter() {
            if id != &target_tee && 
               peer.region_id == region_id && 
               peer.tee_type == complementary_tee_type &&
               peer.is_healthy {
                complementary_tee = Some(id.clone());
                debug!("Found complementary TEE {} of type {} for primary TEE {} of type {}", 
                       id, complementary_tee_type, target_tee, tee_type);
                break;
            }
        }
    }
    
    // Start tracking execution time for metrics
    let start_time = std::time::Instant::now();
    
    // Execute on primary TEE first
    let primary_result = match self.execute(
        target_tee.clone(), 
        region_id.clone(), 
        tee_type.clone(), 
        input.clone(), 
        timeout.clone(), 
        is_async, 
        false // Don't allow fallback for primary attempt
    ).await {
        Ok(result) => result,
        Err(e) => {
            // If primary execution fails and we have a complementary TEE, proceed with it
            if let Some(comp_tee) = &complementary_tee {
                warn!("Primary TEE {} of type {} execution failed: {}. Failing over to complementary TEE {} of type {}", 
                      target_tee, tee_type, e, comp_tee, complementary_tee_type);
                
                // Mark the connection as failed
                if let Ok(tee_type_enum) = tee_type.parse::<TeeType>() {
                    self.connection_pool.mark_connection_failed(&target_tee, &region_id, &tee_type_enum);
                }
                
                // Execute on complementary TEE as failover
                let complementary_result = self.execute(
                    comp_tee.clone(), 
                    region_id.clone(), 
                    complementary_tee_type.to_string(), 
                    input.clone(), 
                    timeout, 
                    is_async, 
                    allow_fallback
                ).await?;
                
                let mut result = complementary_result;
                result.execution_type = "paired_failover".to_string();
                
                // Store in cache with TTL
                self.store_in_cache(&cache_key, &result, Duration::from_secs(3600));
                
                // Attempt recovery of primary TEE (async)
                let primary_tee = target_tee.clone();
                let region = region_id.clone();
                let tee_type_str = tee_type.clone();
                tokio::spawn(async move {
                    debug!("Attempting recovery of failed primary TEE: {}", primary_tee);
                    // Wait before attempting recovery
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    
                    // This is a placeholder for the actual recovery logic
                    // In a real implementation, this would include:
                    // - Restarting the TEE service
                    // - Re-verifying its attestation
                    // - Syncing its state with the complementary TEE
                    
                    debug!("Recovery attempt for TEE {} completed", primary_tee);
                });
                
                return Ok(result);
            } else if allow_fallback {
                // If no complementary TEE and fallback is allowed, attempt to find any healthy TEE
                warn!("Primary TEE {} execution failed and no complementary TEE available. Attempting fallback.", target_tee);
                return self.try_executor(
                    "".to_string(), // Empty target means find any healthy executor
                    region_id.clone(),
                    tee_type.clone(),
                    input.clone(),
                    timeout,
                    is_async,
                    true
                ).await;
            } else {
                // No fallback options, return error
                return Err(e);
            }
        }
    };
    
    // If we have a complementary TEE, execute on it as well to verify results
    if let Some(comp_tee_id) = complementary_tee {
        let complementary_result = match self.execute(
            comp_tee_id.clone(), 
            region_id.clone(), 
            complementary_tee_type.to_string(), 
            input.clone(), 
            timeout.clone(), 
            is_async, 
            false // Don't allow fallback for verification
        ).await {
            Ok(result) => result,
            Err(e) => {
                // If complementary execution fails, log warning but return primary result
                warn!("Complementary TEE {} of type {} execution failed: {}. Using primary result.", 
                      comp_tee_id, complementary_tee_type, e);
                
                // Mark the connection as failed
                if let Ok(tee_type_enum) = complementary_tee_type.parse::<TeeType>() {
                    self.connection_pool.mark_connection_failed(&comp_tee_id, &region_id, &tee_type_enum);
                }
                
                // Return primary result
                let mut result = primary_result;
                result.execution_type = "paired_primary_only".to_string();
                
                // Store in cache with TTL
                self.store_in_cache(&cache_key, &result, Duration::from_secs(3600));
                
                // Attempt recovery of complementary TEE (async)
                let comp_tee = comp_tee_id.clone();
                let region = region_id.clone();
                let comp_type_str = complementary_tee_type.to_string();
                tokio::spawn(async move {
                    debug!("Attempting recovery of failed complementary TEE: {}", comp_tee);
                    // Wait before attempting recovery
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    
                    // Placeholder for actual recovery logic
                    debug!("Recovery attempt for TEE {} completed", comp_tee);
                });
                
                return Ok(result);
            }
        };
        
        // Verify results match between primary and complementary TEEs (cross-type verification)
        if primary_result.result != complementary_result.result {
            error!("Results from primary TEE {} (type {}) and complementary TEE {} (type {}) do not match!", 
                   target_tee, tee_type, comp_tee_id, complementary_tee_type);
            
            // This is a serious security/integrity issue - results don't match
            // In a production environment, this could trigger additional verification
            // or escalation to human operators
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Results from TEEs of different types do not match: {} vs {}", tee_type, complementary_tee_type)
            ));
        }
        
        // Results match, combine the attestations from both TEE types
        let mut result = primary_result;
        result.execution_type = "paired_verified".to_string();
        
        // Add attestations from complementary result if not already present
        if let Some(comp_attestations) = &complementary_result.attestations {
            if let Some(mut attestations) = result.attestations.clone() {
                for attestation in comp_attestations {
                    if !attestations.contains(attestation) {
                        attestations.push(attestation.clone());
                    }
                }
                result.attestations = Some(attestations);
            } else {
                result.attestations = Some(comp_attestations.clone());
            }
        }
        
        // Update execution time to include dual execution and verification
        result.execution_time = start_time.elapsed().as_millis() as u64;
        
        // Add metadata about dual execution
        if result.metadata.is_none() {
            result.metadata = Some(HashMap::new());
        }
        
        if let Some(metadata) = &mut result.metadata {
            metadata.insert("dual_execution".to_string(), "true".to_string());
            metadata.insert("primary_tee_type".to_string(), tee_type.clone());
            metadata.insert("secondary_tee_type".to_string(), complementary_tee_type.to_string());
            metadata.insert("cross_type_verified".to_string(), "true".to_string());
        }
        
        // Store in cache with TTL
        self.store_in_cache(&cache_key, &result, Duration::from_secs(3600));
        
        // Mark both connections as healthy
        if let Ok(primary_tee_type_enum) = tee_type.parse::<TeeType>() {
            self.connection_pool.mark_connection_healthy(
                &target_tee, &region_id, &primary_tee_type_enum, result.execution_time as f64);
        }
        
        if let Ok(comp_tee_type_enum) = complementary_tee_type.parse::<TeeType>() {
            self.connection_pool.mark_connection_healthy(
                &comp_tee_id, &region_id, &comp_tee_type_enum, result.execution_time as f64);
        }
        
        info!("Paired execution completed successfully with cross-verification between TEE types: {} and {}", 
              tee_type, complementary_tee_type);
        return Ok(result);
    }
    
    // Fallback case: No complementary TEE available, use single execution result
    let mut result = primary_result;
    result.execution_type = "paired_fallback".to_string();
    
    // Add metadata about fallback to single execution
    if result.metadata.is_none() {
        result.metadata = Some(HashMap::new());
    }
    
    if let Some(metadata) = &mut result.metadata {
        metadata.insert("dual_execution".to_string(), "false".to_string());
        metadata.insert("fallback_reason".to_string(), "no_complementary_tee".to_string());
    }
    
    // Store in cache with TTL
    self.store_in_cache(&cache_key, &result, Duration::from_secs(3600));
    
    warn!("Paired execution completed with fallback to single TEE (no complementary TEE available)");
    Ok(result)
}
