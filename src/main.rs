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
    info!("📂 [{}] seed_path: {}", instance_id, seed_path);
    info!("📂 [{}] key_path: {}", instance_id, key_path);
    debug!("init_wallet_container: config_dir={}, use_mainnet={}, instance_id={}", config_dir, use_mainnet, instance_id);

    let container = WalletContainer::load_or_create(seed_path, key_path, use_mainnet, max_wallets)?;
    info!("🔑 [{}] WalletContainer initialisé", instance_id);
    Ok(Arc::new(container))
}

/// Initialisation du logger
fn init_logger(instance_id: &str) {
    let instance_id = instance_id.to_string();
    let inst = instance_id.clone();
    Builder::new()
        .format(move |buf, record| {
            let thread = std::thread::current();
            let thread_info = thread
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{:?}", thread.id()));
            writeln!(
                buf,
                "{} [{}] [inst:{}] [thread:{}] {}",
                Utc::now().to_rfc3339(),
                record.level(),
                inst,
                thread_info,
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Info)
        .init();

    info!("Logger initialisé pour l’instance {}", instance_id);
    debug!("Logger format configuré, filter = Info");
}

/// Trouve ou crée un dossier d’instance disponible
fn get_instance_dir(base_dir: &str) -> (String, PathBuf) {
    debug!("get_instance_dir: base_dir={}", base_dir);
    fs::create_dir_all(base_dir).unwrap_or_else(|e| {
        panic!("❌ Impossible de créer le dossier racine {}: {}", base_dir, e)
    });
    debug!("Dossier racine {} créé ou déjà existant", base_dir);

    for i in 1..=100 {
        let inst_dir = Path::new(base_dir).join(format!("{}", i));
        let lock_file = inst_dir.join("in_use.lock");
        debug!("Checking inst_dir={}, lock_file={}", inst_dir.display(), lock_file.display());

        if inst_dir.exists() && lock_file.exists() {
            debug!("Instance dir {} et fichier lock {} déjà utilisés", inst_dir.display(), lock_file.display());
            continue;
        }

        if !inst_dir.exists() {
            fs::create_dir_all(&inst_dir).unwrap_or_else(|e| {
                panic!("❌ Impossible de créer le dossier {}: {}", inst_dir.display(), e)
            });
            info!("📁 Nouveau dossier d’instance créé : {}", inst_dir.display());
        }

        File::create(&lock_file).unwrap_or_else(|e| {
            panic!("❌ Impossible de créer le fichier lock {}: {}", lock_file.display(), e)
        });
        info!("🔒 Fichier de lock créé : {}", lock_file.display());

        let inst_name = format!("miner-{}", i);
        info!("Instance assignée : {}", inst_name);
        debug!("Returning (inst_name={}, inst_dir={})", inst_name, inst_dir.display());
        return (inst_name, inst_dir);
    }

    panic!("❌ Aucun dossier d'instance disponible dans {}", base_dir);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config_root = "/usr/local/bin/config";
    info!("Base config root : {}", config_root);

    let (instance_id, config_dir) = get_instance_dir(config_root);
    debug!("instance_id={}, config_dir={}", instance_id, config_dir.display());
    let wallet_dir = config_dir.join(&instance_id).join("wallets");
    debug!("wallet_dir path : {}", wallet_dir.display());

    init_logger(&instance_id);

    for dir in [&wallet_dir] {
        if !dir.exists() {
            fs::create_dir_all(dir).unwrap_or_else(|e| {
                panic!("❌ [{}] Impossible de créer le dossier {}: {}", instance_id, dir.display(), e)
            });
            info!("📁 [{}] Dossier créé: {}", instance_id, dir.display());
        } else {
            debug!("📁 [{}] Dossier déjà existant: {}", instance_id, dir.display());
        }
    }

    info!("🚀 Démarrage du Scavenger Miner [instance: {}]", instance_id);

    let args: Vec<String> = env::args().collect();
    debug!("Arguments reçus : {:?}", args);
    let base_url = env::var("APP_BASE_URL")
        .unwrap_or_else(|_| {
            let default = "https://scavenger.prod.gd.midnighttge.io".to_string();
            debug!("APP_BASE_URL non défini, utilisation de la valeur par défaut {}", default);
            default
        });
    info!("Base URL du client : {}", base_url);
    let use_mainnet = true;
    debug!("use_mainnet = {}", use_mainnet);

    let client = Arc::new(ApiClient::new(&base_url)?);
    info!("Client API initialisé");
    let max_wallets: usize = env::var("MAX_WALLETS_PER_INSTANCE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    info!("max_wallets per instance = {}", max_wallets);
    debug!("MAX_WALLETS_PER_INSTANCE env var parsed or default");

    let wallet_container = init_wallet_container(wallet_dir.to_str().unwrap(), use_mainnet, max_wallets, &instance_id)?;
    let wallets = wallet_container.read_all();
    info!("💼 [{}] {} wallets chargés", instance_id, wallets.len());
    debug!("Wallets loaded: {:?}", wallets.iter().map(|w| &w.address).collect::<Vec<_>>());

    // --- Mode test optionnel ---
    if args.contains(&"--test-wallet".to_string()) {
        info!("Mode --test-wallet activé");
        for w in &wallets {
            info!("=== Test wallet {} ===", w.address);
            info!("Clé publique : {}", w.public_key_hex());
            info!("Signature test : {}", w.sign("test_message"));
        }
        info!("Fin du mode --test-wallet");
        return Ok(());
    }

    // --- Chargement ou création des adresses de donation ---
    let donate_list_path = Path::new("/usr/local/bin/config/donate_list.txt");
    let donate_seeds_path = Path::new("/usr/local/bin/config/donate_list_seed.txt");
    debug!("donate_list_path = {}", donate_list_path.display());
    debug!("donate_seeds_path = {}", donate_seeds_path.display());
    let mut donate_addresses: Vec<String> = Vec::new();

    if donate_list_path.exists() {
        debug!("Fichier de liste de donation existant détecté");
        donate_addresses = fs::read_to_string(donate_list_path)?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();
        info!("💰 [{}] Liste de donation chargée ({} adresses)", instance_id, donate_addresses.len());
        debug!("donate_addresses = {:?}", donate_addresses);
    } else {
        warn!("⚠️ [{}] Aucune liste globale de donation trouvée. Création automatique...", instance_id);

        let mut seeds = Vec::new();
        let mut addresses = Vec::new();
        debug!("Création automatique de 3 wallets de donation");
        for i in 0..3 {
            let w = wallet::Wallet::generate(use_mainnet);
            seeds.push(w.mnemonic.clone().unwrap_or_default());
            addresses.push(w.address.clone());
            info!("💰 [{}] Wallet de donation {} : {}", instance_id, i + 1, w.address);
            debug!("Donation wallet mnemonic: {}, address: {}", seeds.last().unwrap(), addresses.last().unwrap());
        }
        addresses.push("addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h".to_string());
        debug!("Adresse statique de donation ajoutée : {}", addresses.last().unwrap());
        if let Some(parent) = donate_list_path.parent() {
            fs::create_dir_all(parent)?;
            debug!("Parent directory pour donation list créé : {}", parent.display());
        }
        fs::write(donate_list_path, addresses.join("\n"))?;
        fs::write(donate_seeds_path, seeds.join("\n"))?;
        donate_addresses = addresses;
        info!("💾 [{}] Fichiers de donation créés :\n - {}\n - {}", instance_id, donate_list_path.display(), donate_seeds_path.display());
        debug!("donate_addresses final = {:?}", donate_addresses);
    }

    // --- Threads ---
    let total_threads = env::var("MINER_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(num_cpus::get);
    let threads_per_wallet = std::cmp::max(total_threads / wallets.len(), 1);
    info!(
        "🧠 [{}] Configuration: {} wallets, {} threads totaux → {} threads/wallet",
        instance_id, wallets.len(), total_threads, threads_per_wallet
    );
    debug!("total_threads={}, threads_per_wallet={}", total_threads, threads_per_wallet);

    let end_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
    info!("Date de fin de saison : {}", end_date);
    debug!("end_date set to {}", end_date);

    // --- Lancement des mineurs ---
    for (idx, wallet) in wallets.into_iter().enumerate() {
        let client_clone = client.clone();
        let donate_list = donate_addresses.clone();
        let instance_clone = instance_id.clone();
        let wallet_idx = idx + 1;
        debug!("Spawning task for wallet_idx={}, address={}", wallet_idx, &wallet.address);

        tokio::spawn(async move {
            let wallet_prefix = format!("[inst:{}|wallet-{}|{}]", instance_clone, wallet_idx, &wallet.address[..10]);
            info!("{} ⛏️ Miner démarré avec {} threads", wallet_prefix, threads_per_wallet);
            debug!("{} – donate_list length = {}", wallet_prefix, donate_list.len());

            // RNG Send‑safe
            let mut rng = StdRng::from_entropy();
            debug!("{} – StdRng initialisé", wallet_prefix);

            // Enregistrement du wallet
            if let Ok(terms) = client_clone.get_terms(None).await {
                debug!("{} – Terms récupérés: {:?}", wallet_prefix, terms);
                let signature = wallet.sign_cip30(&terms.message);
                let pubkey = wallet.public_key_hex();
                debug!("{} – signature générée, pubkey={}", wallet_prefix, pubkey);
                if let Err(e) = client_clone.register_address(&wallet.address, &signature, &pubkey).await {
                    warn!("{} ⚠️ Erreur enregistrement: {}", wallet_prefix, e);
                } else {
                    info!("{} ✅ Adresse enregistrée", wallet_prefix);
                }
            } else {
                warn!("{} ⚠️ Impossible de récupérer les termes d’enregistrement", wallet_prefix);
            }

            loop {
                if Utc::now().date_naive() > end_date {
                    info!("{} 💤 Fin de saison, attente...", wallet_prefix);
                    sleep(Duration::from_secs(3600)).await;
                    continue;
                }

                match client_clone.get_challenge().await {
                    Ok(resp) => {
                        debug!("{} – réponse challenge reçue: {:?}", wallet_prefix, resp);
                        match resp.code.as_str() {
                            "before" => {
                                debug!("{} Challenge pas encore dispo", wallet_prefix);
                                sleep(Duration::from_secs(60)).await;
                            }
                            "after" => {
                                info!("{} 🏁 Saison terminée", wallet_prefix);
                                sleep(Duration::from_secs(3600)).await;
                            }
                            "active" => {
                                if let Some(challenge) = resp.challenge {
                                    info!(
                                        "{} 🔥 Challenge actif ID={} diff={}",
                                        wallet_prefix,
                                        challenge.challenge_id,
                                        challenge.difficulty.clone().unwrap_or_default()
                                    );
                                    debug!("{} – details challenge: {:?}", wallet_prefix, challenge);

                                    let miner_config = MinerConfig {
                                        address: wallet.address.clone(),
                                        challenge: Arc::new(challenge.clone()),
                                    };
                                    debug!("{} – miner_config créé: {:?}", wallet_prefix, miner_config);
                                    let start = Instant::now();
                                    if let Ok(result) = mine(miner_config, threads_per_wallet) {
                                        let duration = start.elapsed();
                                        info!("{} 💎 Nonce trouvé={} en {:.2?}", wallet_prefix, result.nonce, duration);
                                        debug!("{} – preimage length={}", wallet_prefix, result.preimage.len());

                                        if let Ok(submit_resp) = client_clone
                                            .submit_solution(&wallet.address, &challenge.challenge_id, &result.nonce, &result.preimage)
                                            .await
                                        {
                                            debug!("{} – réponse submit_solution: {:?}", wallet_prefix, submit_resp);
                                            if let Some(receipt) = submit_resp.crypto_receipt {
                                                info!("{} ✅ Solution acceptée ! ts={} sig={}",
                                                    wallet_prefix, receipt.timestamp, &receipt.signature[..16.min(receipt.signature.len())]);
                                                debug!("{} – full signature={:?}", wallet_prefix, receipt.signature);

                                                // --- Donation aléatoire ---
                                                if let Some(dest) = donate_list.choose(&mut rng) {
                                                    debug!("{} – adresse de donation sélectionnée: {}", wallet_prefix, dest);
                                                    if let Err(e) = client_clone.donate_to(dest, &wallet.address, &receipt.signature).await {
                                                        warn!("{} ⚠️ Erreur donation à {} : {}", wallet_prefix, dest, e);
                                                    } else {
                                                        info!("{} 💝 Donation envoyée à {}", wallet_prefix, dest);
                                                    }
                                                } else {
                                                    warn!("{} ⚠️ Liste de donation vide, aucune donation envoyée", wallet_prefix);
                                                }
                                            } else {
                                                warn!("{} Soumission acceptée sans reçu", wallet_prefix);
                                            }
                                        } else {
                                            error!("{} ❌ Erreur soumission", wallet_prefix);
                                        }
                                    } else {
                                        error!("{} ❌ Erreur de minage", wallet_prefix);
                                    }
                                } else {
                                    warn!("{} Aucun challenge reçu", wallet_prefix);
                                }
                            }
                            other => {
                                warn!("{} Code inattendu: {}", wallet_prefix, other);
                            }
                        }
                    }
                    Err(e) => {
                        error!("{} ⚠️ Erreur récupération challenge: {}", wallet_prefix, e);
                    }
                }

                sleep(Duration::from_secs(10)).await;
            }
        });
    }

    info!("Entrée dans boucle de maintien infinie");
    loop {
        sleep(Duration::from_secs(3600)).await;
    }
}
