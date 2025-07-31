# Tron Vanity Address Generator

![Rust](https://img.shields.io/badge/Rust-1.70+-blue.svg)
![License](https://img.shields.io/badge/License-MIT-green.svg)

A high-performance Rust tool for generating custom Tron addresses with specific patterns or suffixes.

## Features ✨

- 🎯 **Custom Pattern Matching** - Find addresses with specific suffixes (6666, 8888) or complex regex patterns
- 🔒 **Secure Generation** - Cryptographically secure private key generation using secp256k1
- ⚡ **Multi-threaded** - Leverage all CPU cores for faster generation
- 📊 **Progress Tracking** - Real-time statistics on attempts and estimated time
- 💾 **Result Saving** - Automatically save found addresses to file

## Installation

### Prerequisites
- Rust 1.87+ (install via [rustup](https://rustup.rs/))
- Cargo (comes with Rust)

### Clone & Build
```bash
git clone https://github.com/liqingmubai/tron_address_generator.git
cd tron_address_generator
cargo build --release