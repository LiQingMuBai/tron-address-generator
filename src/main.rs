use secp256k1::{Secp256k1, SecretKey};
use rand::rngs::OsRng;
use sha3::{Digest, Keccak256};
use std::time::Instant;
use dotenv::dotenv;
use std::env;
use bs58;

fn main() {
    // 加载 .env 文件
    dotenv().ok();

    // 从环境变量读取后缀，用逗号分隔
    let suffixes_str = env::var("TRON_ADDRESS_SUFFIXES")
        .expect("请在 .env 文件中设置 TRON_ADDRESS_SUFFIXES");

    let suffixes: Vec<&str> = suffixes_str.split(',').map(|s| s.trim()).collect();

    // 从环境变量读取最大尝试次数
    let max_attempts = env::var("MAX_ATTEMPTS")
        .map(|s| s.parse().unwrap_or(100_000_000))
        .unwrap_or(100_000_000);

    println!("开始生成指定后缀的波场地址...");
    println!("目标后缀: {:?}", suffixes);
    println!("最大尝试次数: {}", max_attempts);

    for suffix in suffixes {
        println!("\n正在生成以 '{}' 结尾的地址...", suffix);
        let start_time = Instant::now();

        if let Some((address, private_key)) = find_address_with_suffix(suffix, max_attempts) {
            println!("成功生成地址!");
            println!("地址: {}", address);
            println!("私钥: {}", private_key);
            println!("耗时: {:.2}秒", start_time.elapsed().as_secs_f32());
        } else { println!("在 {} 次尝试后未找到以 '{}' 结尾的地址", max_attempts, suffix);
        }
    }
}

fn find_address_with_suffix(suffix: &str, max_attempts: u64) -> Option<(String, String)> {
    let secp = Secp256k1::new();
    let mut rng = OsRng;

    for _ in 0..max_attempts {
        // 生成随机私钥
        let private_key = SecretKey::new(&mut rng);
        let private_key_hex = hex::encode(private_key.as_ref());

        // 从私钥获取公钥
        let public_key = private_key.public_key(&secp);
        let public_key_bytes = &public_key.serialize_uncompressed()[1..]; // 去掉前缀

        // 计算Keccak256哈希
        let mut hasher = Keccak256::new();
        hasher.update(public_key_bytes);
        let hash = hasher.finalize();

        // 取最后20字节作为地址
        let address_bytes = &hash[hash.len()-20..];

        // 添加波场地址前缀 0x41
        let mut tron_address = vec![0x41];
        tron_address.extend_from_slice(address_bytes);

        // 计算双SHA256哈希的前4字节作为校验和
        let checksum = double_sha256(&tron_address)[..4].to_vec();

        // 组合地址和校验和
        let mut address_with_checksum = tron_address.clone();
        address_with_checksum.extend(checksum);

        // Base58编码
        let address = bs58::encode(address_with_checksum).into_string();

        println!("地址: {}", address);
        // 检查地址是否以指定前缀结尾(区分大小写)
        let start = "T";
        let prefix = format!("{}{}", start, suffix);
        // 检查地址是否以指定后缀结尾(区分大小写)
        if address.ends_with(&suffix) ||address.starts_with(&prefix) {
            return Some((address, private_key_hex));
        }
    }

    None
}

fn double_sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let first_hash = hasher.finalize();

    let mut hasher = sha2::Sha256::new();
    hasher.update(first_hash);
    hasher.finalize().to_vec()
}