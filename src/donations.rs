use std::{collections::HashSet, fs, path::Path};
use serde::{Serialize, Deserialize};
use log::{warn};

#[derive(Serialize, Deserialize, Default)]
pub struct DonationRegistry {
    pub completed: HashSet<(String, String)>, // (original_wallet, destination_address)
}

impl DonationRegistry {
    /// Charge le registre depuis un fichier JSON (ou crée vide)
    pub fn load(path: &Path) -> Self {
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(reg) = serde_json::from_str(&text) {
                return reg;
            }
        }
        Self::default()
    }

    /// Sauvegarde le registre sur disque
    pub fn save(&self, path: &Path) {
        if let Err(e) = fs::write(path, serde_json::to_string_pretty(self).unwrap()) {
            warn!("⚠️ Impossible d’écrire le registre des donations: {}", e);
        }
    }

    /// Vérifie si une donation a déjà été effectuée pour une paire spécifique
    pub fn already_done(&self, orig: &str, dest: &str) -> bool {
        self.completed.contains(&(orig.to_string(), dest.to_string()))
    }

    /// Vérifie si un wallet a déjà été associé à une adresse de donation
    pub fn is_wallet_assigned(&self, orig: &str) -> bool {
        self.completed.iter().any(|(o, _)| o == orig)
    }

    /// Enregistre une donation comme réussie
    pub fn mark_done(&mut self, orig: &str, dest: &str) {
        self.completed.insert((orig.to_string(), dest.to_string()));
    }
}
