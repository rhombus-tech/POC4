#!/usr/bin/env python3
"""
TEE Mesh Network Asset Tokenization Demo
Demonstrates tokenization of market assets through dual TEE architecture with cross-attestation
"""

import json
import time
import random
import requests
import subprocess
import base64
import hashlib
import os
from typing import Dict, List, Optional, Tuple, Union

# Configuration
COORDINATOR_URL = "http://localhost:8080"
REGION_ID = "us-east-1"
PRIMARY_TEE = "worker-sgx"
SECONDARY_TEE = "worker-sev"

# Market data for demonstration
MARKET_SYMBOLS = [
    {"symbol": "AAPL", "name": "Apple Inc.", "price": 185.50},
    {"symbol": "MSFT", "name": "Microsoft Corporation", "price": 420.25},
    {"symbol": "GOOGL", "name": "Alphabet Inc.", "price": 155.75},
    {"symbol": "AMZN", "name": "Amazon.com Inc.", "price": 180.30},
    {"symbol": "TSLA", "name": "Tesla, Inc.", "price": 175.80},
    {"symbol": "NVDA", "name": "NVIDIA Corporation", "price": 950.20}
]

# Color formatting for terminal output
class Colors:
    GREEN = '\033[0;32m'
    BLUE = '\033[0;34m'
    YELLOW = '\033[1;33m'
    RED = '\033[0;31m'
    NC = '\033[0m'  # No Color

def print_header(text: str) -> None:
    """Print a formatted header"""
    print(f"\n{Colors.YELLOW}{'=' * 60}{Colors.NC}")
    print(f"{Colors.GREEN}{text}{Colors.NC}")
    print(f"{Colors.YELLOW}{'=' * 60}{Colors.NC}")

def print_info(text: str) -> None:
    """Print formatted info text"""
    print(f"{Colors.BLUE}{text}{Colors.NC}")

def print_success(text: str) -> None:
    """Print formatted success text"""
    print(f"{Colors.GREEN}{text}{Colors.NC}")

def print_warning(text: str) -> None:
    """Print formatted warning text"""
    print(f"{Colors.YELLOW}{text}{Colors.NC}")

def print_error(text: str) -> None:
    """Print formatted error text"""
    print(f"{Colors.RED}{text}{Colors.NC}")

