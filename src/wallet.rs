use std::fs;
use std::path::Path;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use ed25519_dalek::{Signer, SigningKey};
use bip39::{Mnemonic, Language};
use hex;
use zeroize::Zeroize;
use bech32::{ToBase32, Variant};
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use ciborium::value::{Value, Integer};
use serde_cbor::to_vec;
use log::info;

/// Représente un wallet Ed25519 avec adresse Shelley Bech32
#[derive(Clone)]
pub struct Wallet {
    signing_key: SigningKey,
    pub address: String, // adresse Bech32 (mainnet ou testnet)
    pub mnemonic: Option<String>, // seed phrase optionnelle pour régénération
}

impl Wallet {
    pub fn signing_key_hex(&self) -> String {
        hex::encode(self.signing_key.to_bytes())
    }    
    /// Génère un nouveau wallet Ed25519 aléatoire sans seed BIP39
    pub fn generate(use_mainnet: bool) -> Self {
        let mut rng = ChaCha20Rng::from_entropy();
        let mut entropy = [0u8; 32];
        rng.fill_bytes(&mut entropy);

        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .expect("Erreur génération BIP-39");

        let phrase = mnemonic.to_string();
        let seed_full = mnemonic.to_seed("");
        let seed_bytes = &seed_full[0..32];

        let mut sk_bytes = [0u8; 32];
        sk_bytes.copy_from_slice(seed_bytes);
        let signing_key = SigningKey::from_bytes(&sk_bytes);
        let pubkey_bytes = signing_key.verifying_key().to_bytes();
        let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);

        sk_bytes.zeroize();
        entropy.zeroize();

        Self {
            signing_key,
            address: addr,
            mnemonic: Some(phrase),
        }
    }

    /// Génère un wallet depuis une seed BIP-39 (24 mots)
    pub fn generate_from_bip39(
        seed_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
        use_mainnet: bool,
    ) -> Self {
        // Génération d'entropie sécurisée (32 octets pour 24 mots)
        let mut rng = ChaCha20Rng::from_entropy();
        let mut entropy = [0u8; 32];
        rng.fill_bytes(&mut entropy);

        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .expect("Erreur génération BIP-39");

        let phrase = mnemonic.to_string();
        let seed_full = mnemonic.to_seed("");
        let seed_bytes = &seed_full[0..32];

        let mut sk_bytes = [0u8; 32];
        sk_bytes.copy_from_slice(seed_bytes);
        let signing_key = SigningKey::from_bytes(&sk_bytes);
        let pubkey_bytes = signing_key.verifying_key().to_bytes();
        let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);

        // Écriture des fichiers
        fs::write(&seed_path, &phrase).expect("Impossible d’écrire la seed");
        fs::write(&key_path, hex::encode(signing_key.to_bytes()))
            .expect("Impossible d’écrire la clé privée");

        sk_bytes.zeroize();
        entropy.zeroize();

        info!("🔐 Wallet généré depuis BIP-39 : {}", addr);
        Self {
            signing_key,
            address: addr,
            mnemonic: Some(phrase),
        }
    }

    /// Charge un wallet depuis un fichier clé privée hex
    pub fn load_from_file(
        key_path: impl AsRef<Path>,
        use_mainnet: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>  {
    //) -> Result<Self, Box<dyn std::error::Error>> {
        let hex_str = fs::read_to_string(key_path)?;
        let bytes = hex::decode(hex_str.trim())?;
        if bytes.len() != 32 {
            return Err("La clé privée doit faire 32 octets".into());
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let pubkey_bytes = signing_key.verifying_key().to_bytes();
        let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);
        key_bytes.zeroize();

        Ok(Self {
            signing_key,
            address: addr,
            mnemonic: None,
        })
    }

    /// Retourne la clé publique au format hex
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Signe un message arbitraire
    pub fn sign(&self, message: &str) -> String {
        let sig = self.signing_key.sign(message.as_bytes());
        hex::encode(sig.to_bytes())
    }

    /// Signe un message selon CIP-8 / CIP-30
    pub fn sign_cip30(&self, message: &str) -> String {
        // Protected header (alg = EdDSA)
        let protected = to_vec(&Value::Map(vec![(
            Value::Integer(Integer::from(1i64)),
            Value::Integer(Integer::from(-8i64)),
        )]))
        .unwrap();

        let to_sign = to_vec(&Value::Array(vec![
            Value::Text("Signature1".into()),
            Value::Bytes(protected.clone()),
            Value::Bytes(Vec::new()), // external_aad
            Value::Bytes(message.as_bytes().to_vec()),
        ]))
        .unwrap();

        let sig = self.signing_key.sign(&to_sign);
        let cose = to_vec(&Value::Array(vec![
            Value::Bytes(protected),
            Value::Map(vec![]),
            Value::Bytes(message.as_bytes().to_vec()),
            Value::Bytes(sig.to_bytes().to_vec()),
        ]))
        .unwrap();

        hex::encode(cose)
    }

    /// Décode l’adresse Bech32 en bytes
    pub fn address_bytes(&self) -> Vec<u8> {
        let (_hrp, data, _variant) = bech32::decode(&self.address).expect("Erreur décodage Bech32");
        bech32::FromBase32::from_base32(&data).expect("Erreur conversion from base32")
    }

    /// Sauvegarde une seule clé privée dans un fichier (hex)
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let hex_str = hex::encode(self.signing_key.to_bytes());
        fs::write(path, hex_str)?;
        Ok(())
    }

    /// Dérive une adresse Shelley Bech32 à partir de pubkey
    fn derive_bech32_address(pubkey: &[u8], use_mainnet: bool) -> String {
        let mut hasher = Blake2bVar::new(28).expect("Erreur Blake2bVar");
        hasher.update(pubkey);
        let mut key_hash = vec![0u8; 28];
        hasher.finalize_variable(&mut key_hash).expect("Erreur hash");

        let header: u8 = if use_mainnet { 0b0110_0001 } else { 0b0110_0000 };
        let mut addr_bytes = Vec::with_capacity(1 + key_hash.len());
        addr_bytes.push(header);
        addr_bytes.extend_from_slice(&key_hash);

        let prefix = if use_mainnet { "addr" } else { "addr_test" };
        bech32::encode(prefix, addr_bytes.to_base32(), Variant::Bech32)
            .expect("Erreur encodage Bech32")
    }

    // === Multi-wallet persistence ===
