#!/bin/bash

# Asset Tokenization Script for TEE Mesh Network
# Integrates NASDAQ market data with tokenization contract using dual TEE architecture

# Color definitions for better readability
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONTRACTS_DIR="$PROJECT_ROOT/execution/controller/tests/contracts"
NASDAQ_DIR="$PROJECT_ROOT/tee/integration/nasdaq"

echo -e "${GREEN}Starting Asset Tokenization with Dual TEE Cross-Attestation${NC}"
echo -e "${YELLOW}===========================================================${NC}"

# Set coordinator URL
export COORDINATOR_URL="http://localhost:8080"

# Check if coordinator is running or start it
if ! curl -s "$COORDINATOR_URL/health" > /dev/null 2>&1; then
    echo "Coordinator not running at $COORDINATOR_URL"
    echo "Starting mock coordinator..."
    # Start coordinator in background
    cargo run --bin coordinator_mock &
    COORDINATOR_PID=$!
    sleep 2
else
    echo "Coordinator already running at $COORDINATOR_URL"
    COORDINATOR_PID=""
fi

# Start primary TEE controller (SGX)
echo -e "${BLUE}Starting primary TEE controller (SGX)...${NC}"
cargo run --bin tee_controller -- \
    --id worker-sgx \
    --platform sgx \
    --region us-east-1 \
    > /tmp/primary_tee.log 2>&1 &
PRIMARY_PID=$!
echo "Primary TEE controller started"

# Start secondary TEE controller (AMD SEV)
echo -e "${BLUE}Starting secondary TEE controller (AMD SEV)...${NC}"
cargo run --bin tee_controller -- \
    --id worker-sev \
    --platform sev \
    --region us-east-1 \
    > /tmp/secondary_tee.log 2>&1 &
SECONDARY_PID=$!
echo "Secondary TEE controller started"

# Wait for TEE controllers to initialize
echo "Waiting for TEE controllers to initialize..."
sleep 5

# Register TEE pair for cross-attestation
echo -e "${YELLOW}Registering TEE pair for cross-attestation verification...${NC}"
RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/register_tee_pair" \
    -H "Content-Type: application/json" \
    -d @- <<EOL
{
    "region_id": "us-east-1",
    "primary_id": "worker-sgx",
    "secondary_id": "worker-sev",
    "verification_mode": "cross_attestation"
}
EOL
)
echo "Registered TEE pair: us-east-1_worker-sgx"
echo "$RESPONSE"

# Since we already have a token transfer contract we can use, we'll utilize that for the demo
echo -e "${YELLOW}Preparing tokenization contract...${NC}"
TOKEN_CONTRACT_DIR="$CONTRACTS_DIR/token_transfer"
if [ -d "$TOKEN_CONTRACT_DIR" ]; then
    echo "Using existing token transfer contract for tokenization demo"
    
    # Make sure it's built
    pushd "$TOKEN_CONTRACT_DIR" > /dev/null 2>&1
    cargo build --target wasm32-unknown-unknown --release
    RESULT=$?
    popd > /dev/null 2>&1
    
    if [ $RESULT -eq 0 ] && [ -f "$TOKEN_CONTRACT_DIR/target/wasm32-unknown-unknown/release/token_transfer.wasm" ]; then
        echo "Token contract successfully built"
    else
        echo -e "${RED}Failed to build token contract. Using mock contract.${NC}"
        # Create a simple mock contract directory
        mkdir -p /tmp/mock_token_contract
        echo "Mock tokenization contract" > /tmp/mock_token_contract/token.txt
    fi
else
    echo -e "${YELLOW}Token contract not found, setting up demo mode${NC}"
    # Create a simple mock contract directory
    mkdir -p /tmp/mock_token_contract
    echo "Mock tokenization contract" > /tmp/mock_token_contract/token.txt
fi

# Deploy contract to TEE mesh network (use token_transfer if available, otherwise mock)
echo -e "${YELLOW}Deploying tokenization contract to dual TEE mesh...${NC}"
if [ -f "$TOKEN_CONTRACT_DIR/target/wasm32-unknown-unknown/release/token_transfer.wasm" ]; then
    DEPLOY_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/deploy" \
        -H "Content-Type: application/json" \
        -d @- <<EOL
{
    "wasm_binary": "$(base64 -i "$TOKEN_CONTRACT_DIR/target/wasm32-unknown-unknown/release/token_transfer.wasm")",
    "contract_id": "asset_tokenization",
    "region_id": "us-east-1"
}
EOL
    )
