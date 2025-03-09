#!/bin/bash

# Multi-TEE Demo Script
# This script demonstrates running multiple TEE pairs with the coordinator

# Set coordinator URL
export COORDINATOR_URL="http://localhost:8080"

# Check if coordinator is running or start it
if ! curl -s "$COORDINATOR_URL/health" > /dev/null; then
    echo "Coordinator not running at $COORDINATOR_URL"
    echo "Starting mock coordinator..."
    # Mock coordinator for demo purposes
    cargo run --bin coordinator_mock &
    COORDINATOR_PID=$!
    sleep 2
fi

# Terminal colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting Multi-TEE Demo${NC}"
echo -e "${YELLOW}============================${NC}"
echo

echo -e "${BLUE}Starting Secondary TEE Controller${NC}"
# Start secondary controller
RUN_MODE=secondary WORKER_ID=secondary-tee cargo run --bin multi_tee_integration &
SECONDARY_PID=$!

# Wait for secondary to start up
sleep 3

echo -e "${BLUE}Starting Primary TEE Controller${NC}"
# Start primary controller
RUN_MODE=primary WORKER_ID=primary-tee cargo run --bin multi_tee_integration &
PRIMARY_PID=$!

# Wait for primary to finish
wait $PRIMARY_PID

echo
echo -e "${YELLOW}============================${NC}"
echo -e "${GREEN}Multi-TEE Demo Complete${NC}"

# Cleanup
kill $SECONDARY_PID 2>/dev/null
if [ -n "$COORDINATOR_PID" ]; then
    kill $COORDINATOR_PID 2>/dev/null
fi

echo "Demo processes terminated"