pub fn save_many_to_files(
    wallets: &[Wallet],
    seed_path: &Path,
    key_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut seeds = Vec::new();
    let mut keys = Vec::new();

    for w in wallets {
        // Si pas de mnemonic, en génère une proprement
        let mnemonic_str = if let Some(m) = &w.mnemonic {
            m.clone()
        } else {
            let mut rng = ChaCha20Rng::from_entropy();
            let mut entropy = [0u8; 32];
            rng.fill_bytes(&mut entropy);
            let m = Mnemonic::from_entropy_in(Language::English, &entropy)?;
            entropy.zeroize();
            m.to_string()
        };

        seeds.push(mnemonic_str.clone());
        keys.push(hex::encode(w.signing_key.to_bytes()));
    }

    fs::write(seed_path, seeds.join("\n"))?;
    fs::write(key_path, keys.join("\n"))?;
    Ok(())
}

    pub fn load_many_from_files(
        seed_path: &Path,
        key_path: &Path,
        use_mainnet: bool,
    ) -> Result<Vec<Wallet>, Box<dyn std::error::Error + Send + Sync>>{
    //) -> Result<Vec<Wallet>, Box<dyn std::error::Error>> {
        let seeds_str = fs::read_to_string(seed_path)?;
        let keys_str = fs::read_to_string(key_path)?;
        let seed_lines: Vec<_> = seeds_str.lines().collect();
        let key_lines: Vec<_> = keys_str.lines().collect();

        let mut wallets = Vec::new();
        for (seed_phrase, _key_hex) in seed_lines.iter().zip(key_lines.iter()) {
            let mnemonic = Mnemonic::parse_in_normalized(Language::English, seed_phrase)?;
            let seed_full = mnemonic.to_seed("");
            let mut sk_bytes = [0u8; 32];
            sk_bytes.copy_from_slice(&seed_full[..32]);

            let signing_key = SigningKey::from_bytes(&sk_bytes);
            let pubkey_bytes = signing_key.verifying_key().to_bytes();
            let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);

            wallets.push(Wallet {
                signing_key,
                address: addr,
                mnemonic: Some(seed_phrase.to_string()),
            });
        }

        Ok(wallets)
    }
}
