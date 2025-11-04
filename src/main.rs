mod api_client;
mod miner;

use std::error::Error;
use std::env;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use api_client::ApiClient;
use miner::{MinerConfig, mine};
use num_cpus;
use chrono::{Utc, NaiveDate};
mod keys;
use keys::Wallet;
use std::sync::Arc;
use log::{info, debug, error};
use env_logger;
use rand;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Application démarrée");
    let args: Vec<String> = env::args().collect();

    let base_url = env::var("APP_BASE_URL")
        .unwrap_or_else(|_| "https://scavenger.prod.gd.midnighttge.io".to_string());
    debug!("API base URL: {}", base_url);
    let client = ApiClient::new(&base_url)?;

    // --- 1. Gestion des clés persistées via volume Docker ---
    let str_k_path = env::var("APP_WALLET_KEY_HEX_PATH")
        .unwrap_or_else(|_| "/usr/local/bin/config/mykey.hex".to_string());
    let str_seed_path = env::var("APP_WALLET_SEED_PATH")
        .unwrap_or_else(|_| "/usr/local/bin/config/seed.txt".to_string());

    let key_path = Path::new(&str_k_path);
    let seed_path = Path::new(&str_seed_path);
    let use_mainnet = true;
    let wallet = if key_path.exists() && seed_path.exists() {
        info!("Clé et seed existantes détectées. Chargement depuis '{}'", key_path.display());
        Wallet::load_from_file(key_path, use_mainnet)?
    } else if args.contains(&"--generate-seed".to_string()) {
        info!("Aucune clé détectée. Génération d'une nouvelle clé depuis BIP-39...");
        let wallet = Wallet::generate_from_bip39(seed_path, key_path, use_mainnet);
        info!("Seed générée et sauvegardée dans '{}'", seed_path.display());
        info!("Clé privée sauvegardée dans '{}'", key_path.display());
        wallet
    } else {
        info!("Aucune clé détectée. Génération d'une nouvelle clé Ed25519 aléatoire...");
        let wallet = Wallet::generate(use_mainnet);
        wallet.save_to_file(key_path)?;
        info!("Clé générée et sauvegardée dans '{}'", key_path.display());
        wallet
    };
    info!("Adresse utilisée : {}", wallet.address);
    debug!("Clé publique hex: {}", wallet.public_key_hex());
    debug!("Adresse bytes: {:02x?}", wallet.address_bytes());

    if args.contains(&"--no-api-call".to_string()) {
        info!("Mode --no-api-call activé, sortie immédiate.");
        return Ok(());
    }

    // --- 2. Mode test wallet rapide ---
    if args.contains(&"--test-wallet".to_string()) {
        info!("=== Test wallet terminé avec succès ===");
        info!("Clé publique : {}", wallet.public_key_hex());
        info!("Signature test : {}", wallet.sign("test_message"));
        return Ok(());
    }

    // --- 3. Enregistrement automatique si nécessaire ---
    match client.get_terms(None).await {
        Ok(terms) => {
            info!("Terms : '{}'", &terms.message);
            let address = &wallet.address;
            let signature = wallet.sign_cip30(&terms.message);
            let pubkey = wallet.public_key_hex();
            let address_bytes = wallet.address_bytes();
            debug!("wallet.address_bytes() length = {}", address_bytes.len());
            debug!("wallet.address_bytes() hex = {}", hex::encode(&address_bytes));
            debug!("wallet.address = {}", wallet.address);
            debug!("pubkey hex = {}", pubkey);
            debug!("signature length = {}", signature.len());
            debug!("signature hex prefix = {}", &signature[..32.min(signature.len())]);
            match client.register_address(&address, &signature, &pubkey).await {
                Ok(resp) => {
                    info!("Adresse enregistrée avec succès !");
                    info!("Preimage: {}", resp.registration_receipt.preimage);
                    info!("Signature: {}", resp.registration_receipt.signature);
                    info!("Timestamp: {}", resp.registration_receipt.timestamp);
                }
                Err(e) => {
                    error!("Erreur lors de l'enregistrement (peut-être déjà enregistré) : {}", e);
                }
            }
        }
        Err(e) => {
            error!("Impossible de récupérer les T&C : {}", e);
        }
    }
    // --- 3bis. Mode donation (fichier ou paramètre CLI) ---    

    let donate_list_str = env::var("DONATE_LIST_PATH")
        .unwrap_or_else(|_| "/usr/local/bin/config/donate_list.txt".to_string());
    let donate_list_path = Path::new(&donate_list_str);
    let mut donate_addresses: Vec<String> = Vec::new();

    if donate_list_path.exists() {
        match fs::read_to_string(donate_list_path) {
            Ok(contents) => {
                donate_addresses = contents
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
                    .collect();
                info!("Liste de donation chargée depuis '{}': {} adresses", donate_list_path.display(), donate_addresses.len());
            }
            Err(e) => {
                error!("Erreur de lecture de la liste de donation : {}", e);
            }
        }
    }

    // Détermination de l'adresse destination
    let destination_opt = if !donate_addresses.is_empty() {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        donate_addresses.choose(&mut rng).cloned()
    } else if let Some(index) = args.iter().position(|x| x == "--donate-to") {
        args.get(index + 1).cloned()
    } else {
        None
    };

    if let Some(destination) = destination_opt {
        if destination.trim().is_empty() {
            error!("Adresse de destination vide ou invalide !");
        } else {
            info!("=== Mode donation / consolidation ===");
            info!("Destination sélectionnée : {}", destination);

            let message = format!("Assign accumulated Scavenger rights to: {}", destination);
            let signature = wallet.sign_cip30(&message);

            match client
                .donate_to(&base_url, &destination, &wallet.address, &signature)
                .await
            {
                Ok(resp) => {
                    info!("✅ Donation réussie !");
                    info!(
                        "Message : {}\nDonation ID : {:?}\nSolutions consolidées : {:?}",
                        resp.message.unwrap_or_default(),
                        resp.donation_id,
                        resp.solutions_consolidated
                    );
                    return Ok(());
                }
                Err(e) => {
                    error!("❌ Erreur donation : {}", e);
                    return Ok(());
                }
            }
        }
    } else {
        info!("Aucune donation à effectuer (liste vide et aucun paramètre --donate-to).");
    }

    // --- 4. Boucle principale de minage ---
    let num_threads: usize = env::var("MINER_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| num_cpus::get());

    info!("Démarrage du minage sur {} threads...", num_threads);

    let end_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();

    loop {
        let today = Utc::now().date_naive();
        if today > end_date {
            info!("Fin des challenges. Attente active en boucle...");
            sleep(Duration::from_secs(3600)).await;
            continue;
        }

        match client.get_challenge().await {
            Ok(resp) => match resp.code.as_str() {
                "before" => {
                    info!(
                        "Les challenges n'ont pas encore commencé. Issued at : {:?}",
                        resp.challenge.as_ref().and_then(|c| c.issued_at.clone())
                    );
                    sleep(Duration::from_secs(60)).await;
                    continue;
                }
                "after" => {
                    info!("Les challenges sont terminés pour cette saison. Attente active...");
                    sleep(Duration::from_secs(3600)).await;
                    continue;
                }
                "active" => {
                    if let Some(challenge) = resp.challenge {
                        info!(
                            "Challenge actif : {} (day {}, challenge {})",
                            challenge.challenge_id,
                            challenge.day.unwrap_or(0),
                            challenge.challenge_number.unwrap_or(0)
                        );
                        info!("Challenge difficulty: {}", challenge.difficulty.clone().unwrap_or_default());
                        debug!("Challenge full data: {:?}", challenge);

                        // Conversion en Arc pour MinerConfig
                        let miner_config = MinerConfig {
                            address: wallet.address.clone(),
                            challenge: Arc::new(challenge.clone()),
                        };

                        let start = Instant::now();
                        match mine(miner_config, num_threads) {
                            Ok(result) => {
                                let duration = start.elapsed();
                                info!("Nonce trouvé : {} en {:.2?}", result.nonce, duration);
                                info!("Submitting solution → address={}, challenge_id={}, nonce={}", &wallet.address, &challenge.challenge_id, &result.nonce);
                                info!(
                                    "🧩 Submission details:\n  Address: {}\n  Challenge ID: {}\n  Nonce: {}\n  Preimage: {}",
                                    &wallet.address,
                                    &challenge.challenge_id,
                                    &result.nonce,
                                    &result.preimage
                                );
                                match client
                                    .submit_solution(&wallet.address, &challenge.challenge_id, &result.nonce, &result.preimage)
                                    .await
                                {
                                    Ok(submit_resp) => {
                                        if let Some(receipt) = submit_resp.crypto_receipt {
                                            info!("Solution acceptée !");
                                            info!("Crypto receipt timestamp : {}", receipt.timestamp);
                                            info!("Crypto receipt signature : {}", receipt.signature);
                                        } else {
                                            info!(
                                                "Aucune crypto_receipt renvoyée : {:?}",
                                                submit_resp.message
                                            );
                                        }
                                    }
                                    Err(e) => error!("Erreur lors de la soumission : {}", e),
                                }
                            }
                            Err(e) => {
                                error!("Erreur lors du minage : {}", e);
                            }
                        }

                    } else {
                        error!("Challenge actif mais aucune donnée reçue. Réessai dans 10s...");
                    }
                }
                other => error!("Code inattendu du challenge : {}", other),
            },
            Err(e) => error!("Erreur lors de la récupération du challenge : {}", e),
        }

        sleep(Duration::from_secs(10)).await;
    }
}