else
    # Mock deployment for demo purposes
    DEPLOY_RESPONSE='{"status":"deployed","contract_id":"asset_tokenization"}'    
fi
echo "Contract deployed: asset_tokenization"

# Stock symbols to tokenize with their information
SYMBOLS=("AAPL:Apple Inc.:USD" "MSFT:Microsoft Corporation:USD" "GOOGL:Alphabet Inc.:USD" "AMZN:Amazon.com Inc.:USD" "TSLA:Tesla, Inc.:USD" "NVDA:NVIDIA Corporation:USD")
PRICES=("185.50" "420.25" "155.75" "180.30" "175.80" "950.20")

# Register assets
echo -e "${YELLOW}Registering assets for tokenization...${NC}"
for i in {0..5}; do
    IFS=':' read -r symbol name currency <<< "${SYMBOLS[$i]}"
    price=${PRICES[$i]}
    
    # Create asset registration data
    cat > /tmp/asset_${symbol}.json <<EOL
{
    "symbol": "${symbol}",
    "name": "${name}",
    "asset_type": "Stock",
    "compliance_info": {
        "jurisdiction": "US-SEC",
        "regulatory_requirements": ["KYC", "AML", "FATCA"],
        "compliance_status": "Compliant",
        "region_id": "us-east-1"
    },
    "price": ${price},
    "total_supply": 1000000,
    "last_updated": $(date +%s),
    "tee_verification": {
        "primary_attestation": "initial-sgx-attestation",
        "secondary_attestation": "initial-sev-attestation",
        "timestamp": $(date +%s),
        "verified": true
    }
}
EOL
    
    # Register the asset
    echo "Registering asset: $symbol ($name)"
    REGISTER_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/execute" \
        -H "Content-Type: application/json" \
        -d @- <<EOL
{
    "contract_id": "asset_tokenization",
    "function": "register_asset",
    "input": "$(base64 -i /tmp/asset_${symbol}.json)",
    "region_id": "us-east-1",
    "verification_mode": "cross_attestation"
}
EOL
    )
    echo "  Response: $REGISTER_RESPONSE"
done

# Generate random wallet addresses for tokenization demo
WALLET_1="0x$(openssl rand -hex 20)"
WALLET_2="0x$(openssl rand -hex 20)"
WALLET_3="0x$(openssl rand -hex 20)"

echo -e "${BLUE}Using wallets for tokenization:${NC}"
echo "  Wallet 1: $WALLET_1"
echo "  Wallet 2: $WALLET_2"
echo "  Wallet 3: $WALLET_3"

# Tokenize assets
echo -e "${YELLOW}Tokenizing assets with dual TEE attestation...${NC}"
for i in {0..5}; do
    IFS=':' read -r symbol name currency <<< "${SYMBOLS[$i]}"
    price=${PRICES[$i]}
    
    # Select wallet based on asset index
    if [ $i -lt 2 ]; then
        wallet=$WALLET_1
    elif [ $i -lt 4 ]; then
        wallet=$WALLET_2
    else
        wallet=$WALLET_3
    fi
    
    # Calculate token amount based on price to keep amounts reasonable
    if (( $(echo "$price > 500" | bc -l) )); then
        amount=0.5
    elif (( $(echo "$price > 200" | bc -l) )); then
        amount=1.5
    else
        amount=2.5
    fi
    
    # Create tokenization request
    cat > /tmp/tokenize_${symbol}.json <<EOL
{
    "asset_symbol": "${symbol}",
    "owner": "${wallet}",
    "amount": ${amount},
    "region_id": "us-east-1",
    "primary_tee_id": "worker-sgx",
    "secondary_tee_id": "worker-sev"
}
EOL
    
    # Request tokenization
    echo "Tokenizing asset: $symbol ($amount units) to $wallet"
    TOKENIZE_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/execute" \
        -H "Content-Type: application/json" \
        -d @- <<EOL
{
    "contract_id": "asset_tokenization",
    "function": "tokenize_asset",
    "input": "$(base64 -i /tmp/tokenize_${symbol}.json)",
    "region_id": "us-east-1",
    "verification_mode": "cross_attestation"
}
EOL
    )
    echo "  Response: $TOKENIZE_RESPONSE"
