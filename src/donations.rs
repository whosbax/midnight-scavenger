use std::{collections::HashSet, fs, path::Path};
use serde::{Serialize, Deserialize};
use log::{info, warn};

#[derive(Serialize, Deserialize, Default)]
pub struct DonationRegistry {
    pub completed: HashSet<(String, String)>, // (original, destination)
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

    /// Vérifie si une donation a déjà été effectuée
    pub fn already_done(&self, orig: &str, dest: &str) -> bool {
        self.completed.contains(&(orig.to_string(), dest.to_string()))
    }

    /// Enregistre une donation comme réussie
    pub fn mark_done(&mut self, orig: &str, dest: &str) {
        self.completed.insert((orig.to_string(), dest.to_string()));
    }
}
