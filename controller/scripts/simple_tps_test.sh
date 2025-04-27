#!/bin/bash

# Simple TPS test for TEE Mesh Network
# This script measures raw transactions per second through the dual TEE architecture

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Configuration
COORDINATOR_URL="http://localhost:8080"
NUM_TRANSACTIONS=100
PAYLOAD_SIZE=1024  # Size in bytes
BATCH_SIZE=10
TIMEOUT=30  # Max seconds to wait for completion

echo -e "${YELLOW}===========================================================${NC}"
echo -e "${GREEN}TEE Mesh Network TPS Test${NC}"
echo -e "${BLUE}Testing dual TEE with cross-attestation verification${NC}"
echo -e "${YELLOW}===========================================================${NC}"

# Create test data (market data for simplicity)
echo "Generating test payload ($PAYLOAD_SIZE bytes)..."
PAYLOAD=$(head -c $PAYLOAD_SIZE /dev/urandom | base64)
echo "{\"symbol\":\"TEST\",\"price\":100.00,\"volume\":1000,\"data\":\"$PAYLOAD\"}" > /tmp/test_payload.json

# Ensure coordinator is running
if ! curl -s "$COORDINATOR_URL/health" > /dev/null 2>&1; then
    echo -e "${YELLOW}Starting coordinator...${NC}"
    cargo run --bin coordinator_mock &
    COORDINATOR_PID=$!
    sleep 3
else
    echo "Coordinator already running"
    COORDINATOR_PID=""
fi

# Start TEE controllers (both SGX and SEV)
echo -e "${YELLOW}Starting TEE controllers...${NC}"
cargo run --bin tee_controller -- --id worker-sgx --platform sgx --region us-east-1 > /tmp/tee_sgx.log 2>&1 &
SGX_PID=$!
sleep 2
cargo run --bin tee_controller -- --id worker-sev --platform sev --region us-east-1 > /tmp/tee_sev.log 2>&1 &
SEV_PID=$!
sleep 5

# Register TEE pair
echo "Registering TEE pair for cross-attestation..."
curl -s -X POST "$COORDINATOR_URL/register_tee_pair" \
    -H "Content-Type: application/json" \
    -d "{\"region_id\":\"us-east-1\",\"primary_id\":\"worker-sgx\",\"secondary_id\":\"worker-sev\",\"verification_mode\":\"cross_attestation\"}" \
    > /dev/null

# Run TPS test
echo -e "${YELLOW}\nRunning TPS test with $NUM_TRANSACTIONS transactions...${NC}"
echo "- Cross-attestation: Enabled (Intel SGX + AMD SEV)"
echo "- Batch size: $BATCH_SIZE"

# Time tracking
START_TIME=$(date +%s.%N)
SUCCESSFUL=0
FAILED=0

# Submit transactions in batches
for ((i=1; i<=$NUM_TRANSACTIONS; i+=$BATCH_SIZE)); do
    BATCH_END=$((i+BATCH_SIZE-1))
    if [ $BATCH_END -gt $NUM_TRANSACTIONS ]; then
        BATCH_END=$NUM_TRANSACTIONS
    fi
    
    # Process batch
    BATCH_START=$(date +%s.%N)
    for ((j=i; j<=$BATCH_END; j++)); do
        # Slightly vary the payload for each transaction
        echo "{\"symbol\":\"TEST$j\",\"price\":100.$j,\"volume\":1000,\"data\":\"$PAYLOAD\"}" > /tmp/test_payload_$j.json
        
        # Submit transaction to the TEE mesh
        RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/tasks" \
            -H "Content-Type: application/json" \
            -d "{\"payload\":{\"input\":\"$(base64 < /tmp/test_payload_$j.json)\",\"params\":{\"id_to\":\"market_data_consumer\",\"function_call\":\"process_itch_data\",\"detailed_proof\":true},\"region_id\":\"us-east-1\",\"verification_mode\":\"cross_attestation\",\"target_tee\":null,\"allow_fallback\":true}}" \
            2>/dev/null)
        
        TASK_ID=$(echo "$RESPONSE" | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)
        if [ -n "$TASK_ID" ]; then
            echo "$TASK_ID" >> /tmp/task_ids.txt
            SUCCESSFUL=$((SUCCESSFUL+1))
        else
            FAILED=$((FAILED+1))
        fi
    done
    
    # Log progress
    CURRENT_TIME=$(date +%s.%N)
    ELAPSED=$(echo "$CURRENT_TIME - $START_TIME" | bc)
    CURRENT_TPS=$(echo "scale=2; $SUCCESSFUL / $ELAPSED" | bc)
    echo "Progress: $SUCCESSFUL/$NUM_TRANSACTIONS completed, Current TPS: $CURRENT_TPS"