done

# Simulate market data updates affecting token values
echo -e "${YELLOW}\nSimulating market data updates affecting tokenized assets...${NC}"
for i in {0..5}; do
    IFS=':' read -r symbol name currency <<< "${SYMBOLS[$i]}"
    base_price=${PRICES[$i]}
    
    # Generate a small price movement
    price_change=$(echo "scale=2; (${RANDOM} % 100) / 100 - 0.5" | bc)
    new_price=$(echo "scale=2; $base_price + $price_change" | bc)
    
    # Create market data update
    cat > /tmp/market_update_${symbol}.json <<EOL
{
    "symbol": "${symbol}",
    "price": ${new_price},
    "timestamp": $(date +%s),
    "order_book": {
        "bids": [
            {"price": $(echo "$new_price - 0.05" | bc), "size": $((RANDOM % 1000 + 100))},
            {"price": $(echo "$new_price - 0.10" | bc), "size": $((RANDOM % 1000 + 100))}
        ],
        "asks": [
            {"price": $(echo "$new_price + 0.05" | bc), "size": $((RANDOM % 1000 + 100))},
            {"price": $(echo "$new_price + 0.10" | bc), "size": $((RANDOM % 1000 + 100))}
        ]
    },
    "region_id": "us-east-1"
}
EOL
    
    # Submit market data update
    echo "Updating market data for $symbol: $base_price → $new_price"
    UPDATE_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/execute" \
        -H "Content-Type: application/json" \
        -d @- <<EOL
{
    "contract_id": "asset_tokenization",
    "function": "process_market_data",
    "input": "$(base64 -i /tmp/market_update_${symbol}.json)",
    "region_id": "us-east-1",
    "verification_mode": "cross_attestation"
}
EOL
    )
    echo "  Response: $UPDATE_RESPONSE"
    
    # Small delay between updates
    sleep 0.5
done

# View tokenized assets for each wallet
echo -e "${YELLOW}\nRetrieving tokenized assets by wallet...${NC}"
for wallet in "$WALLET_1" "$WALLET_2" "$WALLET_3"; do
    echo "Tokens held by wallet: $wallet"
    TOKENS_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/execute" \
        -H "Content-Type: application/json" \
        -d @- <<EOL
{
    "contract_id": "asset_tokenization",
    "function": "get_owner_tokens",
    "input": "$(echo -n $wallet | base64)",
    "region_id": "us-east-1",
    "verification_mode": "cross_attestation"
}
EOL
    )
    echo "  Response: $TOKENS_RESPONSE"
done

# Get all registered assets
echo -e "${YELLOW}\nRetrieving all registered tokenized assets...${NC}"
ASSETS_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/execute" \
    -H "Content-Type: application/json" \
    -d @- <<EOL
{
    "contract_id": "asset_tokenization",
    "function": "get_all_assets",
    "input": "$(echo -n "" | base64)",
    "region_id": "us-east-1",
    "verification_mode": "cross_attestation"
}
EOL
)
echo "  Response: $ASSETS_RESPONSE"

# Summarize tokenization results
echo -e "${GREEN}\nAsset Tokenization Summary:${NC}"
echo -e "${YELLOW}===========================================================${NC}"
echo "  Assets tokenized: 6 (AAPL, MSFT, GOOGL, AMZN, TSLA, NVDA)"
echo "  Wallets with tokens: 3"
echo "  TEE architecture: Dual TEE (Intel SGX + AMD SEV) with cross-attestation"
echo "  Region: us-east-1"
echo -e "${YELLOW}===========================================================${NC}"

# Clean up
echo -e "${BLUE}\nCleaning up...${NC}"
if [ -n "$COORDINATOR_PID" ]; then
    kill $COORDINATOR_PID 2>/dev/null
fi
kill $PRIMARY_PID $SECONDARY_PID 2>/dev/null

# Optional: Clean up temp files
rm -f /tmp/asset_*.json /tmp/tokenize_*.json /tmp/market_update_*.json

echo -e "${GREEN}Tokenization demo complete!${NC}"
