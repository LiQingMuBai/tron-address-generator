use secp256k1::{Secp256k1, SecretKey};
use rand::rngs::OsRng;
use sha3::{Digest, Keccak256};
use std::time::Instant;
use bs58;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Pattern to search for (supports ^ for start, $ for end)
    #[arg(short, long)]
    pattern: String,

    /// Case-sensitive matching
    #[arg(short, long, default_value_t = false)]
    case_sensitive: bool,

    /// Maximum generation attempts
    #[arg(short, long, default_value_t = 10_000_000)]
    max_attempts: u64,

    /// Show progress updates
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

/// Macro for advanced pattern matching
macro_rules! match_pattern {
    ($address:expr, $pattern:expr, $case_sensitive:expr) => {{
        let addr = if $case_sensitive {
            $address.clone()
        } else {
            $address.to_lowercase()
        };
        let pat = if $case_sensitive {
            $pattern.to_string()
        } else {
            $pattern.to_lowercase()
        };

        if pat.starts_with('^') && pat.ends_with('$') {
            // Exact match (^pattern$)
            addr == pat[1..pat.len()-1].to_string()
        } else if pat.starts_with('^') {
            // Starts with (^pattern)
            addr.starts_with(&pat[1..])
        } else if pat.ends_with('$') {
            // Ends with (pattern$)
            addr.ends_with(&pat[..pat.len()-1])
        } else {
            // Contains anywhere
            addr.contains(&pat)
        }
    }};
}

fn main() {
    let args = Args::parse();

    println!("🚀 Starting Tron address generator (single-threaded)");
    println!("🔍 Pattern: '{}'", args.pattern);
    println!("🔠 Case-sensitive: {}", args.case_sensitive);
    println!("🔄 Max attempts: {}", args.max_attempts);

    let start_time = Instant::now();
    let mut attempts = 0;

    while attempts < args.max_attempts {
        attempts += 1;

        if args.verbose && attempts % 1_000_000 == 0 {
            println!("Attempts: {}M", attempts / 1_000_000);
        }

        if let Some((address, private_key)) = generate_tron_address() {
            if match_pattern!(address, &args.pattern, args.case_sensitive) {
                println!("\n🎉 Found matching address after {} attempts!", attempts);
                println!("📍 Address: {}", address);
                println!("🔑 Private key: {}", private_key);
                println!("⏱️ Time elapsed: {:.2}s", start_time.elapsed().as_secs_f32());
                return;
            }
        }
    }

    println!("\n🔴 No match found after {} attempts", attempts);
    println!("⏱️ Total time: {:.2}s", start_time.elapsed().as_secs_f32());
}

fn generate_tron_address() -> Option<(String, String)> {
    let secp = Secp256k1::new();
    let private_key = SecretKey::new(&mut OsRng);

    // Generate public key
    let public_key = private_key.public_key(&secp);
    let public_key_bytes = &public_key.serialize_uncompressed()[1..65];

    // Hash public key
    let mut hasher = Keccak256::new();
    hasher.update(public_key_bytes);
    let hash = hasher.finalize();

    // Get address bytes (last 20 bytes of hash)
    let address_bytes = &hash[12..32];

    // Create Tron address (0x41 prefix)
    let mut tron_address = vec![0x41];
    tron_address.extend_from_slice(address_bytes);

    // Calculate checksum
    let checksum = double_sha256(&tron_address)[..4].to_vec();

    // Final address with checksum
    let mut final_address = tron_address.clone();
    final_address.extend(checksum);

    // Base58 encoding
    let address = bs58::encode(final_address).into_string();
    let private_key_hex = hex::encode(private_key.as_ref());

    Some((address, private_key_hex))
}

fn double_sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let first_hash = hasher.finalize();

    let mut hasher = sha2::Sha256::new();
    hasher.update(first_hash);
    hasher.finalize().to_vec()
}