done

# Calculate results
END_TIME=$(date +%s.%N)
TOTAL_TIME=$(echo "$END_TIME - $START_TIME" | bc)
TPS=$(echo "scale=2; $SUCCESSFUL / $TOTAL_TIME" | bc)
AVG_LATENCY=$(echo "scale=2; 1000 * $TOTAL_TIME / $SUCCESSFUL" | bc)

# Additional latency measurement (for a small sample)
echo -e "\n${YELLOW}Measuring processing latency...${NC}"
LATENCY_SAMPLES=5
TOTAL_LATENCY=0

for i in $(seq 1 $LATENCY_SAMPLES); do
    # Generate unique payload
    echo "{\"symbol\":\"LATENCY$i\",\"price\":100.00,\"volume\":1000,\"timestamp\":$(date +%s)}" > /tmp/latency_test.json
    
    # Submit and measure
    START=$(date +%s.%N)
    RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/tasks" \
        -H "Content-Type: application/json" \
        -d "{\"payload\":{\"input\":\"$(base64 < /tmp/latency_test.json)\",\"params\":{\"id_to\":\"market_data_consumer\",\"function_call\":\"process_itch_data\",\"detailed_proof\":true},\"region_id\":\"us-east-1\",\"verification_mode\":\"cross_attestation\"}}" \
        2>/dev/null)
    
    TASK_ID=$(echo "$RESPONSE" | grep -o '"task_id":"[^"]*"' | cut -d'"' -f4)
    if [ -n "$TASK_ID" ]; then
        # Poll until complete
        for ((t=0; t<100; t++)); do
            STATUS=$(curl -s "$COORDINATOR_URL/tasks/$TASK_ID/status" 2>/dev/null | grep -o '"status":"[^"]*"' | cut -d'"' -f4)
            if [ "$STATUS" = "completed" ]; then
                break
            fi
            sleep 0.01
        done
        END=$(date +%s.%N)
        LATENCY=$(echo "($END - $START) * 1000" | bc)
        echo "  Sample $i: $LATENCY ms"
        TOTAL_LATENCY=$(echo "$TOTAL_LATENCY + $LATENCY" | bc)
    fi
done

# Calculate average latency
if [ "$LATENCY_SAMPLES" -gt 0 ]; then
    AVG_SAMPLE_LATENCY=$(echo "scale=2; $TOTAL_LATENCY / $LATENCY_SAMPLES" | bc)
    echo "  Average latency: $AVG_SAMPLE_LATENCY ms"
fi

# Print results
echo -e "\n${GREEN}TEE Mesh Network Performance Results:${NC}"
echo -e "${YELLOW}===========================================================${NC}"
echo "  Throughput: $TPS transactions per second"
echo "  Successful transactions: $SUCCESSFUL"
echo "  Failed transactions: $FAILED"
echo "  Total time: $TOTAL_TIME seconds"
echo "  Average latency: $AVG_LATENCY ms"
echo "  Architecture: Dual TEE with cross-attestation verification"
echo "  Region: us-east-1"
echo -e "${YELLOW}===========================================================${NC}"

# Performance assessment
echo -e "\n${BLUE}Performance Assessment:${NC}"
if (( $(echo "$TPS >= 5500" | bc -l) )); then
    echo -e "  ${GREEN}✓ Meeting target of 5,500+ TPS${NC}"
else
    echo -e "  ${RED}✗ Below target of 5,500+ TPS${NC}"
fi

if (( $(echo "$AVG_LATENCY < 100" | bc -l) )); then
    echo -e "  ${GREEN}✓ Meeting target of sub-100ms latency${NC}"
else
    echo -e "  ${RED}✗ Above target of sub-100ms latency${NC}"
fi

# Clean up
echo -e "\n${BLUE}Cleaning up...${NC}"
kill $SGX_PID $SEV_PID 2>/dev/null
if [ -n "$COORDINATOR_PID" ]; then 
    kill $COORDINATOR_PID 2>/dev/null
fi
rm -f /tmp/test_payload*.json /tmp/task_ids.txt /tmp/latency_test.json

echo -e "${GREEN}Test complete!${NC}"
