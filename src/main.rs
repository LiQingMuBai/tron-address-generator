use secp256k1::{Secp256k1, SecretKey};
use sha3::{Digest, Keccak256};
use std::str;
use dotenv::dotenv;
use std::env;

// Tron地址前缀
const TRON_PREFIX: &[u8] = &[0x41];

fn main() {
    // 加载 .env 文件
    dotenv().ok();

    // 从环境变量获取目标后缀
    let target_suffix = env::var("TARGET_SUFFIX")
        .expect("Please set TARGET_SUFFIX in .env file");

    println!("Searching for Tron address ending with '{}'...", target_suffix);

    let secp = Secp256k1::new();
    let mut attempts = 0;

    loop {
        attempts += 1;

        // 生成随机私钥
        let private_key = SecretKey::new(&mut rand::thread_rng());

        // 获取对应的公钥
        let public_key = private_key.public_key(&secp);

        // 计算Tron地址
        let tron_address = public_key_to_tron_address(&public_key);

        // 检查地址是否以目标后缀结尾
        if tron_address.ends_with(&target_suffix) {
            println!("Found after {} attempts!", attempts);
            println!("Private Key (hex): {}", hex::encode(&private_key[..]));
            println!("Tron Address: {}", tron_address);
            break;
        }

        // 每10000次尝试打印一次进度
        if attempts % 10000 == 0 {
            println!("Attempts: {}", attempts);
        }
    }
}

// 将公钥转换为Tron地址
fn public_key_to_tron_address(public_key: &secp256k1::PublicKey) -> String {
    // 获取压缩公钥的字节表示
    let public_key_bytes = public_key.serialize();

    // 计算Keccak256哈希（注意跳过压缩前缀0x02或0x03）
    let mut hasher = Keccak256::new();
    hasher.update(&public_key_bytes[1..]);
    let hash = hasher.finalize();

    // 取最后20字节作为地址
    let address_bytes = &hash[12..32];

    // 添加Tron前缀0x41
    let mut tron_address_bytes = Vec::with_capacity(21);
    tron_address_bytes.extend_from_slice(TRON_PREFIX);
    tron_address_bytes.extend_from_slice(address_bytes);

    // Base58编码
    bs58::encode(&tron_address_bytes).into_string()
}