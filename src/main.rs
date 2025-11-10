mod api_client;
mod miner;
mod wallet;
mod wallet_container;
mod donations;
mod donations_manager;
mod stats_client;

use std::{
    env,
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::AtomicU64,
    time::{Duration, Instant},
};
use chrono::{NaiveDate, Utc};
use num_cpus;
use tokio::time::sleep;
use log::{info, LevelFilter};
use env_logger::Builder;
use std::io::Write;
use rand::{Rng, distributions::Alphanumeric};

use api_client::ApiClient;
use miner::{mine, MinerConfig};
use wallet_container::WalletContainer;
use donations_manager::{load_or_create_donate_addresses, process_donations_for_wallets};
use stats_client::start_stats_reporter;

fn generate_random_string() -> String {
    let length = 10;
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

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
    let instance_ = instance_id.to_string();
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
    let uniq_inst_id = Arc::new(generate_random_string());
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

    // --- Donations ---
    let wallets_path = wallet_dir.clone();
    let client_clone = Arc::clone(&client);
    let instance_id_clone = instance_id.clone();
    let uniq_inst_id_clone = Arc::clone(&uniq_inst_id);

    tokio::spawn(async move {
        loop {
            let client_ref = Arc::clone(&client_clone);
            let uniq_inst_id_ref = Arc::clone(&uniq_inst_id_clone);
            let donate_addresses = load_or_create_donate_addresses(
                "/usr/local/bin/config",
                use_mainnet,
                &instance_id_clone,
            );
            process_donations_for_wallets(
                client_ref,
                &wallets_path.to_str().unwrap(),
                &donate_addresses,
                &instance_id_clone,
                &uniq_inst_id_ref,
            )
            .await;
            sleep(Duration::from_secs(600)).await;
        }
    });

    let total_threads = env::var("MINER_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(num_cpus::get);
    let threads_per_wallet = std::cmp::max(total_threads / wallets.len(), 1);

    let end_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
    let hash_counter = Arc::new(AtomicU64::new(0));

    // --- Lancement des mineurs ---
    for (idx, wallet) in wallets.into_iter().enumerate() {
        let client_clone = client.clone();
        let instance_clone = instance_id.clone();
        let hash_counter_clone = hash_counter.clone();
        let uniq_inst_id_clone = Arc::clone(&uniq_inst_id);
        let wallet_idx = idx + 1;

        tokio::spawn(async move {
            let wallet_prefix = format!("[{}|wallet-{}|{}]", instance_clone, wallet_idx, &wallet.address[..10]);
            info!("{} ⛏️ Miner lancé avec {} threads", wallet_prefix, threads_per_wallet);

            let container_id_str = (*uniq_inst_id_clone).clone();

            if let Ok(terms) =
                client_clone.get_terms(None, Some(instance_clone.clone()), Some(container_id_str.clone())).await
            {
                let signature = wallet.sign_cip30(&terms.message);
                let pubkey = wallet.public_key_hex();
                let _ = client_clone
                    .register_address(
                        &wallet.address,
                        &signature,
                        &pubkey,
                        Some(instance_clone.clone()),
                        Some(container_id_str.clone()),
                    )
                    .await;
            }

            loop {
                if Utc::now().date_naive() > end_date {
                    sleep(Duration::from_secs(3600)).await;
                    continue;
                }

                if let Ok(resp) =
                    client_clone.get_challenge(Some(instance_clone.clone()), Some(container_id_str.clone())).await
                {
                    if let Some(challenge) = resp.challenge {
                        let miner_config = MinerConfig {
                            address: wallet.address.clone(),
                            challenge: Arc::new(challenge.clone()),
                        };

                        let start = Instant::now();

                        // ✅ Spawn CPU-intensive mining task in blocking thread pool
                        match tokio::task::spawn_blocking({
                            let miner_config = miner_config.clone();
                            let hash_counter = hash_counter_clone.clone();
                            move || mine(miner_config, threads_per_wallet, Some(hash_counter))
                        })
                        .await
                        {
                            Ok(Ok(result)) => {
                                let duration = start.elapsed();
                                info!(
                                    "{} 💎 Nonce trouvé={} ({:.2?})",
                                    wallet_prefix, result.nonce, duration
                                );

                                let _ = client_clone
                                    .submit_solution(
                                        &wallet.address,
                                        &challenge.challenge_id,
                                        &result.nonce,
                                        Some(instance_clone.clone()),
                                        Some(container_id_str.clone()),
                                    )
                                    .await;
                            }
                            Ok(Err(err_msg)) => {
                                info!("{} ⚠️ Minage terminé sans résultat: {}", wallet_prefix, err_msg);
                            }
                            Err(join_err) => {
                                info!("{} ⚠️ spawn_blocking error: {:?}", wallet_prefix, join_err);
                            }
                        }
                    }
                }

                sleep(Duration::from_secs(10)).await;
            }
        });
    }

    // --- Stats reporter ---
    let server_url = env::var("STATS_BACKEND_URL")
        .unwrap_or_else(|_| "http://stats-backend:8080/insert_stat".to_string());
    let version = env::var("APP_VERSION").unwrap_or_else(|_| "0.1.0".to_string());

    start_stats_reporter(
        (*uniq_inst_id).clone(),
        instance_id.clone(),
        hash_counter.clone(),
        server_url,
        version,
        30,
    );

    info!("🕰️ Boucle de maintien infinie démarrée");
    loop {
        sleep(Duration::from_secs(3600)).await;
    }
}
