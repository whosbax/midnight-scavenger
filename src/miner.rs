use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use rand::{Rng, thread_rng};
use hex;
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
#[derive(Debug)]
pub struct MinerResult {
    pub nonce: String,
    pub preimage: String,
}

/// Fonction principale de minage (multi‑thread)
pub fn mine(config: MinerConfig, num_threads: usize) -> Result<MinerResult, String> {
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};
    use ashmaize::{Rom, RomGenerationType, hash};

    info!(
        "🚀 Starting mining: address={}, threads={}, challenge_id={:?}",
        config.address,
        num_threads,
        config.challenge.challenge_id
    );
    debug!("MinerConfig details: {:?}", config);

    let challenge = Arc::new((*config.challenge).clone());
    debug!("Cloned challenge params: {:?}", challenge);

    // Initialisation ROM TwoStep (prod)
    let rom_seed = challenge.no_pre_mine
        .as_ref()
        .map(|s| s.as_bytes())
        .unwrap_or_else(|| b"default-seed");
    debug!("ROM seed: {:?}", rom_seed);

    let rom = Arc::new(Rom::new(
        rom_seed,
        RomGenerationType::TwoStep {
            pre_size: 16 * 1024 * 1024,
            mixing_numbers: 4,
        },
        1024 * 1024 * 1024,
    ));
    info!("ROM initialized for mining.");

    let found_flag = Arc::new(AtomicBool::new(false));
    let result = Arc::new(Mutex::new(None));
    let address = config.address.clone();
    debug!("Mining address set to: {}", address);

    // Convertir la difficulté hex en u32
    let difficulty_mask = challenge.difficulty
        .as_ref()
        .map(|d| {
            let parsed = u32::from_str_radix(d, 16).unwrap_or(0);
            debug!("Parsed difficulty string \"{}\" → {}", d, parsed);
            parsed
        })
        .unwrap_or_else(|| {
            warn!("No difficulty specified in challenge; using mask = 0");
            0
        });
    info!("Difficulty mask computed: {:#034b}", difficulty_mask);

    let mut handles = Vec::with_capacity(num_threads);
    info!("Spawning {} mining threads.", num_threads);

    for thread_index in 0..num_threads {
        let challenge = Arc::clone(&challenge);
        let rom = Arc::clone(&rom);
        let address = address.clone();
        let found = Arc::clone(&found_flag);
        let result_ref = Arc::clone(&result);

        debug!("Preparing thread #{}", thread_index);
        let handle = std::thread::spawn(move || {
            debug!("Thread {} started.", thread_index);
            let mut rng = rand::thread_rng();
            let mut nonce: u64 = thread_rng().gen::<u64>().wrapping_add(thread_index as u64);
            debug!("Thread {} initial nonce: {:016x}", thread_index, nonce);

            let nb_loops: u32 = 8;
            let nb_instrs: u32 = 256;
            debug!("Thread {} parameters: nb_loops={}, nb_instrs={}", thread_index, nb_loops, nb_instrs);

            while !found.load(Ordering::Relaxed) {
                let preimage = format!(
                    "{0:016x}{1}{2}{3}{4}{5}{6}",
                    nonce,
                    address,
                    challenge.challenge_id,
                    challenge.difficulty.clone().unwrap_or_default(),
                    challenge.no_pre_mine.clone().unwrap_or_default(),
                    challenge.latest_submission.clone().unwrap_or_default(),
                    challenge.no_pre_mine_hour.clone().unwrap_or_default()
                );
                debug!("Thread {} preimage generated length {}", thread_index, preimage.len());

                let digest = hash(preimage.as_bytes(), &rom, nb_loops, nb_instrs);
                debug!("Thread {} digest computed: {:?}", thread_index, &digest[0..4]);

                let hash_prefix = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
                if (hash_prefix & !difficulty_mask) == 0 {
                    debug!(
                        "🧮 Thread {} | nonce {:016x} | hash_prefix {:032b} | mask {:032b} | check {:032b}",
                        thread_index, nonce, hash_prefix, difficulty_mask, hash_prefix & !difficulty_mask
                    );

                    if !found.swap(true, Ordering::Relaxed) {
                        info!("Thread {} found valid nonce {:016x}", thread_index, nonce);
                        if let Ok(mut guard) = result_ref.lock() {
                            *guard = Some(MinerResult {
                                nonce: format!("{:016x}", nonce),
                                preimage,
                            });
                            debug!("Thread {} wrote result to shared state.", thread_index);
                        } else {
                            error!("Thread {} failed to lock result mutex", thread_index);
                        }
                    } else {
                        warn!("Thread {} found a result, but another thread already set the flag", thread_index);
                    }
                    break;
                }

                // nonce = nonce.wrapping_add(num_threads);
            }
            debug!("Thread {} exiting loop.", thread_index);
        });

        handles.push(handle);
    }

    for handle in handles {
        debug!("Joining a mining thread.");
        handle.join().map_err(|_| {
            error!("A mining thread panicked.");
            "Thread panicked".to_string()
        })?;
    }
    info!("All mining threads joined.");

    let maybe = Arc::try_unwrap(result)
        .map_err(|_| {
            error!("Error unwrapping result Arc.");
            "Error unwrapping result Arc".to_string()
        })?
        .into_inner()
        .map_err(|_| {
            error!("Mutex poisoned when retrieving result.");
            "Mutex poisoned".to_string()
        })?;

    match &maybe {
        Some(r) => info!("Mining successful: nonce={}, preimage length={}", r.nonce, r.preimage.len()),
        None => warn!("Mining completed but no result found."),
    }

    maybe.ok_or_else(|| {
        error!("No result found from mining.");
        "No result found".to_string()
    })
}
