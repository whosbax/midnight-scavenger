use std::{fs, path::Path, sync::Arc};
use log::{info, warn, error, debug};
use rand::{seq::SliceRandom, rngs::StdRng, SeedableRng};
use std::collections::HashMap;
use crate::api_client::ApiClient;
use crate::wallet::Wallet;
use crate::WalletContainer;
use crate::donations::DonationRegistry;
use parking_lot::RwLock;

/// Charge ou crée la liste d’adresses de donation
pub fn load_or_create_donate_addresses(config_root: &str, use_mainnet: bool, instance_id: &str) -> Vec<String> {
    debug!("🔍 [{}] Chargement des adresses de donation depuis {}", instance_id, config_root);
    let donate_list_path = Path::new(config_root).join("donate_list.txt");
    let donate_seeds_path = Path::new(config_root).join("donate_list_seed.txt");

    let mut donate_addresses: Vec<String> = Vec::new();

    if donate_list_path.exists() {
        debug!("📄 [{}] Fichier donate_list.txt trouvé : {:?}", instance_id, donate_list_path);
        if let Ok(contents) = fs::read_to_string(&donate_list_path) {
            donate_addresses = contents
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            info!("💰 [{}] Liste de donation chargée ({} adresses)", instance_id, donate_addresses.len());
        } else {
            warn!("⚠️ [{}] Impossible de lire la liste de donation, tentative de recréation...", instance_id);
        }
    } else {
        warn!("⚠️ [{}] Aucun fichier donate_list.txt trouvé", instance_id);
    }

    if donate_addresses.is_empty() {
        warn!("⚠️ [{}] Pas de liste de donation trouvée, création automatique...", instance_id);

        let mut seeds = Vec::new();
        let mut addresses = Vec::new();

        for i in 0..3 {
            let w = Wallet::generate(use_mainnet);
            debug!("🪙 [{}] Wallet de donation {} généré: {}", instance_id, i + 1, w.address);
            seeds.push(w.mnemonic.clone().unwrap_or_default());
            addresses.push(w.address.clone());
        }

        // Adresse fallback connue
        let fallback = "addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h".to_string();
        addresses.push(fallback.clone());
        debug!("🧩 [{}] Adresse fallback ajoutée : {}", instance_id, fallback);

        if let Err(e) = fs::write(&donate_list_path, addresses.join("\n")) {
            warn!("❌ [{}] Impossible d’écrire donate_list.txt: {}", instance_id, e);
        }
        if let Err(e) = fs::write(&donate_seeds_path, seeds.join("\n")) {
            warn!("❌ [{}] Impossible d’écrire donate_list_seed.txt: {}", instance_id, e);
        }

        info!("💾 [{}] Fichiers de donation créés ({} adresses)", instance_id, addresses.len());
        donate_addresses = addresses;
    }

    debug!("📦 [{}] Liste finale de donation: {:?}", instance_id, donate_addresses);
    donate_addresses
}

