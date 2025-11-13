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
use ed25519_dalek::Signature;
use serde_cbor::de::from_slice;

/// Représente un wallet Ed25519 avec adresse Shelley Bech32
#[derive(Clone)]
pub struct Wallet {
    signing_key: SigningKey,
    pub address: String,           // adresse Bech32 (mainnet ou testnet)
    pub mnemonic: Option<String>,  // seed phrase optionnelle pour régénération
    pub shelley_addr: String,      // adresse Shelley explicite (vide par défaut pour compatibilité)
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
            shelley_addr: String::new(),
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
            shelley_addr: String::new(),
        }
    }

    /// Génère un wallet depuis une phrase mnémonique donnée (méthode Shelley explicite de type base)
    pub fn generate_shelley_base_from_mnemonic_phrase(
        phrase: &str,
        use_mainnet: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Parse la phrase mnémonique
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)?;
        let seed_full = mnemonic.to_seed("");
        let seed_bytes = &seed_full[0..32];

        // Initialise RNG depuis seed
        let mut rng_seed = [0u8; 32];
        rng_seed.copy_from_slice(seed_bytes);
        let mut rng = ChaCha20Rng::from_seed(rng_seed);

        // Génère la clé de paiement
        let mut sk_bytes_pay = [0u8; 32];
        rng.fill_bytes(&mut sk_bytes_pay);
        let signing_key_pay = SigningKey::from_bytes(&sk_bytes_pay);
        let pubkey_pay = signing_key_pay.verifying_key().to_bytes();
        sk_bytes_pay.zeroize();

        // Génère la clé de staking
        let mut sk_bytes_stake = [0u8; 32];
        rng.fill_bytes(&mut sk_bytes_stake);
        let signing_key_stake = SigningKey::from_bytes(&sk_bytes_stake);
        let pubkey_stake = signing_key_stake.verifying_key().to_bytes();
        sk_bytes_stake.zeroize();

        // Calcule les hash (blake2b‑224) des deux clés publiques
        let mut hasher_pay = Blake2bVar::new(28)?;
        hasher_pay.update(&pubkey_pay);
        let mut payment_hash = vec![0u8; 28];
        hasher_pay.finalize_variable(&mut payment_hash)?;

        let mut hasher_stake = Blake2bVar::new(28)?;
        hasher_stake.update(&pubkey_stake);
        let mut stake_hash = vec![0u8; 28];
        hasher_stake.finalize_variable(&mut stake_hash)?;

        // Construis l’adresse base : header + payment_hash + stake_hash
        let header: u8 = if use_mainnet {
            // type = 0000 (base) + networkid = 1 → bits = 0b0000_0001 = 0x01
            0b0000_0001
        } else {
            // networkid = 0
            0b0000_0000
        };
        let mut addr_bytes = Vec::with_capacity(1 + payment_hash.len() + stake_hash.len());
        addr_bytes.push(header);
        addr_bytes.extend_from_slice(&payment_hash);
        addr_bytes.extend_from_slice(&stake_hash);

        let prefix = if use_mainnet { "addr" } else { "addr_test" };
        let shelley_addr = bech32::encode(prefix, addr_bytes.to_base32(), Variant::Bech32)?;

        info!("🔐 Wallet généré (Shelley base) depuis phrase mnémonique : {}", &shelley_addr);

        // Nous gardons la clé de paiement comme signing_key principal
        Ok(Self {
            signing_key: signing_key_pay,
            address: shelley_addr.clone(),
            mnemonic: Some(phrase.to_string()),
            shelley_addr,
        })
    }

    /// Charge un wallet depuis un fichier clé privée hex
    pub fn load_from_file(
        key_path: impl AsRef<Path>,
        use_mainnet: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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
            shelley_addr: String::new(),
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


    /// Signe un message selon CIP‑8 (COSE_Sign1)
    pub fn sign_cip8(&self, message: &str, external_aad: &[u8]) 
        -> Result<String, Box<dyn std::error::Error + Send + Sync>>
    {
        // 1. Protected header map : alg = EdDSA (-8)
        let protected_map = Value::Map(vec![
            ( Value::Integer(Integer::from(1i64)),     // header key “alg”
              Value::Integer(Integer::from(-8i64)) ),  // EdDSA
            ( Value::Text("address".into()),
              Value::Bytes(self.address_bytes()) ),     // adresse en bytes
        ]);
        let protected_bytes = to_vec(&protected_map)?;                // CBOR serialize
        let protected_bstr = Value::Bytes(protected_bytes.clone());

        // 2. Unprotected header map (vide ou comme souhaité)
        let unprotected_map = Value::Map(vec![]);

        // 3. Compute payload bytes
        let payload_bytes = message.as_bytes();

        // 4. Build Sig_structure = [ context, body_protected, external_aad, payload ]
        let sig_structure = Value::Array(vec![
            Value::Text("Signature1".into()),             // context
            protected_bstr.clone(),
            Value::Bytes(external_aad.to_vec()),
            Value::Bytes(payload_bytes.to_vec()),
        ]);
        let sig_structure_bytes = to_vec(&sig_structure)?;

        // 5. Sign the sig_structure_bytes
        let sig = self.signing_key.sign(&sig_structure_bytes);
        let sig_bytes = sig.to_bytes().to_vec();

        // 6. Build COSE_Sign1 = [ protected_bstr, unprotected_map, payload (bstr), signature (bstr) ]
        let cose_sign1 = Value::Array(vec![
            protected_bstr,
            unprotected_map,
            Value::Bytes(payload_bytes.to_vec()),
            Value::Bytes(sig_bytes.clone()),
        ]);
        let cose_bytes = to_vec(&cose_sign1)?;

        // 7. Return hex‑encoded CBOR signature
        Ok(hex::encode(cose_bytes))
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
    ) -> Result<Vec<Wallet>, Box<dyn std::error::Error + Send + Sync>> {
        let seeds_str = fs::read_to_string(seed_path)?;
        let keys_str = fs::read_to_string(key_path)?;
        let seed_lines: Vec<_> = seeds_str.lines().collect();
        let key_lines: Vec<_> = keys_str.lines().collect();

        let mut wallets = Vec::new();
        for (seed_phrase, _key_hex) in seed_lines.iter().zip(key_lines.iter()) {
            let wallet = Wallet::generate_shelley_base_from_mnemonic_phrase(seed_phrase, use_mainnet)?;
            let mut sk_bytes = [0u8; 32];
            let mnemonic = Mnemonic::parse_in_normalized(Language::English, seed_phrase)?;            
            let seed_full = mnemonic.to_seed("");
            sk_bytes.copy_from_slice(&seed_full[..32]);            
            let signing_key = SigningKey::from_bytes(&sk_bytes);
            let pubkey_bytes = signing_key.verifying_key().to_bytes();
            let addr = Wallet::derive_bech32_address(&pubkey_bytes, use_mainnet);
            wallets.push(Wallet {
                signing_key: wallet.signing_key,
                address: addr,
                mnemonic: Some(seed_phrase.to_string()),
                shelley_addr: wallet.shelley_addr.clone(),
            });
        }

        Ok(wallets)
    }
}
