use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::RngCore;
use rayon::prelude::*;
use secp256k1::{All, PublicKey, Secp256k1, SecretKey};
use sha3::{Digest, Keccak256};

use crate::matcher::Matcher;

/// Number of keys generated between progress flushes and stop-flag checks.
const CHUNK: usize = 256;

/// TRON mainnet address version byte.
const TRON_ADDRESS_PREFIX: u8 = 0x41;

/// Interval between progress callbacks.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// A single vanity address that satisfied the configured conditions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FoundAddress {
    pub index: usize,
    pub address: String,
    pub private_key: String,
    pub found_at_attempts: u64,
}

/// Point-in-time counters published to observers while a search is running.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProgressSnapshot {
    pub attempts: u64,
    pub rate: f64,
    pub found: usize,
    pub target: usize,
    pub elapsed: f64,
}

/// Sink for search events. Implemented by the terminal and web front-ends.
pub trait SearchObserver: Send + Sync {
    /// Called periodically with aggregated counters.
    fn on_progress(&self, snapshot: ProgressSnapshot);
    /// Called once for every newly discovered address.
    fn on_found(&self, record: &FoundAddress);
}

/// Aggregated result of a search run.
pub struct SearchOutcome {
    pub records: Vec<FoundAddress>,
    pub total_attempts: u64,
    pub elapsed: Duration,
}

impl SearchOutcome {
    /// Average keys generated per second across all worker threads.
    pub fn keys_per_second(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            self.total_attempts as f64 / secs
        }
    }
}

/// Brute-force search for addresses matching the compiled conditions.
///
/// The `stop` flag is shared with the caller so a front-end can abort a running
/// search; it is also set internally once `target` records have been collected.
pub fn search(
    matcher: Arc<Matcher>,
    target: usize,
    threads: usize,
    stop: Arc<AtomicBool>,
    observer: Arc<dyn SearchObserver>,
) -> SearchOutcome {
    let start = Instant::now();
    let found: Mutex<Vec<FoundAddress>> = Mutex::new(Vec::with_capacity(target));
    let found_count = Arc::new(AtomicUsize::new(0));
    let attempts = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));

    let secp = Secp256k1::new();

    let reporter = {
        let attempts = Arc::clone(&attempts);
        let found_count = Arc::clone(&found_count);
        let done = Arc::clone(&done);
        let observer = Arc::clone(&observer);
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut last_attempts = attempts.load(Ordering::Relaxed);
            let mut last_sample = Instant::now();

            loop {
                std::thread::sleep(PROGRESS_INTERVAL);

                let now = Instant::now();
                let current = attempts.load(Ordering::Relaxed);
                let dt = now.duration_since(last_sample).as_secs_f64();
                let rate = if dt > 0.0 {
                    (current - last_attempts) as f64 / dt
                } else {
                    0.0
                };
                last_attempts = current;
                last_sample = now;

                observer.on_progress(ProgressSnapshot {
                    attempts: current,
                    rate,
                    found: found_count.load(Ordering::Relaxed),
                    target,
                    elapsed: started.elapsed().as_secs_f64(),
                });

                if done.load(Ordering::Relaxed) {
                    break;
                }
            }
        })
    };

    (0..threads).into_par_iter().for_each(|_| {
        let mut rng = rand::thread_rng();
        let mut pending: u64 = 0;

        'outer: loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            for _ in 0..CHUNK {
                let secret_key = sample_secret_key(&mut rng);
                let address = derive_address(&secp, &secret_key);
                pending += 1;

                if matcher.matches(&address) {
                    // Reserve a result slot first so concurrent workers can never
                    // overshoot the requested target count.
                    let slot = found_count.fetch_add(1, Ordering::SeqCst);
                    if slot >= target {
                        found_count.fetch_sub(1, Ordering::SeqCst);
                        attempts.fetch_add(pending, Ordering::Relaxed);
                        stop.store(true, Ordering::SeqCst);
                        break 'outer;
                    }

                    let record = FoundAddress {
                        index: slot + 1,
                        address,
                        private_key: hex::encode(secret_key.secret_bytes()),
                        found_at_attempts: attempts.load(Ordering::Relaxed) + pending,
                    };
                    found
                        .lock()
                        .expect("result mutex poisoned")
                        .push(record.clone());
                    observer.on_found(&record);

                    if slot + 1 >= target {
                        attempts.fetch_add(pending, Ordering::Relaxed);
                        stop.store(true, Ordering::SeqCst);
                        break 'outer;
                    }
                }
            }

            attempts.fetch_add(pending, Ordering::Relaxed);
            pending = 0;
        }
    });

    done.store(true, Ordering::SeqCst);
    let _ = reporter.join();

    let elapsed = start.elapsed();
    let mut records = found.into_inner().expect("result mutex poisoned");
    records.sort_by_key(|record| record.index);
    SearchOutcome {
        records,
        total_attempts: attempts.load(Ordering::Relaxed),
        elapsed,
    }
}

/// Draw a uniformly distributed 256-bit private key.
fn sample_secret_key(rng: &mut impl RngCore) -> SecretKey {
    loop {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        if let Ok(key) = SecretKey::from_slice(&bytes) {
            return key;
        }
    }
}

/// Derive the base58check TRON address for a secp256k1 private key.
fn derive_address(secp: &Secp256k1<All>, secret_key: &SecretKey) -> String {
    let public_key = PublicKey::from_secret_key(secp, secret_key);
    let uncompressed = public_key.serialize_uncompressed();

    // Keccak-256 over the 64-byte public key body, then take the trailing 20 bytes.
    let digest = Keccak256::digest(&uncompressed[1..]);

    let mut payload = [0u8; 21];
    payload[0] = TRON_ADDRESS_PREFIX;
    payload[1..].copy_from_slice(&digest[12..32]);

    bs58::encode(&payload).with_check().into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_address_shape_is_valid() {
        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();
        for _ in 0..8 {
            let key = sample_secret_key(&mut rng);
            let address = derive_address(&secp, &key);
            assert_eq!(address.len(), 34, "address: {address}");
            assert!(address.starts_with('T'), "address: {address}");
        }
    }

    #[test]
    fn private_key_one_maps_to_known_ethereum_payload() {
        // Private key 0x01 yields the canonical secp256k1 generator point whose
        // Ethereum address is a well-known constant; TRON reuses the same payload.
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        let key = SecretKey::from_slice(&bytes).expect("valid key");

        let address = derive_address(&secp, &key);
        let decoded = bs58::decode(&address)
            .with_check(None)
            .into_vec()
            .expect("valid base58check");

        assert_eq!(decoded.len(), 21);
        assert_eq!(decoded[0], TRON_ADDRESS_PREFIX);
        assert_eq!(
            hex::encode_upper(&decoded[1..]),
            "7E5F4552091A69125D5DFCB7B8C2659029395BDF"
        );
        assert!(address.starts_with('T'));
        assert_eq!(address.len(), 34);
    }
}
