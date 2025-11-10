// src/miner.rs (optimised, no new deps)
use parking_lot::{Mutex, RwLock};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use rand::{Rng, thread_rng};
use crate::api_client::ChallengeParams;
use ashmaize::{Rom, RomGenerationType, hash};
use log::{info, debug, warn, error};

/// Configuration du minage
#[derive(Clone, Debug)]
pub struct MinerConfig {
    pub address: String,
    pub challenge: Arc<ChallengeParams>,
}

/// Résultat du minage
#[derive(Debug, Clone)]
pub struct MinerResult {
    pub nonce: String,
    pub preimage: String,
}

// Global ROM cache keyed by seed bytes
static ROM_CACHE: OnceLock<RwLock<HashMap<Vec<u8>, Arc<Rom>>>> = OnceLock::new();

fn get_or_create_rom(seed: &[u8]) -> Arc<Rom> {
    let cache = ROM_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let key = seed.to_vec();

    // Fast path: read lock to check existing
    {
        let read_guard = cache.read();
        if let Some(existing) = read_guard.get(&key) {
            return Arc::clone(existing);
        }
    }

    // Not found: create ROM outside of locks (expensive operation)
    let rom = Arc::new(Rom::new(
        seed,
        RomGenerationType::TwoStep {
            pre_size: 16 * 1024 * 1024,
            mixing_numbers: 4,
        },
        1024 * 1024 * 1024,
    ));

    // Insert under write lock (double-check pattern)
    {
        let mut write_guard = cache.write();
        if let Some(existing) = write_guard.get(&key) {
            return Arc::clone(existing);
        }
        write_guard.insert(key, Arc::clone(&rom));
    }

    rom
}