def ensure_coordinator_running() -> Tuple[bool, Optional[subprocess.Popen]]:
    """Ensure the coordinator is running, start if needed"""
    try:
        response = requests.get(f"{COORDINATOR_URL}/health", timeout=2)
        if response.status_code == 200:
            print_info("Coordinator already running")
            return True, None
    except requests.RequestException:
        pass
    
    print_info("Starting coordinator...")
    process = subprocess.Popen(
        ["cargo", "run", "--bin", "coordinator_mock"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    )
    
    # Wait for coordinator to start
    for _ in range(10):
        try:
            response = requests.get(f"{COORDINATOR_URL}/health", timeout=1)
            if response.status_code == 200:
                print_success("Coordinator started successfully")
                return True, process
        except requests.RequestException:
            pass
        time.sleep(1)
    
    print_error("Failed to start coordinator")
    return False, process

def start_tee_controllers() -> Tuple[subprocess.Popen, subprocess.Popen]:
    """Start primary and secondary TEE controllers"""
    print_info("Starting primary TEE controller (SGX)...")
    primary = subprocess.Popen(
        ["cargo", "run", "--bin", "tee_controller", "--", "--id", PRIMARY_TEE, "--platform", "sgx", "--region", REGION_ID],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    )
    
    print_info("Starting secondary TEE controller (AMD SEV)...")
    secondary = subprocess.Popen(
        ["cargo", "run", "--bin", "tee_controller", "--", "--id", SECONDARY_TEE, "--platform", "sev", "--region", REGION_ID],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    )
    
    # Give them time to initialize
    time.sleep(5)
    return primary, secondary

def register_tee_pair() -> bool:
    """Register TEE pair for cross-attestation"""
    print_info("Registering TEE pair for cross-attestation verification...")
    try:
        response = requests.post(
            f"{COORDINATOR_URL}/register_tee_pair",
            json={
                "region_id": REGION_ID,
                "primary_id": PRIMARY_TEE,
                "secondary_id": SECONDARY_TEE,
                "verification_mode": "cross_attestation"
            },
            timeout=5
        )
        
        if response.status_code == 200:
            print_success(f"TEE pair registered: {REGION_ID}_{PRIMARY_TEE}")
            return True
        else:
            print_error(f"Failed to register TEE pair: {response.text}")
            return False
    except requests.RequestException as e:
        print_error(f"Error registering TEE pair: {str(e)}")
        return False

class AssetTokenizer:
    """Handles tokenization of assets through the TEE mesh network"""
    
    def __init__(self):
        self.tokens = {}  # token_id -> token_data
        self.wallet_tokens = {}  # wallet -> [token_ids]
        self.assets = {}  # symbol -> asset_data
        self.transactions = []  # List of transactions
    
    def generate_id(self, prefix: str) -> str:
        """Generate a unique ID with the given prefix"""
        timestamp = int(time.time())
        random_bytes = os.urandom(8)
        hash_input = f"{prefix}:{timestamp}:{random_bytes.hex()}"
        hash_output = hashlib.sha256(hash_input.encode()).hexdigest()
        return f"{prefix}-{hash_output[:8]}"
    
    def register_asset(self, symbol: str, name: str, price: float) -> Dict:
        """Register an asset for tokenization with dual TEE attestation"""
        print_info(f"Registering asset: {symbol} ({name})")
        
        # Create attestation proofs from both TEEs
        primary_attestation = self.generate_attestation(PRIMARY_TEE, symbol)
        secondary_attestation = self.generate_attestation(SECONDARY_TEE, symbol)
        
        # Create asset record with attestations
        asset = {
            "symbol": symbol,
            "name": name,
            "asset_type": "Stock",
            "compliance_info": {
                "jurisdiction": "US-SEC",
                "regulatory_requirements": ["KYC", "AML", "FATCA"],
                "compliance_status": "Compliant",
                "region_id": REGION_ID
            },
            "price": price,
            "total_supply": 1000000,
            "last_updated": int(time.time()),
            "tee_verification": {
                "primary_attestation": primary_attestation,
                "secondary_attestation": secondary_attestation,
                "timestamp": int(time.time()),
                "verified": self.verify_cross_attestation(primary_attestation, secondary_attestation)
            }
        }
        
        # Store the asset
        self.assets[symbol] = asset
        
        return {
            "success": True,
            "message": f"Asset {symbol} registered successfully",
            "data": asset
        }
    
    def tokenize_asset(self, symbol: str, owner: str, amount: float) -> Dict:
        """Tokenize an asset with dual TEE attestation"""
        print_info(f"Tokenizing {amount} units of {symbol} for {owner}")
        
        if symbol not in self.assets:
            return {"success": False, "message": f"Asset {symbol} not found"}
        
        # Generate tokenization proofs from both TEEs
        primary_attestation = self.generate_attestation(PRIMARY_TEE, f"{symbol}:{owner}:{amount}")
        secondary_attestation = self.generate_attestation(SECONDARY_TEE, f"{symbol}:{owner}:{amount}")
        
        # Verify cross-attestation
        cross_verified = self.verify_cross_attestation(primary_attestation, secondary_attestation)
        if not cross_verified:
            return {"success": False, "message": "Cross-attestation verification failed"}
        
        # Create tokenization proof
        proof = {
            "primary_attestation": primary_attestation,
            "secondary_attestation": secondary_attestation,
            "accumulator_value": self.generate_id("acc"),
            "region_id": REGION_ID
        }
        
        # Generate token
        token_id = self.generate_id("tkn")
        token = {
            "id": token_id,
            "asset_symbol": symbol,
            "owner": owner,
            "amount": amount,
            "issued_at": int(time.time()),
            "last_transfer": None,
            "proof": proof
        }
        
        # Store token
        self.tokens[token_id] = token
        if owner not in self.wallet_tokens:
            self.wallet_tokens[owner] = []
        self.wallet_tokens[owner].append(token_id)
        
        # Record transaction
        tx_id = self.generate_id("tx")
        tx_hash = self.generate_transaction_hash(symbol, owner, amount)
        transaction = {
            "id": tx_id,
            "tx_type": "Tokenize",
            "sender": None,
            "receiver": owner,
            "asset_symbol": symbol,
            "amount": amount,
            "timestamp": int(time.time()),
            "verified": True,
            "hash": tx_hash
        }
        self.transactions.append(transaction)
        
        return {
            "success": True,
            "message": f"Asset {symbol} tokenized successfully",
            "data": token
        }
    
    def update_market_data(self, symbol: str, price: float) -> Dict:
        """Update market data for a tokenized asset"""
        print_info(f"Updating market data for {symbol}: {price}")
        
        if symbol not in self.assets:
            return {"success": False, "message": f"Asset {symbol} not found"}
        
        # Update asset price with attestations
        primary_attestation = self.generate_attestation(PRIMARY_TEE, f"{symbol}:{price}")
        secondary_attestation = self.generate_attestation(SECONDARY_TEE, f"{symbol}:{price}")
        
        # Verify cross-attestation
        cross_verified = self.verify_cross_attestation(primary_attestation, secondary_attestation)
        
        # Update the asset
        self.assets[symbol]["price"] = price
        self.assets[symbol]["last_updated"] = int(time.time())
        self.assets[symbol]["tee_verification"] = {
            "primary_attestation": primary_attestation,
            "secondary_attestation": secondary_attestation,
            "timestamp": int(time.time()),
            "verified": cross_verified
        }
        
        # Record update transaction
        tx_id = self.generate_id("tx")
        tx_hash = self.generate_transaction_hash(symbol, "UPDATE", price)
        transaction = {
            "id": tx_id,
            "tx_type": "Update",
            "sender": None,
            "receiver": None,
            "asset_symbol": symbol,
            "amount": 0.0,
            "timestamp": int(time.time()),
            "verified": cross_verified,
            "hash": tx_hash
        }
        self.transactions.append(transaction)
        
        return {
            "success": True,
            "message": f"Market data for {symbol} updated successfully",
            "data": self.assets[symbol]
        }
    
    def get_wallet_tokens(self, wallet: str) -> Dict:
        """Get all tokens owned by a wallet"""
        print_info(f"Retrieving tokens for wallet: {wallet}")
        
        if wallet not in self.wallet_tokens:
            return {
                "success": True,
                "message": "No tokens found for wallet",
                "data": []
            }
        
        tokens = []
        for token_id in self.wallet_tokens[wallet]:
            if token_id in self.tokens:
                tokens.append(self.tokens[token_id])
        
        return {
            "success": True,
            "message": f"Retrieved {len(tokens)} tokens",
            "data": tokens
        }
    
    def get_all_assets(self) -> Dict:
        """Get all registered assets"""
        print_info("Retrieving all registered assets")
        
        return {
            "success": True,
            "message": f"Retrieved {len(self.assets)} assets",
            "data": list(self.assets.values())
        }
    
    def generate_attestation(self, tee_id: str, data: str) -> str:
        """Generate a simulated attestation from the specified TEE"""
        attestation_input = f"{tee_id}:{data}:{time.time()}"
        attestation_hash = hashlib.sha256(attestation_input.encode()).hexdigest()
        return f"{tee_id[:3]}-att-{attestation_hash[:16]}"
    
    def verify_cross_attestation(self, primary_att: str, secondary_att: str) -> bool:
        """Verify cross-attestation between primary and secondary TEEs"""
        # In a real implementation, this would verify the attestations cryptographically
        # For the demo, we'll always return True after a brief delay to simulate verification
        time.sleep(0.05)  # Simulate verification latency (50ms)
        return True
    
    def generate_transaction_hash(self, symbol: str, party: str, amount: float) -> str:
        """Generate a transaction hash"""
        tx_input = f"{symbol}:{party}:{amount}:{time.time()}"
        return hashlib.sha256(tx_input.encode()).hexdigest()
    
    def get_asset_portfolio_value(self, wallet: str) -> Dict:
        """Calculate total portfolio value for a wallet"""
        if wallet not in self.wallet_tokens:
            return {
                "success": True,
                "message": "No tokens found for wallet",
                "data": {"total_value": 0.0, "assets": []}
            }
        
        total_value = 0.0
        asset_values = []
        
        for token_id in self.wallet_tokens[wallet]:
            if token_id in self.tokens:
                token = self.tokens[token_id]
                symbol = token["asset_symbol"]
                amount = token["amount"]
                
                if symbol in self.assets:
                    price = self.assets[symbol]["price"]
                    value = price * amount
                    total_value += value
                    
                    asset_values.append({
                        "symbol": symbol,
                        "amount": amount,
                        "price": price,
                        "value": value,
                        "token_id": token_id
                    })
        
        return {
            "success": True,
            "message": f"Portfolio value calculated with dual TEE verification",
            "data": {
                "total_value": total_value,
                "assets": asset_values
            }
        }

def run_tokenization_demo():
    """Run the full tokenization demo with dual TEE mesh"""
    print_header("TEE Mesh Network Asset Tokenization Demo")
    print_info("Demonstrating tokenization of market assets with dual TEE cross-attestation")
    
    # Setup environment
    coordinator_running, coordinator_process = ensure_coordinator_running()
    if not coordinator_running:
        return
    
    # Start TEE controllers
    primary_process, secondary_process = start_tee_controllers()
    
    # Register TEE pair
    if not register_tee_pair():
        print_error("Cannot continue without TEE pair registration")
        return
    
    # Create tokenizer
    tokenizer = AssetTokenizer()
    
    # Register assets
    for asset in MARKET_SYMBOLS:
        result = tokenizer.register_asset(
            asset["symbol"], 
            asset["name"], 
            asset["price"]
        )
        print(f"  Result: {result['success']} - {result['message']}")
    
    # Generate wallets
    wallets = [
        "0x" + os.urandom(20).hex()[:40],
        "0x" + os.urandom(20).hex()[:40],
        "0x" + os.urandom(20).hex()[:40]
    ]
    
    print_info("\nWallets for tokenization:")
    for i, wallet in enumerate(wallets, 1):
        print(f"  Wallet {i}: {wallet}")
    
    # Tokenize assets
    print_header("Tokenizing Assets with Dual TEE Attestation")
    
    for i, asset in enumerate(MARKET_SYMBOLS):
        symbol = asset["symbol"]
        price = asset["price"]
        
        # Select wallet based on asset index
        wallet_index = min(i // 2, 2)
        wallet = wallets[wallet_index]
        
        # Calculate amount based on price
        if price > 500:
            amount = 0.5
        elif price > 200:
            amount = 1.5
        else:
            amount = 2.5
            
        # Tokenize
        result = tokenizer.tokenize_asset(symbol, wallet, amount)
        print(f"  Result: {result['success']} - {result['message']}")
    
    # Simulate market data updates
    print_header("Simulating Market Data Updates with Cross-Attestation")
    
    for asset in MARKET_SYMBOLS:
        symbol = asset["symbol"]
        base_price = asset["price"]
        
        # Generate price movement
        price_change = (random.random() * 2 - 1) * (base_price * 0.01)  # +/- 1% movement
        new_price = round(base_price + price_change, 2)
        
        # Update market data
        result = tokenizer.update_market_data(symbol, new_price)
        print(f"  Updated {symbol}: {base_price} → {new_price}")
    
    # Get wallet portfolios
    print_header("Tokenized Asset Portfolios (Dual TEE Verified)")
    
    for i, wallet in enumerate(wallets, 1):
        print(f"\nWallet {i}: {wallet}")
        
        # Get tokens
        tokens_result = tokenizer.get_wallet_tokens(wallet)
        if tokens_result["success"] and tokens_result["data"]:
            token_count = len(tokens_result["data"])
            print(f"  Tokens: {token_count}")
            
            # Get portfolio value
            portfolio = tokenizer.get_asset_portfolio_value(wallet)
            if portfolio["success"]:
                print(f"  Portfolio value: ${portfolio['data']['total_value']:.2f}")
                print("  Asset breakdown:")
                for asset in portfolio["data"]["assets"]:
                    print(f"    {asset['symbol']}: {asset['amount']} units at ${asset['price']} = ${asset['value']:.2f}")
        else:
            print("  No tokens found")
    
    # Show all registered assets
    print_header("All Registered Tokenized Assets")
    assets_result = tokenizer.get_all_assets()
    for asset in assets_result["data"]:
        print(f"  {asset['symbol']} ({asset['name']}): ${asset['price']}")
        print(f"    Last updated: {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(asset['last_updated']))}")
        print(f"    TEE verification: {'Verified' if asset['tee_verification']['verified'] else 'Failed'}")
        print(f"    Primary attestation: {asset['tee_verification']['primary_attestation']}")
        print(f"    Secondary attestation: {asset['tee_verification']['secondary_attestation']}")
    
    # Show verification summary
    print_header("TEE Mesh Verification Summary")
    verified_count = sum(1 for asset in assets_result["data"] if asset["tee_verification"]["verified"])
    print(f"  Assets with successful cross-attestation: {verified_count}/{len(assets_result['data'])}")
    print(f"  Dual TEE architecture: Intel SGX + AMD SEV")
    print(f"  Regional mesh network: {REGION_ID}")
    print(f"  Cross-attestation verification: Enabled")
    
    # Cleanup
    print_info("\nCleaning up...")
    try:
        primary_process.terminate()
        secondary_process.terminate()
        if coordinator_process:
            coordinator_process.terminate()
    except:
        pass
    
    print_success("Tokenization demo completed successfully!")

if __name__ == "__main__":
    run_tokenization_demo()
