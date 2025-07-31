use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use rand::Rng;
use sha3::{Digest, Keccak256};
use bs58;
use secp256k1;

/// 从 `.env` 读取目标后缀
fn load_target_suffix() -> String {
    dotenv::dotenv().ok(); // 加载 .env 文件
    env::var("TARGET_SUFFIX").unwrap_or_else(|_| "UUUU".to_string())
}

/// 生成随机的 TRON 地址
fn generate_random_tron_address() -> String {
    let mut rng = rand::thread_rng();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key);

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
    bs58::encode(prefixed_address).into_string()
}

/// 检查地址是否以指定后缀结尾
fn is_matching_address(address: &str, suffix: &str) -> bool {
    address.len() == 34 && address.ends_with(suffix)
}

fn main() {
    let target_suffix = load_target_suffix();
    println!("Searching for TRON addresses ending with: '{}'", target_suffix);

    let found = Arc::new(AtomicBool::new(false));
    let num_threads = num_cpus::get();
    println!("Searching with {} threads...", num_threads);
    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let found = Arc::clone(&found);
            let suffix = target_suffix.clone();
            thread::spawn(move || {
                while !found.load(Ordering::Relaxed) {
                    let address = generate_random_tron_address();
                    println!("address: {}", address);
                    if is_matching_address(&address, &suffix) {
                        println!("Found matching address: {}", address);
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
