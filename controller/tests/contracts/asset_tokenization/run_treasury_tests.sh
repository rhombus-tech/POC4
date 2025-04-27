#!/bin/bash
# Treasury Tokenization Contract Test Runner
# Runs all wasmlanche tests for the dual TEE attestation system

set -e

YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=============================================================${NC}"
echo -e "${BLUE}   US Treasury Tokenization Contract - Wasmlanche Tests${NC}"
echo -e "${BLUE}   Dual TEE Architecture with Cryptographic Accumulator${NC}"
echo -e "${BLUE}=============================================================${NC}"

# Ensure we're in the right directory
cd "$(dirname "$0")"
CONTRACT_DIR="$(pwd)"
PROJECT_ROOT="$(cd ../../../../.. && pwd)"

echo -e "\nContract directory: $CONTRACT_DIR"
echo -e "Project root: $PROJECT_ROOT"

# Check wasmlanche path
WASMLANCHE_PATH="$PROJECT_ROOT/hyper/x/contracts/wasmlanche"
if [ ! -d "$WASMLANCHE_PATH" ]; then
    echo -e "\n${RED}Error: Wasmlanche path not found at $WASMLANCHE_PATH${NC}"
    echo -e "Please adjust the path in Cargo.toml to point to your wasmlanche installation"
    exit 1
fi

# Update contract dependencies
echo -e "\n${YELLOW}Updating dependencies...${NC}"
cargo update

# Build the contract with wasmlanche support
echo -e "\n${YELLOW}Building Treasury contract for wasmlanche...${NC}"
RUSTFLAGS="-C link-arg=-zstack-size=65536" cargo build --target wasm32-unknown-unknown --features "simulator" || {
    echo -e "\n${RED}Build failed. Attempting to fix common issues...${NC}"
    
    # Check wasmlanche dependency in Cargo.toml
    echo -e "\n${YELLOW}Checking wasmlanche path in Cargo.toml...${NC}"
    EXISTING_PATH=$(grep -o 'path = "[^"]*"' Cargo.toml | head -1 | cut -d'"' -f2)
    echo "Current path: $EXISTING_PATH"
    
    # Suggest manual fixes
    echo -e "\n${YELLOW}Please verify:${NC}"
    echo -e "1. The wasmlanche path in Cargo.toml is correct"
    echo -e "2. All required dependencies are included"
    echo -e "3. The simulator feature is properly defined"
    exit 1
}

# Run the contract tests
echo -e "\n${YELLOW}Running Treasury tokenization contract tests...${NC}"
cargo test --features "simulator" -- --nocapture

# Collect and display benchmark results
echo -e "\n${YELLOW}Generating performance report...${NC}"
echo -e "${BLUE}=============================================================${NC}"
echo -e "${BLUE}   Performance Benchmark Results${NC}"
echo -e "${BLUE}=============================================================${NC}"

TEST_OUTPUT=$(cargo test --features "simulator" test_verification_performance -- --nocapture 2>&1)

# Extract verification metrics from test output
TOTAL_VERIFICATIONS=$(echo "$TEST_OUTPUT" | grep "Total verifications:" | awk '{print $3}')
AVG_TIME=$(echo "$TEST_OUTPUT" | grep "Average verification time:" | awk '{print $4}')
FASTEST=$(echo "$TEST_OUTPUT" | grep "Fastest verification:" | awk '{print $3}')
SLOWEST=$(echo "$TEST_OUTPUT" | grep "Slowest verification:" | awk '{print $3}')

# Display results in a table
echo -e "\nDual TEE Cross-Attestation Performance:"
echo -e "┌───────────────────────────┬─────────────┐"
echo -e "│ Metric                    │ Value       │"
echo -e "├───────────────────────────┼─────────────┤"
echo -e "│ Total Verifications       │ $TOTAL_VERIFICATIONS          │"
echo -e "│ Average Verification Time │ $AVG_TIME ms      │"
echo -e "│ Fastest Verification      │ $FASTEST ms        │"
echo -e "│ Slowest Verification      │ $SLOWEST ms        │"
echo -e "└───────────────────────────┴─────────────┘"

# Calculate theoretical TPS based on average verification time
if [[ ! -z "$AVG_TIME" ]]; then
    TPS=$(echo "scale=2; 1000 / $AVG_TIME" | bc)
    PROJECTED_TPS=$(echo "scale=0; $TPS * 50" | bc)
    
    echo -e "\nTheoretical Performance Projections:"
    echo -e "┌───────────────────────────┬─────────────┐"
    echo -e "│ Metric                    │ Value       │"
    echo -e "├───────────────────────────┼─────────────┤"
    echo -e "│ Current TPS (single node) │ $TPS         │"
    echo -e "│ Projected TPS (50 nodes)  │ ~$PROJECTED_TPS        │"
    echo -e "└───────────────────────────┴─────────────┘"
    
    # Check if we're meeting performance targets
    if (( $(echo "$AVG_TIME < 100" | bc -l) )); then
        echo -e "\n${GREEN}✓ Meeting sub-100ms verification target${NC}"
    else
        echo -e "\n${RED}✗ Not meeting sub-100ms verification target${NC}"
    fi
    
    if (( $(echo "$PROJECTED_TPS >= 50000" | bc -l) )); then
        echo -e "${GREEN}✓ Meeting 50,000+ TPS target at scale${NC}"
    else
        echo -e "${RED}✗ Not meeting 50,000+ TPS target at scale${NC}"
    fi
fi

echo -e "\n${BLUE}=============================================================${NC}"
echo -e "${BLUE}   Test Execution Complete${NC}"
echo -e "${BLUE}=============================================================${NC}"

# Exit with success
exit 0
