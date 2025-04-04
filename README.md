# Titan - High-Performance Bitcoin Indexer

Titan is a next-gen Bitcoin indexer by Saturn, designed for real-time blockchain access, optimized queries, and seamless Runes protocol integration. It delivers superior performance with minimal resource consumption.

## Key Features

- Optimized Performance – Fast, efficient queries tailored for dApps and validator nodes.
- Real-Time Blockchain Access – Instant data retrieval for responsive applications.
- Scalable & Lightweight – High throughput with minimal computational overhead.
- Advanced Runes Integration – Native support for Runes tokens, instant token data, and secure transaction validation.
- Mempool-Level Indexing – Differentiates finalized transactions from those at risk of front-running.

## Requirements

- Bitcoin Node 27.0
- Rust v1.56+ (using the 2021 edition)
- Cargo v1.85.1

## How to run it
```bash
cargo run -p titan -- --bitcoin-rpc-url http://localhost:<PORT> --bitcoin-rpc-username <USERNAME> --bitcoin-rpc-password <PASSWORD> --chain regtest --index-addresses --index-bitcoin-transactions --enable-tcp-subscriptions --data-dir ~/titan-indexer
```

## How to build it
```bash
cargo build --release
```

## Client SDKs

Titan provides official client SDKs for easy integration:

- Rust SDK – [Titan Client for Rust](./client/README.md)

- TypeScript SDK –  [Titan Client for TypeScript](./ts-sdk/README.md)

Each SDK has its own README with detailed installation and usage instructions.