/// Fonction principale pour exécuter les donations pour tous les wallets
pub async fn process_donations_for_wallets(
    client: Arc<ApiClient>,
    wallets_path: &str,
    donate_addresses: &[String],
    instance_id: &str,
    uniq_inst_id: &str,
) {
    info!("🚀 [{}] Démarrage du processus de donation...", instance_id);


    let base_path = Path::new("./config");

    // Limite supérieure configurable
    let max_id = 100;
                    // --- Ajout des statistiques locales ---
                    let mut total_attempts = 0usize;
                    let mut total_success = 0usize;
                    let mut total_fail = 0usize;
                    let mut error_stats: HashMap<String, usize> = HashMap::new();
        let donate_registry_path = Path::new("/usr/local/bin/config/donations_log.json");
        let mut donation_registry = DonationRegistry::load(donate_registry_path);
        info!("📒 [{}] Registre de donations chargé : {} entrées", instance_id, donation_registry.completed.len());                    
    for id in 1..=max_id {
        let id_str = id.to_string();

        // Construit les chemins d'intérêt
        let seeds_path = base_path.join(&id_str).join(format!("miner-{id}/wallets/seeds.txt"));
        let keys_path = base_path.join(&id_str).join(format!("miner-{id}/wallets/keys.hex"));


        let mut rng = StdRng::from_entropy();

        if seeds_path.exists() && keys_path.exists() {
            debug!("🔧 Config valide pour miner-{id}:");
            debug!("   -> {:?}", seeds_path);
            debug!("   -> {:?}", keys_path);

            match Wallet::load_many_from_files(&seeds_path, &keys_path, true) {
                Ok(w) => {
                    let container = WalletContainer::new(w, seeds_path.clone(), keys_path.clone(), true);
                    let w_list = Arc::new(container);
                    let wallets = w_list.read_all();

                    debug!("💼 [{}] {} wallets chargés pour rediriger les donations", instance_id, wallets.len());



                    for (_idx, wallet) in wallets.into_iter().enumerate() {
                        debug!("🔓 [{}] Wallet chargé: {}", instance_id, wallet.address);

                        if donation_registry.is_wallet_assigned(&wallet.address) {
                            debug!("🔁 [{}] Wallet {} déjà assigné à une donation, skip.", instance_id, wallet.address);
                            continue;
                        }

                        if let Some(dest) = donate_addresses.choose(&mut rng) {
                            debug!("🎯 [{}] Adresse de destination choisie: {}", instance_id, dest);
                            if dest == &wallet.address {
                                debug!("⛔ [{}] Auto-donation détectée, ignorée pour {}", instance_id, wallet.address);
                                continue;
                            }

                            //let message = format!("Assign accumulated Scavenged NIGHT to: {}", dest);
                            let message = format!("\"Assign accumulated Scavenger rights to: {}\"", dest);
                            let pubkey = wallet.public_key_hex();
                            let signature = wallet.sign_cip30(&message);
                            let signature_8 = match wallet.sign_cip8(&message, &[]) {
                                Ok(sig) => sig,
                                Err(err) => {
                                    eprintln!("Erreur signature CIP8 : {:?}", err);
                                    return;
                                },
                            };
                            debug!("✍️ Start donation      ");
                            debug!("   ✍️ Entreprise        : [{}]", wallet.address);
                            debug!("   ✍️ Shelley Base      : [{}]", wallet.shelley_addr);
                            debug!("   ✍️ Donate to addr    : [{}]", dest);
                            debug!("   ✍️ Pub key Hex       : [{}]", pubkey);
                            debug!("   ✍️ Message plain text: [{}]", message);
                            debug!("   ✍️ CIP_30 sig        : [{}]", signature);
                            debug!("   ✍️ CIP_8  sig        : [{}]", signature_8);

                            info!("✍️ [{}] Signature créée pour donation {} → {}", instance_id, wallet.address, dest);

                            total_attempts += 1;

                            match client
                                .donate_to(dest, &wallet.shelley_addr, &signature_8, Some(instance_id.to_string()), Some(uniq_inst_id.to_string()))
                                .await
                            {
                                Ok(resp) => {
                                    total_success += 1;
                                    info!(
                                        "✅ [{}] Donation réussie de {} → {} | status: {:?}",
                                        instance_id, wallet.address, dest, resp.status
                                    );
                                    donation_registry.mark_done(&wallet.address, dest);
                                    donation_registry.save(donate_registry_path);
                                    debug!("🧾 [{}] Registre de donation mis à jour", instance_id);
                                }
                                Err(e) => {
                                    total_fail += 1;
                                    let err_msg = e.to_string();
                                    *error_stats.entry(err_msg).or_insert(0) += 1;
                                    debug!("⚠️ [{}] Échec donation {} → {} : {}", instance_id, wallet.address, dest, e);
                                }
                            }
                        } else {
                            warn!("⚠️ [{}] Aucune adresse de donation valide disponible", instance_id);
                        }
                    }


                }

                Err(e) => {
                    error!("❌ [{}] Impossible de charger wallet {:?} : {}", instance_id, seeds_path, e);
                }
            }
        } else {
            debug!("⏭️ Config incomplète ou absente pour miner-{id}");
        }
    }

                    // --- Résumé des stats pour miner-{id} ---
                    info!("📊 Résumé donations :");
                    info!("   Tentatives totales : {}", total_attempts);
                    info!("   Succès             : {}", total_success);
                    info!("   Échecs             : {}", total_fail);

                    if !error_stats.is_empty() {
                        info!("   Erreurs distinctes :");
                        for (err, count) in error_stats {
                            info!("     - {} ({}x)", err, count);
                        }
                    }
    info!("🏁 [{}] Fin du cycle de donation", instance_id);
}
