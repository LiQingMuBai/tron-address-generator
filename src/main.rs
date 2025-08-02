use bs58;
use hex;
use rand::Rng;
use secp256k1;
use sha3::{Digest, Keccak256};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
/// 从 `.env` 读取目标后缀
fn load_target_suffix() -> String {
    dotenv::dotenv().ok();
    env::var("TARGET_SUFFIX").unwrap_or_else(|_| "UUUU".to_string())
}

/// 生成随机的 TRON 地址，并返回（私钥, 地址）
fn generate_tron_address_with_private_key() -> (String, String) {
    let mut rng = rand::thread_rng();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key);

    // 1. 打印私钥（16进制格式）
    let private_key_hex = hex::encode(private_key);
    // println!("Generated Private Key: 0x{}", private_key_hex);

    // 2. 计算公钥和地址
    let secp = secp256k1::Secp256k1::new();
    let secret_key = secp256k1::SecretKey::from_slice(&private_key).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let public_key_bytes = public_key.serialize_uncompressed();

    let mut hasher = Keccak256::new();
    hasher.update(&public_key_bytes[1..]);
    let hash = hasher.finalize();
    let address_bytes = &hash[12..32];

    let mut prefixed_address = vec![0x41];
    prefixed_address.extend_from_slice(address_bytes);

    let checksum = &Keccak256::digest(&Keccak256::digest(&prefixed_address))[0..4];
    prefixed_address.extend_from_slice(checksum);
    let address = bs58::encode(prefixed_address).into_string();

    (private_key_hex, address)
}

/// 检查地址是否匹配目标后缀
fn is_matching_address(address: &str, suffix: &str) -> bool {
    let items = parse_comma_separated(suffix);

    for item in items {
        // println!("- {}", item);
        // thread::sleep(Duration::from_millis(1500));
        if address.len() == 34 && address.ends_with(&item) {
            return true;
        }
    }
    return false;
}
fn parse_comma_separated(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn main() {
    let target_suffix = load_target_suffix();
    println!(
        "Searching for TRON addresses ending with: '{}'",
        target_suffix
    );

    let found = Arc::new(AtomicBool::new(false));
    let num_threads = num_cpus::get();

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let found = Arc::clone(&found);
            let suffix = target_suffix.clone();
            thread::spawn(move || {
                while !found.load(Ordering::Relaxed) {
                    let (private_key, address) = generate_tron_address_with_private_key();
                    if is_matching_address(&address, &suffix) {
                        println!("\n✅ Found matching address!");
                        println!("Private Key: 0x{}", private_key);
                        println!("TRON Address: {}", address);
                        found.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
