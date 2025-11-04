// src/keys.rs
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

/// Représente un wallet Ed25519 avec adresse Shelley Bech32
pub struct Wallet {
    signing_key: SigningKey,
    pub address: String, // adresse Bech32 (mainnet ou testnet selon usage)
}

impl Wallet {
    /// Génère un nouveau wallet Ed25519 aléatoire
    pub fn generate(use_mainnet: bool) -> Self {
        let mut csprng = ChaCha20Rng::from_entropy();
        let mut secret_bytes = [0u8; 32];
        csprng.fill_bytes(&mut secret_bytes);

        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let pubkey_bytes = signing_key.verifying_key().to_bytes();
        let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);
        secret_bytes.zeroize();

        Self { signing_key, address: addr }
    }

    /// Génère un wallet depuis une seed BIP‑39 (24 mots)
    pub fn generate_from_bip39(
        seed_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
        use_mainnet: bool
    ) -> Self {
        let mut rng = ChaCha20Rng::from_entropy();
        let mut entropy = [0u8; 32];
        rng.fill_bytes(&mut entropy);

        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .expect("Erreur lors de génération du mnemonic");
        let phrase = mnemonic.to_string();

        let seed_full = mnemonic.to_seed("");
        let seed_bytes = &seed_full[0..32];
        let mut sk_bytes = [0u8; 32];
        sk_bytes.copy_from_slice(seed_bytes);

        let signing_key = SigningKey::from_bytes(&sk_bytes);
        let pubkey_bytes = signing_key.verifying_key().to_bytes();
        let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);

        fs::write(&seed_path, &phrase).expect("Impossible d’écrire la seed");
        fs::write(&key_path, hex::encode(signing_key.to_bytes()))
            .expect("Impossible d’écrire la clé privée");

        sk_bytes.zeroize();

        Self { signing_key, address: addr }
    }

    /// Charge un wallet depuis un fichier clé privée hex
    pub fn load_from_file(
        key_path: impl AsRef<Path>,
        use_mainnet: bool
    ) -> Result<Self, Box<dyn std::error::Error>> {
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
        Ok(Self { signing_key, address: addr })
    }

    /// Retourne la clé publique au format hex (64 caractères = 32 octets)
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Signe un message arbitraire (non CIP‑8) et retourne la signature hex
    pub fn sign(&self, message: &str) -> String {
        let sig = self.signing_key.sign(message.as_bytes());
        hex::encode(sig.to_bytes())
    }

    /// Signe un message selon CIP‑8 / CIP‑30 (COSE_Sign1) et retourne hex‑encode
    pub fn sign_cip30(&self, message: &str) -> String {
        // Protected header (alg = EdDSA → label 1 = -8)
        let protected = to_vec(&Value::Map(vec![
            ( Value::Integer(Integer::from(1i64)) , Value::Integer(Integer::from(-8i64)) )
        ])).unwrap();

        // Build Sig_structure as per CIP‑8
        let to_sign = to_vec(&Value::Array(vec![
            Value::Text("Signature1".into()),
            Value::Bytes(protected.clone()),
            Value::Bytes(Vec::new()), // external_aad
            Value::Bytes(message.as_bytes().to_vec()),
        ])).unwrap();

        let sig = self.signing_key.sign(&to_sign);
        let cose = to_vec(&Value::Array(vec![
            Value::Bytes(protected),
            Value::Map(vec![]),
            Value::Bytes(message.as_bytes().to_vec()),
            Value::Bytes(sig.to_bytes().to_vec()),
        ])).unwrap();

        hex::encode(cose)
    }

    /// Decode l’adresse Bech32 en bytes
    pub fn address_bytes(&self) -> Vec<u8> {
        let (_hrp, data, _variant) = bech32::decode(&self.address)
            .expect("Erreur décodage Bech32");
        bech32::FromBase32::from_base32(&data)
            .expect("Erreur conversion from base32")
    }
    /// Sauvegarde la clé privée dans un fichier (hex) 
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> 
    { 
        let hex_str = hex::encode(self.signing_key.to_bytes()); 
        fs::write(path, hex_str)?; 
        Ok(()) 
    }
    /// Dérive une adresse Shelley Bech32 à partir de pubkey. use_mainnet = true → “addr”, false → “addr_test”
    fn derive_bech32_address(pubkey: &[u8], use_mainnet: bool) -> String {
        // Blake2b‑224 (28 octets) du pubkey
        let mut hasher = Blake2bVar::new(28).expect("Erreur initialisation Blake2bVar");
        hasher.update(pubkey);
        let mut key_hash = vec![0u8; 28];
        hasher.finalize_variable(&mut key_hash).expect("Erreur finalisation hash");

        // Header Shelley : base addr + network bit
        // mainsnet network id = 1, testnet = 0 → header bit = 0b0110_0001 for mainnet, 0b0110_0000 for testnet
        let header: u8 = if use_mainnet { 0b0110_0001 } else { 0b0110_0000 };

        let mut addr_bytes = Vec::with_capacity(1 + key_hash.len());
        addr_bytes.push(header);
        addr_bytes.extend_from_slice(&key_hash);

        let prefix = if use_mainnet { "addr" } else { "addr_test" };
        bech32::encode(prefix, addr_bytes.to_base32(), Variant::Bech32)
            .expect("Erreur encodage Bech32")
    }
}
