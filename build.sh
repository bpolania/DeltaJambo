#!/bin/bash
set -e

echo "Building NEAR smart contracts..."

CONTRACTS=(
    "long-token"
    "short-token"
    "fee-collector"
    "oracle-router"
    "forward-market"
    "forward-factory"
)

for contract in "${CONTRACTS[@]}"; do
    echo "Building $contract..."
    cd contracts/$contract
    cargo build --target wasm32-unknown-unknown --release
    cd ../..
done

echo "Creating res directory for WASM files..."
mkdir -p res

echo "Copying WASM files to res directory..."
for contract in "${CONTRACTS[@]}"; do
    cp target/wasm32-unknown-unknown/release/${contract//-/_}.wasm res/$contract.wasm
done

echo "Build complete! WASM files are in the res/ directory"
ls -la res/