/// Fonction principale de minage (multi-thread)
///
/// Si `global_counter` est fourni, chaque hash calculé incrémente un compteur partagé
/// utilisé pour calculer le hashrate global (cross-container via volume partagé).
pub fn mine(
    config: MinerConfig,
    num_threads: usize,
    global_counter: Option<Arc<AtomicU64>>,
) -> Result<MinerResult, String> {
    info!(
        "🚀 Starting mining: address={}, threads={}, challenge_id={:?}",
        config.address,
        num_threads,
        config.challenge.challenge_id
    );
    debug!("MinerConfig details: {:?}", config);

    // Clone challenge once
    let challenge = Arc::new((*config.challenge).clone());
    debug!("Cloned challenge params: {:?}", challenge);

    // Prepare ROM seed bytes once
    let rom_seed_bytes: Vec<u8> = challenge
        .no_pre_mine
        .as_ref()
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_else(|| b"default-seed".to_vec());
    debug!("ROM seed bytes length: {}", rom_seed_bytes.len());

    // Use global cache to avoid regenerating heavy ROM if seed is identical
    let rom = get_or_create_rom(&rom_seed_bytes);
    info!("ROM initialized for mining (from cache or new).");

    let found_flag = Arc::new(AtomicBool::new(false));
    let result = Arc::new(Mutex::new(None));
    let address = config.address.clone();
    debug!("Mining address set to: {}", address);

    // Convertir la difficulté hex en u32 (once)
    let difficulty_mask = challenge
        .difficulty
        .as_ref()
        .and_then(|d| u32::from_str_radix(d, 16).ok())
        .unwrap_or_else(|| {
            warn!("No difficulty specified in challenge; using mask = 0");
            0
        });
    info!("Difficulty mask computed: {:#034b}", difficulty_mask);

    // Pre-extract constant strings from challenge to avoid clones per-iteration
    let challenge_id = challenge.challenge_id.clone();
    let difficulty_str = challenge.difficulty.clone().unwrap_or_default();
    let no_pre_mine_str = challenge.no_pre_mine.clone().unwrap_or_default();
    let latest_submission_str = challenge.latest_submission.clone().unwrap_or_default();
    let no_pre_mine_hour_str = challenge.no_pre_mine_hour.clone().unwrap_or_default();

    let mut handles = Vec::with_capacity(num_threads);
    info!("Spawning {} mining threads.", num_threads);

    for thread_index in 0..num_threads {
        let rom = Arc::clone(&rom);
        let address = address.clone();
        let found = Arc::clone(&found_flag);
        let result_ref = Arc::clone(&result);
        let global_counter = global_counter.clone();

        // Clone the strings we will use in the thread
        let challenge_id = challenge_id.clone();
        let difficulty_str = difficulty_str.clone();
        let no_pre_mine_str = no_pre_mine_str.clone();
        let latest_submission_str = latest_submission_str.clone();
        let no_pre_mine_hour_str = no_pre_mine_hour_str.clone();

        let handle = std::thread::spawn(move || {
            debug!("🧵 Thread {} started.", thread_index);
            let mut rng = thread_rng();
            let mut nonce: u64 = rng.gen::<u64>().wrapping_add(thread_index as u64);
            debug!("Thread {} initial nonce: {:016x}", thread_index, nonce);

            // Local config constants
            let nb_loops: u32 = 8;
            let nb_instrs: u32 = 256;

            // Reusable buffer for preimage construction to avoid allocation each iter
            let mut preimage_buf = String::with_capacity(256);

            // Local counter to minimize atomic contention
            let mut local_counter: u64 = 0;
            // Batch size tuned for reduced contention but still responsive
            const LOCAL_BATCH: u64 = 1_000;

            debug!(
                "Thread {} parameters: nb_loops={}, nb_instrs={}",
                thread_index, nb_loops, nb_instrs
            );

            // Use Acquire for load to ensure memory ordering with writers
            while !found.load(Ordering::Acquire) {
                // Build preimage into preimage_buf (reuse, avoid format!)
                preimage_buf.clear();
                // hex nonce (16 hex digits), then concatenated fields
                write!(&mut preimage_buf, "{:016x}", nonce).unwrap();
                preimage_buf.push_str(&address);
                preimage_buf.push_str(&challenge_id);
                preimage_buf.push_str(&difficulty_str);
                preimage_buf.push_str(&no_pre_mine_str);
                preimage_buf.push_str(&latest_submission_str);
                preimage_buf.push_str(&no_pre_mine_hour_str);

                let digest = hash(preimage_buf.as_bytes(), &rom, nb_loops, nb_instrs);

                // Increment local counter and flush to global in batches
                local_counter += 1;
                if let Some(ref counter) = global_counter {
                    if local_counter >= LOCAL_BATCH {
                        counter.fetch_add(local_counter, Ordering::Relaxed);
                        local_counter = 0;
                    }
                }

                let hash_prefix = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);

                if (hash_prefix & !difficulty_mask) == 0 {
                    // Found a solution
                    // Use AcqRel swap to ensure proper ordering with other threads
                    if !found.swap(true, Ordering::AcqRel) {
                        info!(
                            "✅ Thread {} found valid nonce {:016x} | prefix={:032b}",
                            thread_index, nonce, hash_prefix
                        );
                        // Ensure global counter updated before exit
                        if let Some(ref counter) = global_counter {
                            if local_counter > 0 {
                                counter.fetch_add(local_counter, Ordering::Relaxed);
                                local_counter = 0;
                            }
                        }
                        let mut guard = result_ref.lock();
                        *guard = Some(MinerResult {
                            nonce: format!("{:016x}", nonce),
                            preimage: preimage_buf.clone(),
                        });
                        debug!("Thread {} wrote result to shared state.", thread_index);
                    } else {
                        debug!(
                            "Thread {} also found a solution but another thread won the race.",
                            thread_index
                        );
                    }
                    break;
                }

                // Occasional debug to avoid too-frequent logging
                if nonce % 10_000_000 == 0 {
                    debug!(
                        "Thread {} still mining... current nonce={:016x}, prefix={:032b}",
                        thread_index, nonce, hash_prefix
                    );
                }

                // Advance nonce by number of threads to avoid collisions
                nonce = nonce.wrapping_add(num_threads as u64);
            }

            // flush remaining local counter if we exit without finding solution
            if let Some(ref counter) = global_counter {
                if local_counter > 0 {
                    counter.fetch_add(local_counter, Ordering::Relaxed);
                }
            }

            debug!("Thread {} exiting loop.", thread_index);
        });

        handles.push(handle);
    }

    for handle in handles {
        if let Err(_) = handle.join() {
            error!("A mining thread panicked.");
            return Err("Thread panicked".to_string());
        }
    }

    info!("All mining threads joined.");

    let maybe = Arc::try_unwrap(result)
        .map_err(|_| "Error unwrapping result Arc".to_string())?
        .into_inner();

    match maybe {
        Some(ref r) => {
            info!(
                "🎉 Mining successful: nonce={}, preimage length={}",
                r.nonce, r.preimage.len()
            );
            Ok(r.clone())
        }
        None => {
            warn!("⚠️ Mining completed but no result found.");
            Err("No result found".to_string())
        }
    }
}
