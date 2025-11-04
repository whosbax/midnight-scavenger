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

/// Fonction principale de minage (multi-thread)
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

    let challenge = Arc::new((*config.challenge).clone());


    // Initialisation ROM TwoStep (prod)
    let rom_seed = challenge.no_pre_mine
        .as_ref()
        .map(|s| s.as_bytes())
        .unwrap_or_else(|| b"default-seed");
    let rom = Arc::new(Rom::new(
        rom_seed,
        RomGenerationType::TwoStep {
            pre_size: 16 * 1024 * 1024,
            mixing_numbers: 4,
        },
        1024 * 1024 * 1024,
    ));


    let found_flag = Arc::new(AtomicBool::new(false));
    let result = Arc::new(Mutex::new(None));
    let address = config.address.clone();

    // Convertir la difficulté hex en u32
    let difficulty_mask = challenge.difficulty
        .as_ref()
        .map(|d| u32::from_str_radix(d, 16).unwrap_or(0))
        .unwrap_or(0);

    let mut handles = Vec::with_capacity(num_threads);

    for thread_index in 0..num_threads {
        let challenge = Arc::clone(&challenge);
        let rom = Arc::clone(&rom);
        let address = address.clone();
        let found = Arc::clone(&found_flag);
        let result_ref = Arc::clone(&result);

        let handle = std::thread::spawn(move || {
            // ✅ Correction principale : nonce séquentiel
            let mut rng = rand::thread_rng();
            //let nonce: u64 = rng.gen::<u64>() + thread_index as u64;
            //let mut nonce: u64 = thread_rng().gen::<u64>().wrapping_add(thread_index as u64);
            let mut nonce: u64 = thread_rng().gen::<u64>().wrapping_add(wallet_index as u64);

            let nb_loops: u32 = 8;
            let nb_instrs: u32 = 256;

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

                let digest = hash(preimage.as_bytes(), &rom, nb_loops, nb_instrs);

                let hash_prefix = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
                if (hash_prefix & !difficulty_mask) == 0 {

                    debug!(
                        "🧮 Thread {} | nonce {:016x} | hash_prefix {:032b} | mask {:032b} | check {:032b}",
                        thread_index, nonce, hash_prefix, difficulty_mask, hash_prefix & !difficulty_mask
                    );

                    if !found.swap(true, Ordering::Relaxed) {
                        if let Ok(mut guard) = result_ref.lock() {
                            *guard = Some(MinerResult {
                                nonce: format!("{:016x}", nonce),
                                preimage,
                            });
                        }
                    }
                    break;
                }

                //nonce = nonce.wrapping_add(num_threads); // incrémente pour éviter collision
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().map_err(|_| "Thread panicked".to_string())?;
    }

    // Après que tous les threads soient terminés
    let maybe = Arc::try_unwrap(result)
        .map_err(|_| "Error unwrapping result Arc".to_string())?
        .into_inner()
        .map_err(|_| "Mutex poisoned".to_string())?;

    maybe.ok_or_else(|| "No result found".to_string())
}

