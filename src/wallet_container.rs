use std::path::{Path, PathBuf};
use std::fs::{self, OpenOptions};
use std::time::{Duration, Instant};
use std::thread::sleep;

use parking_lot::RwLock;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::sync::Arc;

use crate::wallet::Wallet;

/// Container thread-safe pour gérer plusieurs wallets par instance.
pub struct WalletContainer {
    wallets: Arc<RwLock<Vec<Wallet>>>,
    seeds_path: PathBuf,
    keys_path: PathBuf,
    use_mainnet: bool,
}

impl WalletContainer {
    /// Charge si possible depuis les fichiers ; sinon génère uniquement les manquants.
    pub fn load_or_create<P: AsRef<Path>>(
        seeds_path: P,
        keys_path: P,
        use_mainnet: bool,
        max_wallets: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let seeds_path = seeds_path.as_ref().to_path_buf();
        let keys_path = keys_path.as_ref().to_path_buf();

        if let Some(parent) = seeds_path.parent() { fs::create_dir_all(parent)?; }
        if let Some(parent) = keys_path.parent() { fs::create_dir_all(parent)?; }

        let mut wallets: Vec<Wallet> = Vec::new();

        // 🔹 Étape 1 : Charger les seeds existantes si elles existent
        if seeds_path.exists() && keys_path.exists() {
            match Wallet::load_many_from_files(&seeds_path, &keys_path, use_mainnet) {
                Ok(list) => {
                    log::info!("♻️  WalletContainer: {} wallets existants chargés", list.len());
                    wallets = list;
                }
                Err(e) => log::warn!("⚠️ WalletContainer: impossible de charger les fichiers existants: {}", e),
            }
        }

        let existing = wallets.len();

        // 🔹 Étape 2 : Compléter si besoin
        if existing < max_wallets {
            let to_generate = max_wallets - existing;
            log::info!("🪙 Génération de {} nouveaux wallets (déjà {} existants)", to_generate, existing);
            for _ in 0..to_generate {
                wallets.push(Wallet::generate(use_mainnet));
            }
        } else if existing > max_wallets {
            log::warn!(
                "⚠️ {} wallets existants mais max_wallets={} — aucun n’est supprimé (préservation)",
                existing,
                max_wallets
            );
        }

        let container = Self {
            wallets: Arc::new(RwLock::new(wallets)),
            seeds_path,
            keys_path,
            use_mainnet,
        };

        // 🔹 Étape 3 : Sauvegarder seulement si ajout de nouveaux wallets
        if existing < max_wallets {
            log::info!("💾 Sauvegarde des nouveaux wallets ajoutés...");
            container.save()?;
        }

        Ok(container)
    }

    /// Sauvegarde atomique et protégée par lock
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let lock_path = self.seeds_path.with_extension("lock");

        // Essayer d'obtenir le lock avec retry
        let start = Instant::now();
        let mut got_lock = OpenOptions::new().write(true).create_new(true).open(&lock_path).is_ok();
        while !got_lock && start.elapsed() < Duration::from_secs(5) {
            got_lock = OpenOptions::new().write(true).create_new(true).open(&lock_path).is_ok();
            if !got_lock {
                sleep(Duration::from_millis(100));
            }
        }

        if !got_lock {
            return Err(format!(
                "WalletContainer: impossible d'obtenir le lock pour {:?}",
                lock_path
            )
            .into());
        }

        let wallets = self.wallets.read();
        let seeds: Vec<String> = wallets
            .iter()
            .map(|w| w.mnemonic.clone().unwrap_or_default())
            .collect();
        let keys: Vec<String> = wallets.iter().map(|w| w.signing_key_hex()).collect();

        let seeds_tmp = self.seeds_path.with_extension("tmp");
        let keys_tmp = self.keys_path.with_extension("tmp");

        fs::write(&seeds_tmp, seeds.join("\n"))?;
        fs::write(&keys_tmp, keys.join("\n"))?;

        fs::rename(&seeds_tmp, &self.seeds_path)?;
        fs::rename(&keys_tmp, &self.keys_path)?;

        let _ = fs::remove_file(&lock_path);

        Ok(())
    }

    pub fn get_random(&self) -> Option<Wallet> {
        let wallets = self.wallets.read();
        wallets.choose(&mut thread_rng()).cloned()
    }

    pub fn get_by_index(&self, idx: usize) -> Option<Wallet> {
        let wallets = self.wallets.read();
        wallets.get(idx).cloned()
    }

    pub fn len(&self) -> usize {
        self.wallets.read().len()
    }

    pub fn read_all(&self) -> Vec<Wallet> {
        self.wallets.read().clone()
    }

    pub fn push_and_save(&self, w: Wallet) -> Result<(), Box<dyn std::error::Error>> {
        {
            self.wallets.write().push(w);
        }
        self.save()
    }
}
